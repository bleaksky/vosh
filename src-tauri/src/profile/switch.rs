//! Switching the active profile, when you pick one and when a login
//! names a character another profile claims. A switch saves the profile
//! you leave, reads the next one's file and global.toml, and lays them
//! over the live profile. The connection carries on as it was, apart
//! from the tick settings and the prompt table the next profile hands
//! it. The same read of global.toml keeps your shared settings across a
//! `#profile reset` or `#profile load`.

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tracing::warn;

use crate::app::events::{broadcast, broadcast_profile_ui, PROFILE_SWITCHED};
use crate::app::state::SharedState;
use crate::disk::save::{persist_state, PERSIST_LOCK};
use crate::loadouts::catalog::lay_catalog_over;
use crate::output;
use crate::profile::file::ProfileConfig;
use crate::profile::live::Profile;
use crate::profile::shared::{GlobalConfig, SharedLayer};
use crate::script::ApplyResult;
use crate::session::connection::Connection;
use crate::sessions::Session;
use crate::tick::TickConfig;

/// The files a switch to a profile loads: its own file, None for a
/// profile that never saved one, and global.toml, None before the first
/// save.
struct SwitchFiles {
    per_profile: Option<ProfileConfig>,
    global: Option<GlobalConfig>,
}

/// Read the files a switch to `name` loads, and only then point the
/// index at it. A file that does not read changes nothing, so the index
/// keeps naming the profile the live state holds and the next persist
/// still writes that profile to its own file.
fn open_profile_for_switch(
    set: &mut crate::profile::set::ProfileSet,
    name: &str,
) -> Result<SwitchFiles, String> {
    use crate::profile::set::{display_name, ProfileSetError};
    if set.get(name).is_none() {
        return Err(ProfileSetError::NotFound(name.to_string()).to_string());
    }
    let refused = |what: &str| {
        format!(
            "Vosh could not open the {} profile because it could not read {what}. You are \
             still using the {} profile.",
            display_name(name),
            display_name(set.active_name()),
        )
    };
    let path = set.profile_path(name);
    let per_profile = if path.exists() {
        match ProfileConfig::load(&path) {
            Ok(config) => Some(config),
            Err(e) => {
                warn!(error = %e, path = %path.display(), "profile file unreadable at switch");
                return Err(refused("the profile file"));
            }
        }
    } else {
        None
    };
    let global_path = set.global_path();
    // Only the categories the scope shares, so a value global.toml still
    // holds from before cannot cover the one the profile file owns.
    let global = match GlobalConfig::load_shared(&global_path, set.scope()) {
        Ok(config) => config,
        Err(e) => {
            warn!(error = %e, path = %global_path.display(), "global config unreadable at switch");
            return Err(refused("global.toml, which holds your shared settings"));
        }
    };
    let leaving = set.active_path();
    set.switch(name).map_err(|e| e.to_string())?;
    // Both files read, and the live profile is about to hold what they
    // say, so the saves may write them again. The file of the profile you
    // left no longer stands behind the live profile, and every other write
    // to it reads it first, so a file that did not read at launch is safe
    // from here on.
    for path in [&leaving, &path, &global_path] {
        crate::disk::atomic::release_unread(path);
    }
    Ok(SwitchFiles {
        per_profile,
        global,
    })
}

