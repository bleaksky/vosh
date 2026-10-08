//! Lua plugin manager. A plugin is a directory under
//! `<app_data_dir>/plugins/<slug>/` containing a `manifest.toml` and one or
//! more Lua files. The manifest declares which file is the entry point;
//! enabled plugins have their entry script loaded into each session's
//! Lua engine as the session opens its profile.
//!
//! The `[plugins] enabled` list in the profile file says which plugins
//! are on in that profile. The plugins it names load at launch and in
//! each session you open, and a profile switch turns the next profile's
//! plugins on and the others off in the session it switches while you
//! play. The Scripts page in Settings makes, reads and saves a plugin's
//! folder through [`folder`], turns a plugin on or off in a profile and
//! loads it again through [`live`], shows its folder through [`reveal`],
//! and installs and exports a plugin as a .zip through [`archive`].

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{error, info};
use vosh_script::Owner;

use crate::app::state::{AppState, NO_APP_DATA};
use crate::profile::live::Profile;
use crate::script::ApplyResult;
use crate::session::connection::Connection;
use crate::sessions::Session;

pub(crate) mod archive;
pub(crate) mod folder;
pub(crate) mod live;
pub(crate) mod reveal;

#[derive(Debug, Error)]
pub(crate) enum PluginError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest parse: {0}")]
    Manifest(#[from] toml::de::Error),
    #[error("plugin `{0}` not found")]
    NotFound(String),
    #[error("plugin `{0}` entry script `{1}` is missing")]
    EntryMissing(String, String),
    #[error("plugin `{0}` entry script `{1}` is outside its folder")]
    EntryOutside(String, String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PluginManifestFile {
    pub plugin: PluginManifest,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Path to the Lua entry script, relative to the plugin directory.
    #[serde(default = "default_entry")]
    pub entry: String,
}

fn default_entry() -> String {
    "main.lua".to_string()
}

/// The plugins in the plugins folder, as the last look found them. The
/// profile each session plays says which are on, so the list holds only
/// what each manifest says.
#[derive(Debug, Default)]
pub(crate) struct PluginManager {
    plugins_dir: Option<PathBuf>,
    plugins: Vec<PluginManifest>,
}

pub(crate) type SharedPluginManager = Arc<Mutex<PluginManager>>;

impl PluginManager {
    pub(crate) fn set_plugins_dir(&mut self, dir: PathBuf) {
        self.plugins_dir = Some(dir);
    }

    /// Re-scan the plugins directory and update the in-memory list.
    pub(crate) fn discover(&mut self) -> Result<(), PluginError> {
        let Some(dir) = &self.plugins_dir else {
            self.plugins.clear();
            return Ok(());
        };
        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
        }
        let mut found = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("manifest.toml");
            if !manifest_path.exists() {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
                continue;
            };
            let Ok(parsed) = toml::from_str::<PluginManifestFile>(&raw) else {
                continue;
            };
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if dir_name != parsed.plugin.name {
                continue;
            }
            found.push(parsed.plugin);
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        self.plugins = found;
        Ok(())
    }

    /// The plugins the last [`PluginManager::discover`] found, sorted by
    /// name.
    pub(crate) fn list(&self) -> &[PluginManifest] {
        &self.plugins
    }
}

/// A plugin's manifest and entry script as they stand on disk.
#[derive(Debug)]
pub(crate) struct PluginCode {
    /// What manifest.toml says, the entry script's path inside the
    /// plugin folder among it.
    pub(crate) manifest: PluginManifest,
    pub(crate) code: String,
}

impl PluginCode {
    /// The chunk name the plugin `name` runs under, which its errors
    /// name, like `@vitals_alert/main.lua`.
    pub(crate) fn chunk(&self, name: &str) -> String {
        format!("@{name}/{}", self.manifest.entry)
    }
}

/// Read the manifest and the entry script of the plugin `name` in
/// `plugins_dir` as they stand now. The folder must carry the name the
/// manifest gives, as discovery asks, and the entry script must sit
/// inside the folder.
pub(crate) fn read_plugin(
    plugins_dir: &std::path::Path,
    name: &str,
) -> Result<PluginCode, PluginError> {
    let not_found = || PluginError::NotFound(name.to_string());
    // A name from a profile file you edit by hand must not climb out of
    // the plugins folder.
    if !is_one_folder(name) {
        return Err(not_found());
    }
    let dir = plugins_dir.join(name);
    let manifest_path = dir.join("manifest.toml");
    if !manifest_path.is_file() {
        return Err(not_found());
    }
    let manifest =
        toml::from_str::<PluginManifestFile>(&std::fs::read_to_string(manifest_path)?)?.plugin;
    if manifest.name != name {
        return Err(not_found());
    }
    let outside = || PluginError::EntryOutside(name.to_string(), manifest.entry.clone());
    if !entry_stays_inside(&manifest.entry) {
        return Err(outside());
    }
    let entry_path = dir.join(&manifest.entry);
    if !entry_path.is_file() {
        return Err(PluginError::EntryMissing(
            name.to_string(),
            manifest.entry.clone(),
        ));
    }
    // A link in the folder could still point elsewhere.
    let real_dir = dir.canonicalize()?;
    if !entry_path.canonicalize()?.starts_with(&real_dir) {
        return Err(outside());
    }
    Ok(PluginCode {
        code: std::fs::read_to_string(entry_path)?,
        manifest,
    })
}

/// True when `entry`, a manifest's entry script, names a file below the
/// plugin folder by its parts alone, with no `..`, `.` or root. A link
/// on the way can still lead out, which the caller checks on disk.
fn entry_stays_inside(entry: &str) -> bool {
    !entry.is_empty()
        && std::path::Path::new(entry)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

/// True when `name` names one folder, with no separator and no `.` or
/// `..`.
fn is_one_folder(name: &str) -> bool {
    let mut parts = std::path::Path::new(name).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(part)), None) if part == name
    )
}

/// Drop the example plugins shipped with the app into the user's plugins
/// directory if they're not already there, so a fresh install has one
/// to turn on in its profile file.
pub(crate) fn seed_example_plugins(plugins_dir: &std::path::Path) {
    const EXAMPLES: &[(&str, &[(&str, &str)])] = &[(
        "vitals_alert",
        &[
            (
                "manifest.toml",
                include_str!("../../../plugins/vitals_alert/manifest.toml"),
            ),
            (
                "main.lua",
                include_str!("../../../plugins/vitals_alert/main.lua"),
            ),
        ],
    )];
    for (name, files) in EXAMPLES {
        let dir = plugins_dir.join(name);
        if dir.exists() {
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            error!(plugin = %name, error = %e, "failed to seed plugin directory");
            continue;
        }
        for (filename, contents) in *files {
            let path = dir.join(filename);
            if let Err(e) = std::fs::write(&path, contents) {
                error!(plugin = %name, file = %filename, error = %e, "failed to seed plugin file");
            }
        }
        info!(plugin = %name, "seeded example plugin");
    }
}

/// The plugins folder, or the error a command returns when launch could
/// not find the app data folder.
pub(crate) fn plugins_dir_of(state: &AppState) -> Result<PathBuf, String> {
    state
        .app_data
        .get()
        .map(|app_data| crate::disk::paths::plugins_dir(app_data))
        .ok_or_else(|| NO_APP_DATA.to_string())
}

/// Turn the plugin `name` on, or load it again: read it from
/// `plugins_dir` as it stands and load it. Returns what it asks of the
/// session. When Vosh cannot read it, a red `[lua]` line says so, and
/// the plugin waits for `#script reload` as one whose Lua failed does.
pub(crate) fn plugin_on(
    p: &mut Profile,
    c: &mut Connection,
    plugins_dir: &std::path::Path,
    name: &str,
) -> ApplyResult {
    match read_plugin(plugins_dir, name) {
        Ok(plugin) => load_plugin(p, c, name, &plugin),
        Err(e) => left_off(p, c, name, &e),
    }
}