/// Steps 2 and 3 of a switch, after the flush of the outgoing profile.
/// Call with [`PERSIST_LOCK`] held. Loads the incoming profile's file
/// and global.toml, points the index at it, then lays both over the
/// live profile, and in loadout mode the catalog and the loadouts too.
/// Either every step lands or none does, and a save that waits on the
/// lock finds the live profile whole. The connection then takes the
/// incoming profile's tick settings and `[prompt]` table and keeps the
/// rest as it was, and its prompt drops the values the last profile's
/// prompt read. The plugins the incoming profile turns on start and the
/// others stop in the same step, and what they ask for comes back for
/// the caller to deliver once the lock drops.
pub(crate) async fn switch_live_profile(
    state: &SharedState,
    session: &Session,
    name: &str,
) -> Result<ApplyResult, String> {
    // Step 2: read the incoming files, then flip the active pointer in
    // the index.
    let SwitchFiles {
        per_profile,
        global,
    } = {
        let mut set = state.loaded_profile_set().await?;
        open_profile_for_switch(&mut set, name)?
    };

    // In loadout mode the catalog holds the aliases, triggers, and
    // macros, so it fills the stores in the same step. Were a save to
    // find the stores empty in between, it would write an empty catalog.
    let catalog = state.global_catalog.lock().await.clone();
    let loadouts = state.loadout_set.lock().await.clone();

    // Step 3: lay the per-profile file (or defaults) over the live
    // profile, then global.toml so theme/font/keep-last/auto-update/
    // dock_layout survive the switch, then the catalog. The connection
    // lock comes after these, so a command that reads only the connection
    // never waits while the files apply.
    {
        let mut p = state.profile.lock().await;
        p.display_name = Some(crate::profile::set::display_name(name));
        state.note_active_profile(name);
        let tick_before = p.tick.config.clone();
        match per_profile {
            Some(snap) => {
                snap.apply_to(&mut p);
            }
            None => {
                // A profile that never saved a file is fresh.
                let fresh = ProfileConfig::fresh();
                fresh.apply_to(&mut p);
            }
        }
        if let Some(g) = global {
            g.apply_to(&mut p);
        }
        if let Some(catalog) = &catalog {
            lay_catalog_over(&mut p, catalog, loadouts.as_ref());
        }
        // Under the same lock as the swap, so a pane layout write edited
        // from the old profile's tree, or a whole config save read from
        // the old profile, is refused from here on.
        state.note_ui_config_replaced();

        // The connection did not change, so it keeps what it holds and
        // takes only the next profile's tick settings and [prompt] table.
        // The values the last profile's prompt read go first, since they
        // came from its capture and its scripts.
        let mut c = session.connection.lock();
        c.prompt.switch_profile();
        hand_to_connection(&mut p, &mut c, &tick_before);
        // The latest Char.Prompt of the connection applies to the new
        // profile's capture by the rule every packet follows, and the
        // profile keeps the table as it then stands.
        let before = c.prompt.revision();
        c.prompt.follow_latest(chrono::Local::now().fixed_offset());
        crate::prompt::keep_table(&mut p, &c, before);
        // Under both locks, so no plugin of the profile you left answers
        // a line or a packet for the next one.
        let plugins = match state.app_data.get() {
            Some(app_data) => crate::app::plugins::follow_profile_plugins(
                &mut p,
                &mut c,
                &crate::disk::paths::plugins_dir(app_data),
            ),
            None => ApplyResult::default(),
        };
        Ok(plugins)
    }
}

/// Hand the connection `c` the two things it takes from the profile that
/// a switch, `#profile load`, `#profile reset` or launch just laid over
/// `p`: the tick settings and the `[prompt]` table. The tick keeps its
/// count under the new settings, which replaced `tick_before`, so the
/// status line counts on from the last tick. The prompt engine takes the
/// table, and the profile keeps it as the engine then holds it.
pub(crate) fn hand_to_connection(p: &mut Profile, c: &mut Connection, tick_before: &TickConfig) {
    c.tick
        .adopt(&mut p.tick, tick_before, tokio::time::Instant::now());
    let table = p.prompt.clone();
    crate::prompt::take_config(p, c, table);
}

/// Shared body for switching the active profile, for `session`. The
/// `profile_switch` Tauri command and the Char.Status auto-switch
/// path in `auto_switch_for_character` both call this so the
/// persist + load + flip sequence stays identical. An error is a
/// sentence for you, and leaves the index and the live profile on the
/// profile you were using.
pub(crate) async fn apply_profile_switch<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    name: &str,
) -> Result<(), String> {
    let plugins = switch_profile(state, session, name).await?;
    // The new profile's capture took the game's latest prompt settings.
    let seen = session.connection.lock().prompt.take_seen();
    crate::prompt::report_game_prompt_seen(app, seen);

    // Hand every window the new profile's panes, tracked affects, tick
    // settings, and chip style from here, then the replace notice, on
    // which the main window reads the config again and sends every
    // window the rest. These go out before profile-switched so the
    // stores already hold the new values when windows react to the
    // switch.
    broadcast_profile_ui(app, state).await;

    broadcast(app, PROFILE_SWITCHED, &name);

    // What the plugins this profile turned on and the others asked for
    // as the switch made it live.
    crate::app::plugins::follow_profile(app, state, session, plugins).await;
    Ok(())
}