/// Say in a red `[lua]` line that Vosh could not read the plugin `name`,
/// for `e`, and list it for `#script reload`, which tries it again.
fn left_off(p: &mut Profile, c: &mut Connection, name: &str, e: &PluginError) -> ApplyResult {
    error!(name = %name, error = %e, "plugin entry missing");
    c.script.list_unread_plugin(name);
    let outcome = vosh_script::ScriptOutcome {
        actions: vec![vosh_script::Action::Error {
            owner: Owner::Plugin(name.to_string()),
            text: format!("Vosh could not read plugin {name} and left it off."),
            at: None,
        }],
        failed: true,
        ..vosh_script::ScriptOutcome::default()
    };
    crate::script::apply_actions(p, c, outcome)
}

/// Load `plugin`, the plugin `name` as Vosh just read it, which takes the
/// place of what it ran before and clears a stop.
fn load_plugin(
    p: &mut Profile,
    c: &mut Connection,
    name: &str,
    plugin: &PluginCode,
) -> ApplyResult {
    // A load runs Lua for certain, even when nothing else is loaded,
    // as at a switch that turned every other plugin off first.
    crate::script::refresh_vars(p, c);
    let owner = Owner::Plugin(name.to_string());
    c.lua_output.note_load(&owner, crate::session::now_ms());
    let outcome = c
        .script
        .load_script(owner, &plugin.chunk(name), &plugin.code);
    if outcome.failed {
        error!(name = %name, "plugin script error");
    } else {
        info!(name = %name, "loaded plugin");
    }
    crate::script::apply_actions(p, c, outcome)
}

/// Turn the plugin `name` off: take back its Lua triggers, GMCP
/// handlers, timers and aliases. The variables it set and the groups it
/// turned on or off stay.
pub(crate) fn plugin_off(p: &mut Profile, c: &mut Connection, name: &str) -> ApplyResult {
    let outcome = c.script.unload(&Owner::Plugin(name.to_string()));
    info!(name = %name, "unloaded plugin");
    crate::script::apply_actions(p, c, outcome)
}

/// Turn off each plugin that runs and the live profile does not turn
/// on, then turn on each one it turns on that does not run yet, in the
/// order its list gives. A plugin both profiles turn on keeps running as
/// it is, a stopped one included. A plugin Vosh stopped stays off, so a
/// switch back to a profile that turns it on leaves it off until you
/// save it or restart Vosh.
pub(crate) fn follow_profile_plugins(
    p: &mut Profile,
    c: &mut Connection,
    plugins_dir: &std::path::Path,
) -> ApplyResult {
    let mut wanted: Vec<String> = Vec::new();
    for name in &p.plugins.enabled {
        if !wanted.contains(name) {
            wanted.push(name.clone());
        }
    }
    let running = c.script.loaded_plugins();
    let mut apply = ApplyResult::default();
    for name in running.iter().filter(|name| !wanted.contains(name)) {
        apply.append(plugin_off(p, c, name));
    }
    for name in wanted.iter().filter(|name| !running.contains(name)) {
        if c.script.is_stopped(&Owner::Plugin(name.clone())) {
            continue;
        }
        apply.append(plugin_on(p, c, plugins_dir, name));
    }
    apply
}

/// Once a profile switch made the next profile live for `session` and
/// turned its plugins on and the others off, deliver what they ask for,
/// `apply`. Their lines print in the terminal, and what they send goes to
/// the game when the session runs a connection. Every window then hears
/// that the plugins changed, so the Scripts page reads them again.
pub(crate) async fn follow_profile<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &Arc<Session>,
    apply: ApplyResult,
) {
    crate::session::effects::deliver_detached(app, session, apply).await;
    // The session runs other plugins now, and its profile turns others on.
    crate::app::events::broadcast(app, crate::app::events::PLUGINS_CHANGED, &());
}

/// Load each plugin in `plugins_dir` the profile turns on into the
/// engine of `session`, once each in the order its list gives, as a
/// switch does from a start with none running. Launch calls
/// it for the session the app starts with, and `session_open` for each
/// session you open. What an entry script asks for applies as on every
/// other path that runs Lua, so its timers, `mud.input` lines and prompt
/// values take effect. No terminal shows the session and no game listens
/// yet, so the lines it prints wait for [`show_launch_lines`], and what
/// it would send goes to the log.
pub(crate) async fn load_enabled_plugins<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &Arc<Session>,
    plugins_dir: std::path::PathBuf,
) {
    let apply = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        follow_profile_plugins(&mut p, &mut c, &plugins_dir).ran_under(p.open())
    };
    let collected = crate::session::effects::collect_script_result(app, session, apply).await;
    if !collected.bytes.is_empty() || collected.walk.is_some() {
        info!(
            bytes = collected.bytes.len(),
            walk = collected.walk.is_some(),
            "plugin output before a connect has no game to go to"
        );
    }
    session
        .launch_lua_lines
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(collected.echoes);
}

/// Print the lines the plugins printed as they loaded into `session`, at
/// launch or as it opened, once, now that a terminal listens. The first connect and the
/// first line you type each call it, and whichever comes first prints
/// them.
pub(crate) fn show_launch_lines<R: tauri::Runtime>(app: &tauri::AppHandle<R>, session: &Session) {
    let lines = std::mem::take(
        &mut *session
            .launch_lua_lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    crate::output::echo_lines(app, session, &lines);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    fn write_plugin(root: &Path, slug: &str, manifest_name: &str, entry_body: &str) {
        let dir = root.join(slug);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = format!(
            "[plugin]\nname = \"{manifest_name}\"\nversion = \"0.1.0\"\ndescription = \"x\"\nauthor = \"y\"\nentry = \"main.lua\"\n"
        );
        std::fs::write(dir.join("manifest.toml"), manifest).unwrap();
        let mut f = std::fs::File::create(dir.join("main.lua")).unwrap();
        f.write_all(entry_body.as_bytes()).unwrap();
    }

    #[test]
    fn discover_picks_up_well_formed_plugin() {
        let tmp = tempdir();
        write_plugin(tmp.path(), "alpha", "alpha", "-- hello\n");
        let mut mgr = PluginManager::default();
        mgr.set_plugins_dir(tmp.path().to_path_buf());
        mgr.discover().unwrap();
        assert_eq!(mgr.list().len(), 1);
        assert_eq!(mgr.list()[0].name, "alpha");
    }

    #[test]
    fn discover_skips_when_dir_name_does_not_match_manifest() {
        let tmp = tempdir();
        write_plugin(tmp.path(), "wrong_dir", "actual_name", "");
        let mut mgr = PluginManager::default();
        mgr.set_plugins_dir(tmp.path().to_path_buf());
        mgr.discover().unwrap();
        assert!(mgr.list().is_empty());
    }

    #[test]
    fn read_plugin_returns_the_entry_script_as_it_stands() {
        let tmp = tempdir();
        write_plugin(tmp.path(), "p", "p", "print('hi')");
        let plugin = read_plugin(tmp.path(), "p").unwrap();
        assert_eq!(plugin.code, "print('hi')");
        assert_eq!(plugin.chunk("p"), "@p/main.lua");
        std::fs::write(tmp.path().join("p").join("main.lua"), "print('again')").unwrap();
        assert_eq!(read_plugin(tmp.path(), "p").unwrap().code, "print('again')");
    }

    #[test]
    fn read_plugin_stays_inside_the_plugin_folder() {
        let tmp = tempdir();
        let plugins = tmp.path().join("plugins");
        write_plugin(&plugins, "p", "p", "");
        std::fs::write(tmp.path().join("outside.lua"), "mud.send('look')").unwrap();
        // A name that climbs out of the plugins folder finds nothing.
        for name in ["..", "../plugins/p", "p/.", ""] {
            assert!(
                matches!(read_plugin(&plugins, name), Err(PluginError::NotFound(_))),
                "{name}"
            );
        }
        // Nor does a folder named unlike its manifest.
        write_plugin(&plugins, "q", "other", "");
        assert!(matches!(
            read_plugin(&plugins, "q"),
            Err(PluginError::NotFound(_))
        ));
        // An entry outside the folder is refused.
        let manifest = plugins.join("p").join("manifest.toml");
        for entry in ["../../outside.lua", "/etc/hosts", "."] {
            std::fs::write(
                &manifest,
                format!("[plugin]\nname = \"p\"\nentry = \"{entry}\"\n"),
            )
            .unwrap();
            assert!(
                matches!(
                    read_plugin(&plugins, "p"),
                    Err(PluginError::EntryOutside(..))
                ),
                "{entry}"
            );
        }
        // So is a link that leads out.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                tmp.path().join("outside.lua"),
                plugins.join("p").join("link.lua"),
            )
            .unwrap();
            std::fs::write(&manifest, "[plugin]\nname = \"p\"\nentry = \"link.lua\"\n").unwrap();
            assert!(matches!(
                read_plugin(&plugins, "p"),
                Err(PluginError::EntryOutside(..))
            ));
        }
    }

    #[test]
    fn a_profile_switch_turns_its_plugins_on_and_the_others_off() {
        let tmp = tempdir();
        write_plugin(
            tmp.path(),
            "everywhere",
            "everywhere",
            "mud.alias('ev', 'look') mud.timer(600, function() end)",
        );
        write_plugin(
            tmp.path(),
            "healer_only",
            "healer_only",
            "mud.alias('hl', 'cast heal')",
        );
        write_plugin(
            tmp.path(),
            "warrior_only",
            "warrior_only",
            "mud.trigger('hunger', 'You are hungry', function() end) \
             mud.timer(600, function() end)",
        );
        let mut p = Profile::default();
        let mut c = Connection::default();
        p.plugins.enabled = vec!["everywhere".into(), "warrior_only".into()];
        let launch = follow_profile_plugins(&mut p, &mut c, tmp.path());
        assert_eq!(launch.new_timers.len(), 2);
        let warrior_timer = launch.new_timers[1].timer_id;
        // The next profile lists one plugin twice and one it does not
        // have.
        p.plugins.enabled = vec![
            "healer_only".into(),
            "everywhere".into(),
            "healer_only".into(),
            "missing".into(),
        ];
        let switched = follow_profile_plugins(&mut p, &mut c, tmp.path());
        // The one it does not have says so, and a reload would try it.
        assert_eq!(
            switched.echoes,
            ["\x1b[90m[lua]\x1b[0m \x1b[31mVosh could not read plugin missing and left it off.\x1b[0m"]
        );
        assert_eq!(
            c.script.loaded_plugins(),
            ["everywhere", "healer_only", "missing"]
        );
        let leftover = &c.script.lua_triggers();
        assert!(leftover.is_empty(), "{leftover:?}");
        // Only the plugin that went off lost its timer, and the one both
        // turn on did not load again.
        assert_eq!(switched.cancel_timers, [warrior_timer]);
        let leftover = &switched.new_timers;
        assert!(leftover.is_empty(), "{leftover:?}");
        let aliases: Vec<(&str, &str)> = c
            .plugin_aliases
            .list()
            .into_iter()
            .map(|(by, alias)| (by, alias.name.as_str()))
            .collect();
        assert_eq!(aliases, [("everywhere", "ev"), ("healer_only", "hl")]);
        // Back again, the healer's alias goes with it.
        p.plugins.enabled = vec!["everywhere".into()];
        follow_profile_plugins(&mut p, &mut c, tmp.path());
        let aliases: Vec<&str> = c
            .plugin_aliases
            .list()
            .into_iter()
            .map(|(by, _)| by)
            .collect();
        assert_eq!(aliases, ["everywhere"]);
    }

    #[test]
    fn a_stopped_plugin_stays_off_across_a_switch_and_back() {
        let tmp = tempdir();
        write_plugin(
            tmp.path(),
            "runaway",
            "runaway",
            "mud.on_gmcp('Char.Vitals', function() while true do end end) \
             mud.alias('ra', 'look')",
        );
        let mut p = Profile::default();
        let mut c = Connection::default();
        p.plugins.enabled = vec!["runaway".into()];
        follow_profile_plugins(&mut p, &mut c, tmp.path());
        let stopped = c
            .script
            .dispatch_gmcp("Char.Vitals", &serde_json::json!({}));
        assert_eq!(stopped.stopped, [Owner::Plugin("runaway".into())]);
        crate::script::apply_actions(&mut p, &mut c, stopped);
        // Away and back again, it stays off with nothing registered.
        p.plugins.enabled = Vec::new();
        follow_profile_plugins(&mut p, &mut c, tmp.path());
        p.plugins.enabled = vec!["runaway".into()];
        let back = follow_profile_plugins(&mut p, &mut c, tmp.path());
        let leftover = &back.echoes;
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &c.script.loaded_plugins();
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(c.script.is_stopped(&Owner::Plugin("runaway".into())));
        let leftover = &c.plugin_aliases.list();
        assert!(leftover.is_empty(), "{leftover:?}");
        let quiet = c
            .script
            .dispatch_gmcp("Char.Vitals", &serde_json::json!({}));
        assert!(quiet.actions.is_empty(), "{:?}", quiet.actions);
    }

    /// `wait_full`, a plugin whose loop never ends, which never returns
    /// while you are hurt.
    const WAIT_FULL: &str = "-- wait_full