/// Why a profile cannot switch between `migration_apply` and the relaunch
/// that finishes it. The next profile's file holds no aliases, triggers,
/// or macros any more, and the shared catalog that holds them loads only
/// at launch, so the switch would leave you with none.
const SWITCH_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then switch profiles.";

/// Steps 1 to 3 of [`apply_profile_switch`], which need no app handle,
/// so a test can run them. Returns what the plugins the switch turned on
/// and off ask for.
pub(crate) async fn switch_profile(
    state: &SharedState,
    session: &Session,
    name: &str,
) -> Result<ApplyResult, String> {
    // Hold the persist lock from the flush through loading the next
    // file, so a Settings write to the incoming profile's file lands
    // either before the load reads it or after the switch made the
    // profile live, never in between.
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if state
        .relaunch_pending
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(SWITCH_MIGRATION_PENDING.into());
    }

    // Step 1: snapshot + write the CURRENT active profile so user
    // changes since the last persist are not lost on switch. Skipped
    // after a #profile reset/load: the live profile is deliberately
    // diverged from disk and a passive switch (the GMCP Char.Status
    // auto-switch reaches here too) must not write it back.
    if !state
        .auto_persist_suppressed
        .load(std::sync::atomic::Ordering::Acquire)
    {
        persist_state(state).await;
    }

    switch_live_profile(state, session, name).await
}

/// Switch to the profile that claims `character` on the connection
/// `session` runs, when that is not the active one already.
pub(crate) async fn auto_switch_for_character<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    character: &str,
) {
    let Some(new_name) = auto_switch_target(state, session, character).await else {
        return;
    };
    // A switch that fails leaves the live profile and the index as they
    // were, and says so on the terminal, since nothing else would tell
    // you the login kept the old profile.
    let line = match apply_profile_switch(app, state, session, &new_name).await {
        Ok(()) => auto_switch_line(&new_name),
        Err(e) => {
            warn!(error = %e, "auto profile switch failed");
            auto_switch_failed_line(&e)
        }
    };
    output::emit_output(app, line.into_bytes());
}

/// The profile that `character` logging in on the connection `session`
/// runs should load, when that is not the active one already.
async fn auto_switch_target(
    state: &SharedState,
    session: &Session,
    character: &str,
) -> Option<String> {
    let (host, port) = session
        .current_connection
        .lock()
        .ok()
        .and_then(|g| g.clone())?;
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref()?;
    set.resolve_match(&host, port, Some(character))
        .filter(|name| name != set.active_name())
}

/// The terminal line that says a login switched the profile, in the
/// yellow that tick warnings use.
fn auto_switch_line(profile: &str) -> String {
    format!(
        "\r\n\x1b[33mVosh switched to the {} profile.\x1b[0m\r\n",
        crate::profile::set::display_name(profile)
    )
}

/// The terminal line that says a login switch did not happen, in the
/// same yellow. `error` is the sentence the switch returned.
pub(crate) fn auto_switch_failed_line(error: &str) -> String {
    format!("\r\n\x1b[33m{error}\x1b[0m\r\n")
}

/// global.toml as a switch reads it, for `#profile reset` and `#profile
/// load` to lay back over the config they swap in. Holds the persist
/// lock for the read, so a save cannot move the file aside midway. None
/// before startup loads the profile set.
pub(crate) async fn read_shared_layer(state: &SharedState) -> Option<SharedLayer> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref()?;
    Some(SharedLayer::read(&set.global_path(), *set.scope()))
}