-- Stand up once your hit points are full.

mud.on_gmcp(\"Char.Vitals\", function(data)
  while data.hp < data.maxhp do
    -- data never changes inside this loop, so it never ends
  end
  mud.send(\"stand\")
end)
";

    #[test]
    fn a_plugin_stop_prints_as_the_terminal_frame_draws_it() {
        let tmp = tempdir();
        write_plugin(tmp.path(), "wait_full", "wait_full", WAIT_FULL);
        let mut p = Profile::default();
        let mut c = Connection::default();
        p.plugins.enabled = vec!["wait_full".into()];
        follow_profile_plugins(&mut p, &mut c, tmp.path());
        let stopped = c.script.dispatch_gmcp(
            "Char.Vitals",
            &serde_json::json!({"hp": 186, "maxhp": 1020}),
        );
        let apply = crate::script::apply_actions(&mut p, &mut c, stopped);
        // The stop reads red, and how long the plugin stays off plain.
        assert_eq!(
            apply.echoes,
            [
                "\x1b[90m[lua]\x1b[0m \x1b[31mVosh stopped wait_full at main.lua line 5 after 100 ms.\x1b[0m",
                "\x1b[90m[lua]\x1b[0m wait_full stays off until you save it under Scripts in Settings or restart Vosh.",
            ]
        );
    }

    #[test]
    fn a_plugin_reads_the_variables_of_the_profile_that_turns_it_on() {
        let tmp = tempdir();
        for name in ["first_side", "second_side"] {
            write_plugin(
                tmp.path(),
                name,
                name,
                "mud.echo(tostring(mud.var('home')))",
            );
        }
        let mut p = Profile::default();
        let mut c = Connection::default();
        p.vars.set("home", "first");
        p.plugins.enabled = vec!["first_side".into()];
        let launch = follow_profile_plugins(&mut p, &mut c, tmp.path());
        assert_eq!(launch.echoes, ["first"]);
        // The switch lays the next profile over this one, and no other
        // Lua stays loaded once its one plugin turns off.
        p.vars.set("home", "second");
        p.plugins.enabled = vec!["second_side".into()];
        let switched = follow_profile_plugins(&mut p, &mut c, tmp.path());
        assert_eq!(switched.echoes, ["second"]);
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }
}