/// [`read_shared_layer`] for a path other than typed input, when one of
/// `lines` is a `#profile reset` or `#profile load` that acts. A timer,
/// the tick auto-fire command, and `mud.input` then keep the shared
/// settings across it the way typed input does. Call before taking the
/// profile lock.
pub(crate) async fn shared_layer_for_lines<'a, R: tauri::Runtime>(
    app: &AppHandle<R>,
    lines: impl IntoIterator<Item = &'a str>,
) -> Option<SharedLayer> {
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let replaces = lines
        .into_iter()
        .any(|line| crate::input::may_replace_profile(&state, line));
    if !replaces {
        return None;
    }
    read_shared_layer(&state).await
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::app::state::AppState;
    use crate::disk::save::tests::{affect, launch_state, live_affects, persist, read, UNREADABLE};
    use crate::profile::file::ProfileConfig;
    use crate::profile::set::{ProfileSet, DEFAULT_PROFILE_NAME};
    use crate::profile::tests::james_like_set;

    #[test]
    fn auto_switch_line_names_the_profile_in_a_sentence() {
        assert_eq!(
            super::auto_switch_line("Ilsabet"),
            "\r\n\x1b[33mVosh switched to the Ilsabet profile.\x1b[0m\r\n"
        );
        assert_eq!(
            super::auto_switch_line("default"),
            "\r\n\x1b[33mVosh switched to the Default profile.\x1b[0m\r\n"
        );
    }

    /// App state over James's profile set in `dir`, with `default` live
    /// and tracking Sanctuary.
    async fn switch_state(dir: &std::path::Path) -> super::SharedState {
        let state: super::SharedState = std::sync::Arc::new(AppState::default());
        state.app_data.set(dir.to_path_buf()).unwrap();
        state.profile.lock().await.ui.tracked_affects = vec![affect("Sanctuary")];
        *state.profile_set.lock().await = Some(james_like_set(dir));
        state
    }

    pub(crate) async fn active(state: &super::SharedState) -> String {
        let guard = state.profile_set.lock().await;
        guard.as_ref().unwrap().active_name().to_string()
    }

    fn healer_file(dir: &std::path::Path) -> std::path::PathBuf {
        ProfileSet::load_or_migrate(dir.to_path_buf())
            .unwrap()
            .profile_path("Healer")
    }

    #[tokio::test]
    async fn a_switch_loads_the_named_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        let mut config = ProfileConfig::default();
        config.ui.tracked_affects = vec![affect("Haste")];
        config.save(&healer_file(dir.path())).unwrap();

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        assert_eq!(active(&state).await, "Healer");
        // The events that name the active profile name the new one.
        assert_eq!(state.active_profile().as_deref(), Some("Healer"));
        assert_eq!(live_affects(&state).await, ["Haste"]);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), "Healer");
    }

    #[tokio::test]
    async fn a_switch_turns_the_plugins_over_with_the_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        let plugins = crate::disk::paths::plugins_dir(dir.path());
        for name in ["everywhere", "healer_only", "default_only"] {
            let plugin = plugins.join(name);
            std::fs::create_dir_all(&plugin).unwrap();
            std::fs::write(
                plugin.join("manifest.toml"),
                format!("[plugin]\nname = \"{name}\"\n"),
            )
            .unwrap();
            std::fs::write(plugin.join("main.lua"), format!("mud.echo('{name} on')")).unwrap();
        }
        {
            let mut p = state.profile.lock().await;
            let mut c = session.connection.lock();
            p.plugins.enabled = vec!["default_only".into(), "everywhere".into()];
            crate::app::plugins::follow_profile_plugins(&mut p, &mut c, &plugins);
        }
        let mut config = ProfileConfig::default();
        config.plugins.enabled = vec!["everywhere".into(), "healer_only".into()];
        config.save(&healer_file(dir.path())).unwrap();

        let apply = super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        // Once the swap lets go of the profile, the plugins of the one you
        // left are off, and what the new one printed waits to show.
        assert_eq!(
            state.profile.lock().await.script.loaded_plugins(),
            ["everywhere", "healer_only"]
        );
        assert_eq!(apply.echoes, ["healer_only on"]);
    }

    #[tokio::test]
    async fn a_switch_keeps_the_gmcp_packets_and_drops_the_values_triggers_set() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        *session.current_connection.lock().unwrap() =
            Some(("play.theforsakenlands.com".into(), 1848));
        {
            let mut c = session.connection.lock();
            c.prompt.connect(true);
            let at = chrono::Local::now().fixed_offset();
            c.prompt.vars.observe(
                "Char.Prompt",
                serde_json::json!({"enabled":true,"prompt":"%n%P%C<%hhp %mm %vmv> ","fprompt":""}),
                at,
            );
            c.prompt
                .vars
                .observe("Char.Vitals", serde_json::json!({"hp":850,"maxhp":900}), at);
            c.prompt.vars.set_script("hp", "840");
            c.prompt.vars.set_script("mood", "grim");
            assert_eq!(c.prompt.vars.prompt_vars().len(), 2);
        }

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();

        let c = session.connection.lock();
        assert!(c.prompt.forsaken());
        assert!(
            c.prompt.vars.new_build(),
            "the Char.Prompt of this connection stays"
        );
        assert!(c.prompt.vars.gmcp().get("Char.Vitals").is_some());
        assert!(c.prompt.vars.prompt_vars().is_empty());
    }

    #[tokio::test]
    async fn a_switch_leaves_the_connection_as_it_was_but_drops_the_prompt_values() {
        use crate::session::connection::{QuickKey, RoomChar};
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        // Healer saved the tick off.
        let mut config = ProfileConfig::default();
        config.tick.enabled = false;
        config.save(&healer_file(dir.path())).unwrap();
        let goblin = vec![RoomChar {
            name: "a goblin".into(),
            npc: true,
        }];
        let gg = QuickKey {
            name: "gg".into(),
            verb: "kill".into(),
        };
        let status = serde_json::json!({"level": 60});
        let char_state = serde_json::json!({"language": ""});
        let vitals = serde_json::json!({"hp": 850, "maxhp": 900});
        let (look, count, who, pulse) = {
            let mut p = state.profile.lock().await;
            let mut c = session.connection.lock();
            c.target.name = Some("goblin".into());
            c.target.room_idx = Some(1);
            c.target.quick_keys = vec![gg.clone()];
            c.room_chars = goblin.clone();
            c.room_block.room_chars(1);
            c.fight_tail = true;
            let t0 = tokio::time::Instant::now();
            c.tick.start_session(&mut p.tick, t0);
            assert!(
                c.tick.on_game_tick(&p.tick, t0).is_some(),
                "the game ticked"
            );
            c.prompt.connect(true);
            let at = chrono::Local::now().fixed_offset();
            c.prompt.observe("Char.Status", status.clone(), at);
            c.prompt.observe("Char.State", char_state.clone(), at);
            c.prompt.observe("Char.Vitals", vitals.clone(), at);
            c.prompt.vars.set_script("mood", "grim");
            assert!(!c.prompt.vars.prompt_vars().is_empty());
            (
                c.room_block.clone(),
                (c.tick.last_tick, c.tick.last_signal),
                c.prompt.who(),
                c.prompt.vars.gmcp().pulse(),
            )
        };
        assert_ne!(look, crate::session::room_block::RoomBlock::default());
        assert_ne!(
            who,
            vosh_prompt::aabahran::Who::default(),
            "an immortal in a mobile"
        );

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();

        let p = state.profile.lock().await;
        let c = session.connection.lock();
        assert_eq!(c.target.name.as_deref(), Some("goblin"));
        assert_eq!(c.target.room_idx, Some(1));
        assert_eq!(c.target.quick_keys, [gg]);
        assert_eq!(c.room_chars, goblin);
        assert_eq!(c.room_block, look);
        assert!(c.fight_tail);
        assert!(p.tick.config.enabled, "a running tick stays on");
        assert!(c.tick.synced);
        assert_eq!((c.tick.last_tick, c.tick.last_signal), count);
        let gmcp = c.prompt.vars.gmcp();
        assert_eq!(gmcp.get("Char.Status"), Some(&status));
        assert_eq!(gmcp.get("Char.State"), Some(&char_state));
        assert_eq!(gmcp.get("Char.Vitals"), Some(&vitals));
        assert_eq!(gmcp.pulse(), pulse);
        assert_eq!(c.prompt.who(), who);
        let left = c.prompt.vars.prompt_vars();
        assert!(left.is_empty(), "{left:?}");
    }

    #[tokio::test]
    async fn a_switch_hands_the_prompt_the_next_profile_table_and_its_rules() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        {
            let mut p = state.profile.lock().await;
            let mut c = session.connection.lock();
            // A world Vosh does not know, where no Forsaken Lands rule
            // holds until a capture reads Aabahran's codes.
            c.prompt.connect(false);
            crate::prompt::take_config(
                &mut p,
                &mut c,
                vosh_prompt::PromptConfig::from_legacy(true, "%hp"),
            );
            let at = chrono::Local::now().fixed_offset();
            c.prompt
                .vars
                .observe("Char.Vitals", serde_json::json!({"hp":850,"maxhp":900}), at);
            c.prompt.vars.set_script("mood", "grim");
            assert!(!c.prompt.forsaken());
        }
        let healer = vosh_prompt::PromptConfig {
            draw: false,
            template: "%mana".into(),
            previous_templates: vec!["%move".into()],
            capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
                prompt: "<%h%m %vmv> ".into(),
                ..vosh_prompt::config::AabahranCapture::default()
            }),
            // Each profile keeps its own place, through its file.
            show: vosh_prompt::PromptShow::Pinned,
            mirror: false,
        };
        let mut file = ProfileConfig::default();
        file.set_prompt(healer.clone());
        file.save(&healer_file(dir.path())).unwrap();

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        {
            let p = state.profile.lock().await;
            let c = session.connection.lock();
            assert_eq!(*c.prompt.config(), healer);
            assert!(!p.ui.prompt_template_enabled);
            assert_eq!(p.ui.prompt_template, "%mana");
            assert!(c.prompt.forsaken(), "the capture reads Aabahran's codes");
            assert!(c.prompt.vars.gmcp().get("Char.Vitals").is_some());
            assert!(c.prompt.vars.prompt_vars().is_empty());
        }

        // On to a profile that never saved a file, a fresh one, which
        // follows the game and draws nothing until you turn drawing on.
        super::switch_live_profile(&state, &session, "Test-Prompt")
            .await
            .unwrap();
        let c = session.connection.lock();
        assert_eq!(*c.prompt.config(), vosh_prompt::PromptConfig::fresh());
        assert!(!c.prompt.draws());
        assert!(!c.prompt.forsaken());
        assert!(c.prompt.vars.gmcp().get("Char.Vitals").is_some());
    }

    #[tokio::test]
    async fn a_profile_you_create_follows_the_game_and_keeps_following_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        {
            let mut guard = state.profile_set.lock().await;
            let set = guard.as_mut().unwrap();
            set.create_from("Fresh", None, None).unwrap();
            // A profile whose file holds no design keeps none.
            set.create_from("Mortal", None, None).unwrap();
            let mut file = ProfileConfig::default();
            file.ui.tracked_affects = vec![affect("Haste")];
            file.save(&set.profile_path("Mortal")).unwrap();
        }

        super::switch_live_profile(&state, &session, "Fresh")
            .await
            .unwrap();
        {
            let p = state.profile.lock().await;
            let c = session.connection.lock();
            assert_eq!(*c.prompt.config(), vosh_prompt::PromptConfig::fresh());
            assert_eq!(p.ui.prompt_template, "");
        }
        // The first save keeps it following the game.
        persist(&state).await;
        let path = state
            .profile_set
            .lock()
            .await
            .as_ref()
            .unwrap()
            .profile_path("Fresh");
        assert_eq!(
            ProfileConfig::load(&path).unwrap().prompt_config(),
            vosh_prompt::PromptConfig::fresh()
        );

        super::switch_live_profile(&state, &session, "Mortal")
            .await
            .unwrap();
        let p = state.profile.lock().await;
        assert!(session.connection.lock().prompt.config().is_default());
        assert_eq!(live_names(&p.ui.tracked_affects), ["Haste"]);
    }

    fn live_names(list: &[crate::profile::ui::TrackedAffect]) -> Vec<&str> {
        list.iter().map(|t| t.name.as_str()).collect()
    }

    #[tokio::test]
    async fn a_switch_applies_the_latest_char_prompt_to_the_next_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        let game = "%n%P%C<%hhp %mm %vmv> ";
        {
            let mut c = session.connection.lock();
            c.prompt.connect(true);
            c.prompt.observe(
                "Char.Prompt",
                serde_json::json!({"enabled": true, "prompt": game, "fprompt": ""}),
                chrono::Local::now().fixed_offset(),
            );
            assert!(!c.prompt.take_seen()[0].applied, "default reads nothing");
        }
        let codes = |follow_game| vosh_prompt::PromptConfig {
            draw: true,
            template: "%hp".into(),
            capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
                prompt: "<%h%m %vmv> ".into(),
                follow_game,
                ..vosh_prompt::config::AabahranCapture::default()
            }),
            ..vosh_prompt::PromptConfig::default()
        };
        let mut file = ProfileConfig::default();
        file.set_prompt(codes(true));
        file.save(&healer_file(dir.path())).unwrap();

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        {
            let mut c = session.connection.lock();
            let vosh_prompt::CaptureConfig::Aabahran(taken) = &c.prompt.config().capture else {
                panic!("an aabahran capture");
            };
            assert_eq!(taken.prompt, game);
            assert_eq!(taken.source, Some(vosh_prompt::config::CaptureSource::Gmcp));
            let seen = c.prompt.take_seen();
            assert_eq!(seen.len(), 1);
            assert!(seen[0].applied, "the toast follows");
        }

        // A capture that does not follow the game keeps its codes.
        let mut file = ProfileConfig::default();
        file.set_prompt(codes(false));
        file.save(&healer_file(dir.path())).unwrap();
        super::switch_live_profile(&state, &session, "Test-Prompt")
            .await
            .unwrap();
        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        let mut c = session.connection.lock();
        assert_eq!(*c.prompt.config(), codes(false));
        let leftover = &c.prompt.take_seen();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[tokio::test]
    async fn a_profile_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        std::fs::write(healer_file(dir.path()), UNREADABLE).unwrap();

        let err = super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "Vosh could not open the Healer profile because it could not read the profile \
             file. You are still using the Default profile."
        );
        // The index still names the live profile, in memory and on
        // disk, so the next persist writes it to its own file.
        assert_eq!(active(&state).await, DEFAULT_PROFILE_NAME);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), DEFAULT_PROFILE_NAME);
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
        // The file that did not read stays as it was.
        assert_eq!(
            std::fs::read_to_string(healer_file(dir.path())).unwrap(),
            UNREADABLE
        );
    }

    #[tokio::test]
    async fn a_global_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        let mut config = ProfileConfig::default();
        config.ui.tracked_affects = vec![affect("Haste")];
        config.save(&healer_file(dir.path())).unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        std::fs::write(set.global_path(), UNREADABLE).unwrap();

        let err = super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "Vosh could not open the Healer profile because it could not read global.toml, \
             which holds your shared settings. You are still using the Default profile."
        );
        assert_eq!(active(&state).await, DEFAULT_PROFILE_NAME);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), DEFAULT_PROFILE_NAME);
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
    }

    #[tokio::test]
    async fn a_login_switch_to_a_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        std::fs::write(healer_file(dir.path()), UNREADABLE).unwrap();
        *session.current_connection.lock().unwrap() =
            Some(("play.theforsakenlands.com".into(), 1848));

        // Corvanne logging in picks Healer, whose file does not read.
        let target = super::auto_switch_target(&state, &session, "Corvanne").await;
        assert_eq!(target.as_deref(), Some("Healer"));
        let err = super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap_err();
        assert_eq!(
            super::auto_switch_failed_line(&err),
            "\r\n\x1b[33mVosh could not open the Healer profile because it could not read \
             the profile file. You are still using the Default profile.\x1b[0m\r\n"
        );
        assert_eq!(active(&state).await, DEFAULT_PROFILE_NAME);
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
        // Ilsabet belongs to the live profile, so nothing switches.
        assert_eq!(
            super::auto_switch_target(&state, &session, "Ilsabet").await,
            None
        );
    }

    #[tokio::test]
    async fn a_switch_that_reads_its_files_lets_the_saves_resume() {
        let dir = tempfile::tempdir().unwrap();
        let set = james_like_set(dir.path());
        std::fs::write(set.active_path(), UNREADABLE).unwrap();
        let mut healer = ProfileConfig::default();
        healer.ui.tracked_affects = vec![affect("Haste")];
        healer.save(&set.profile_path("Healer")).unwrap();
        let state = launch_state(dir.path()).await;
        let session = state.selected_session();

        // A switch saves the profile you leave first, and that save
        // leaves the file that did not read alone.
        persist(&state).await;
        assert_eq!(read(&set.active_path()), UNREADABLE);
        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        assert_eq!(live_affects(&state).await, ["Haste"]);
        state.profile.lock().await.ui.tracked_affects = vec![affect("Fly")];
        persist(&state).await;

        let saved = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
        assert_eq!(saved.ui.tracked_affects[0].name, "Fly");
        // The file that did not read was never written.
        assert_eq!(read(&set.profile_path(DEFAULT_PROFILE_NAME)), UNREADABLE);
    }
}
