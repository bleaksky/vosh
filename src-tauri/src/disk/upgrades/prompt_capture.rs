//! The one-time move of prompt capture triggers into the profiles.
//!
//! Older builds read your prompt with a trigger that hides the prompt
//! line and hands its groups to `mud.set_prompt_var`, the kind `#prompt
//! {regex}` wrote. In loadout mode that trigger sits in the shared
//! catalog, so it hid the prompt in every profile, even one that draws
//! nothing in its place. Launch moves each such trigger into the
//! `[prompt.capture]` of the profiles that draw, turns it off and keeps
//! it. Nothing is deleted.
//!
//! - A trigger counts by what it does, whatever its name: a Script action
//!   that calls `mud.set_prompt_var`. It moves when its actions are Gag
//!   then Script, or Script alone, and its script only hands groups of its
//!   one pattern to `mud.set_prompt_var` (see
//!   [`vosh_prompt::capture::from_trigger`]). One that does more stays as
//!   it was, and the launch notice names it when it hides the prompt.
//! - Enabled triggers move. A `prompt-capture` that was off before the
//!   first move stays off, since you turned it off yourself.
//! - Loadout mode. A catalog trigger goes into every profile that draws a
//!   design, or when none does, into every profile, so a Vitals pane the
//!   capture feeds keeps working. The catalog trigger is turned off.
//! - A trigger in a profile file, in either mode, goes into that profile
//!   and is turned off there.
//! - A profile that already has a capture keeps it. `prompt-prefix`, which
//!   only hides a line, is left alone.
//! - Nothing moves while a profile file does not read, since Vosh cannot
//!   tell whether it draws. The move waits for the next launch.
//!
//! The move is recorded in `profiles.toml` as `prompt-capture-to-profile`
//! and runs once. Launch runs it before any profile loads, under the
//! persist lock, and skips it while the shared catalog wizard waits for a
//! relaunch.
//!
//! An older build reads the switch and the design from `[ui]` and drops
//! `[prompt]` from each profile file it saves, the active one at every
//! quit, while `profiles.toml` keeps the record. So the move runs again
//! whenever a profile file that Vosh wrote a `[prompt]` into, which its
//! `.before-prompt-editor` copy shows, has none. Then the `prompt-capture`
//! that move turned off goes back into each such profile it fits.

use std::path::{Path, PathBuf};

use vosh_automation::trigger::{Trigger, TriggerAction};
use vosh_prompt::capture::{self, NotACapture};
use vosh_prompt::card::sentences::and_list;
use vosh_prompt::config::RegexCapture;
use vosh_prompt::CaptureConfig;

use crate::loadouts::catalog::{load_global_catalog, loadout_mode_on, save_global_catalog};
use crate::profile::file::{before_prompt_editor_path, ProfileConfig};
use crate::profile::set::{display_name, ProfileSet};

/// The id the move is recorded under in `profiles.toml`.
pub(crate) const MIGRATION: &str = "prompt-capture-to-profile";

/// The name `#prompt {regex}` gave its trigger in older builds.
const CAPTURE_TRIGGER: &str = "prompt-capture";

/// Run the move over `set`, the profile set launch read from the app
/// data folder `app_data`, unless it already ran, and return the launch
/// notices it leaves. A catalog that does not read or save, or an index
/// that does not save, leaves the move unrecorded, so it runs again at
/// the next launch.
pub(crate) fn run(set: &mut ProfileSet, app_data: &Path) -> Vec<String> {
    let returned = set
        .list()
        .iter()
        .any(|entry| lost_prompt(&set.profile_path(&entry.name)));
    if set.migrated(MIGRATION) && !returned {
        return Vec::new();
    }
    let loadout = loadout_mode_on(app_data);
    let moved = match migrate(set, app_data, loadout) {
        Ok(moved) => moved,
        Err(e) => {
            tracing::error!(error = %e, "prompt capture move stopped, it runs again at the next launch");
            return Vec::new();
        }
    };
    if let Err(e) = set.record_migration(MIGRATION) {
        tracing::error!(error = %e, "prompt capture move: could not record it in profiles.toml");
    }
    tracing::info!(
        profiles = ?moved.into,
        left = ?moved.left,
        "moved prompt capture triggers into the profiles",
    );
    moved.notices()
}

/// What the move did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Moved {
    /// The profiles that took a capture, in index order.
    pub(crate) into: Vec<String>,
    /// Triggers that still hide the prompt and stayed as they were, with
    /// why, each named once.
    pub(crate) left: Vec<(String, NotACapture)>,
}

impl Moved {
    /// The launch notices, as sentences. None when nothing moved and
    /// nothing was left.
    pub(crate) fn notices(&self) -> Vec<String> {
        let mut notices = Vec::new();
        if !self.into.is_empty() {
            let names: Vec<String> = self.into.iter().map(|n| display_name(n)).collect();
            let noun = if names.len() == 1 {
                "profile"
            } else {
                "profiles"
            };
            notices.push(format!(
                "Vosh moved your prompt capture into the {} {noun}. Profiles that do not draw a \
                 prompt now show the game's prompt.",
                and_list(&names)
            ));
        }
        for (name, why) in &self.left {
            let reason = match why {
                NotACapture::Patterns => "it reads your prompt with more than one pattern",
                NotACapture::ScriptDoesMore | NotACapture::BadPattern => {
                    "its script does more than read your prompt"
                }
            };
            notices.push(format!(
                "Vosh left the trigger {name} as it was, since {reason}. It still hides the \
                 prompt in profiles that do not draw one."
            ));
        }
        notices
    }

    fn took(&mut self, profile: &str) {
        if !self.into.iter().any(|n| n == profile) {
            self.into.push(profile.to_string());
        }
    }

    fn leave(&mut self, trigger: &Trigger, why: NotACapture) {
        if !self.left.iter().any(|(n, _)| *n == trigger.name) {
            self.left.push((trigger.name.clone(), why));
        }
    }
}

/// One profile's file as the move reads and writes it.
struct ProfileFile {
    name: String,
    path: PathBuf,
    config: ProfileConfig,
    /// An older build saved it without the `[prompt]` Vosh wrote into it.
    lost_prompt: bool,
    changed: bool,
}

/// Whether the profile file at `path` lost the `[prompt]` table Vosh
/// wrote into it, as an older build that saves it leaves it. Vosh keeps
/// the file as it was before its first `[prompt]`, and a `[prompt]` that
/// says nothing a default one does not stays out of the file, so a file
/// with that copy and no table either went back to a default prompt in
/// this build or lost its table to an older one. A file that does not
/// read says nothing.
fn lost_prompt(path: &Path) -> bool {
    before_prompt_editor_path(path).exists()
        && std::fs::read_to_string(path)
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
            .is_some_and(|table| !table.contains_key("prompt"))
}

impl ProfileFile {
    fn has_capture(&self) -> bool {
        !self.config.prompt_config().capture.is_none()
    }

    fn draws(&self) -> bool {
        let prompt = self.config.prompt_config();
        prompt.draw && !prompt.template.is_empty()
    }

    /// Give the profile `capture` unless it has one. Returns whether it
    /// took it.
    fn take(&mut self, capture: &RegexCapture) -> bool {
        if self.has_capture() {
            return false;
        }
        let mut prompt = self.config.prompt_config();
        prompt.capture = CaptureConfig::Regex(capture.clone());
        self.config.set_prompt(prompt);
        self.changed = true;
        true
    }
}

/// How a trigger that calls `mud.set_prompt_var` reads as a capture, or
/// None for a trigger that does not call it.
fn candidate(trigger: &Trigger) -> Option<Result<RegexCapture, NotACapture>> {
    let body = trigger.actions.iter().find_map(|a| match a {
        TriggerAction::Script { body } if body.contains("set_prompt_var") => Some(body),
        _ => None,
    })?;
    let fits_actions = matches!(
        trigger.actions.as_slice(),
        [TriggerAction::Gag, TriggerAction::Script { .. }] | [TriggerAction::Script { .. }]
    );
    if !fits_actions {
        return Some(Err(NotACapture::ScriptDoesMore));
    }
    // The regex each row compiles to, so a Text or Starts with row reads
    // as the trigger store reads it.
    let sources: Vec<_> = trigger
        .patterns
        .iter()
        .filter(|p| p.enabled)
        .map(vosh_automation::trigger::TriggerPattern::regex_source)
        .collect();
    let patterns: Vec<&str> = sources.iter().map(AsRef::as_ref).collect();
    Some(capture::from_trigger(&patterns, body))
}

/// Whether a trigger that reads your prompt moves into `file`: it is on,
/// or it is the `prompt-capture` an earlier move turned off and `file`
/// lost the capture that move gave it to an older build.
fn moves_into(trigger: &Trigger, file: &ProfileFile) -> bool {
    trigger.enabled || (trigger.name == CAPTURE_TRIGGER && file.lost_prompt)
}

/// A trigger that stays but still hides the prompt, which the notice
/// names. One that never matched or hides nothing is left out.
fn still_hides(trigger: &Trigger, why: NotACapture) -> bool {
    trigger.enabled
        && why != NotACapture::BadPattern
        && trigger
            .actions
            .iter()
            .any(|a| matches!(a, TriggerAction::Gag))
}

/// The move itself over the profiles `set` lists. `loadout` is whether
/// catalog.toml holds the shared triggers. Writes each profile file it
/// changed, then the catalog. The caller records the move.
pub(crate) fn migrate(set: &ProfileSet, app_data: &Path, loadout: bool) -> Result<Moved, String> {
    let mut files: Vec<ProfileFile> = Vec::new();
    for stored in set.read_all() {
        let config = match stored.file {
            Some(Ok(file)) => file.config,
            // A file that does not read may be the one that draws, so the
            // whole move waits. Launch holds the file, and the move never
            // writes over it.
            Some(Err(e)) => return Err(format!("{}: {e}", stored.path.display())),
            None => ProfileConfig::fresh(),
        };
        files.push(ProfileFile {
            name: stored.name.to_string(),
            lost_prompt: lost_prompt(&stored.path),
            path: stored.path,
            config,
            changed: false,
        });
    }
    let mut moved = Moved::default();

    // The shared catalog, in loadout mode.
    let mut catalog = if loadout {
        Some(load_global_catalog(app_data).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let mut catalog_changed = false;
    if let Some(catalog) = catalog.as_mut() {
        let drawing: Vec<usize> = (0..files.len()).filter(|&i| files[i].draws()).collect();
        let targets: Vec<usize> = if drawing.is_empty() {
            (0..files.len())
                .filter(|&i| !files[i].config.prompt_config().draw)
                .collect()
        } else {
            drawing
        };
        for trigger in &mut catalog.triggers {
            let Some(read) = candidate(trigger) else {
                continue;
            };
            match read {
                Ok(capture) => {
                    for &i in &targets {
                        if moves_into(trigger, &files[i]) && files[i].take(&capture) {
                            moved.took(&files[i].name);
                        }
                    }
                    if trigger.enabled {
                        trigger.enabled = false;
                        catalog_changed = true;
                    }
                }
                Err(why) => {
                    if still_hides(trigger, why) {
                        moved.leave(trigger, why);
                    }
                }
            }
        }
    }

    // Each profile file's own triggers, in either mode.
    for file in &mut files {
        let mut triggers = std::mem::take(&mut file.config.triggers);
        for trigger in &mut triggers {
            let Some(read) = candidate(trigger) else {
                continue;
            };
            match read {
                Ok(capture) => {
                    if moves_into(trigger, file) && file.take(&capture) {
                        moved.took(&file.name);
                    }
                    if trigger.enabled {
                        trigger.enabled = false;
                        file.changed = true;
                    }
                }
                Err(why) => {
                    if still_hides(trigger, why) {
                        moved.leave(trigger, why);
                    }
                }
            }
        }
        file.config.triggers = triggers;
    }

    // The profiles first, so a catalog that fails to save leaves its
    // trigger on and the move runs again, where each profile keeps the
    // capture it took.
    for file in files.iter().filter(|f| f.changed) {
        file.config
            .save(&file.path)
            .map_err(|e| format!("{}: {e}", file.path.display()))?;
    }
    if let (Some(catalog), true) = (catalog.as_ref(), catalog_changed) {
        save_global_catalog(app_data, catalog).map_err(|e| e.to_string())?;
    }
    // Profiles listed in index order.
    moved.into.sort_by_key(|name| {
        set.list()
            .iter()
            .position(|e| e.name == *name)
            .unwrap_or(usize::MAX)
    });
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use vosh_prompt::config::{AabahranCapture, CaptureSource};
    use vosh_prompt::{CaptureConfig, PromptConfig};

    use super::{run, MIGRATION};
    use crate::loadouts::catalog::load_global_catalog;
    use crate::profile::file::{before_prompt_editor_path, ProfileConfig};
    use crate::profile::set::ProfileSet;

    /// The design the default profile draws, as its file keeps it in
    /// `[ui]`.
    const TEMPLATE: &str = vosh_prompt::testkit::designs::JAMES;

    /// The pattern of the capture `#prompt` wrote into the catalog.
    const PATTERN: &str = r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";

    const MOVED_INTO_DEFAULT: &str = "Vosh moved your prompt capture into the Default profile. \
         Profiles that do not draw a prompt now show the game's prompt.";

    const INDEX: &str = r#"active = "default"

[[profile]]
name = "default"
description = "Immortal"

[profile.auto_match]
host = "play.theforsakenlands.com"
port = 1848
characters = ["Tester"]

[[profile]]
name = "Healer"

[profile.auto_match]
host = "play.theforsakenlands.com"
port = 1848
characters = ["Testhealer"]

[[profile]]
name = "Test-Prompt"
description = "Copy of default for testing"

[profile.auto_match]
host = "play.theforsakenlands.com"
port = 1848
characters = ["Tester"]

[scope]
theme = "profile"
font = "global"
dock_layout = "profile"
keep_last_command = "global"
auto_update = "global"
"#;

    /// The capture trigger as the catalog holds it, pattern twice and
    /// all.
    const CAPTURE_TRIGGER: &str = r#"[[triggers]]
name = "prompt-capture"
pattern = '\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]'
priority = 100
enabled = true

[[triggers.patterns]]
pattern = '\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]'
enabled = true

[[triggers.actions]]
kind = "gag"

[[triggers.actions]]
kind = "script"
body = """
mud.set_prompt_var(\"hp\", captures[2])
mud.set_prompt_var(\"maxhp\", captures[3])
mud.set_prompt_var(\"mana\", captures[4])
mud.set_prompt_var(\"maxmana\", captures[5])
mud.set_prompt_var(\"move\", captures[6])
mud.set_prompt_var(\"maxmove\", captures[7])"""
"#;

    /// The rest of the catalog: a highlight preset, the prefix gag that
    /// never matches, and an alias.
    const CATALOG_REST: &str = r#"
[[triggers]]
name = "combat.outgoing_miss"
pattern = "^(You(?:r .+?)? )(misses|miss)( .+[!.])$"
priority = 7
enabled = true
preset = "combat_outgoing"

[[triggers.patterns]]
pattern = "^(You(?:r .+?)? )(misses|miss)( .+[!.])$"
enabled = true

[[triggers.actions]]
kind = "replace"
template = "\u001B[38;5;253m$1\u001B[0m\u001B[38;5;152m$2\u001B[0m\u001B[38;5;253m$3\u001B[0m"

[[triggers]]
name = "prompt-prefix"
pattern = '^\(Wizi \d+\) \(Incog \d+\) *$'
priority = 0
enabled = true

[[triggers.patterns]]
pattern = '^\(Wizi \d+\) \(Incog \d+\) *$'
enabled = true

[[triggers.actions]]
kind = "gag"

[[aliases]]
name = "another"
expansion = "say Another"
enabled = true
"#;

    const LOADOUTS: &str = r#"active = ["default", "Healer"]
dormant = false

[[loadouts]]
name = "default"
enabled_groups = []

[[loadouts]]
name = "Healer"
enabled_groups = []
"#;

    /// The default profile's file: it draws the design, and holds no
    /// triggers of its own.
    fn default_file() -> String {
        format!(
            r#"aliases = []
triggers = []
macros = []

[connection]
host = "play.theforsakenlands.com"
port = 1848
tls = false

[profile_vars]

[tick]
enabled = true
interval_secs = 30
sound = true

[ui]
theme = "vellum"
theme_terminal_colors = true
prompt_template_enabled = true
prompt_template = {}
tick_count = "up"

[[ui.tracked_affects]]
name = "mounted"

[plugins]
enabled = []
"#,
            toml::Value::String(TEMPLATE.to_string())
        )
    }

    /// A profile that draws nothing, with no prompt keys at all.
    const HEALER_FILE: &str = r#"aliases = []
triggers = []
autoload_scripts = []
macros = []

[connection]
host = "play.theforsakenlands.com"
port = 1848
tls = false

[ui]
theme = "kanso-zen"
theme_terminal_colors = true

[[ui.tracked_affects]]
name = "sanctuary"
label = "sanc"

[plugins]
enabled = []
"#;

    /// An older copy of default with legacy triggers of its own, none of
    /// them a capture.
    const TEST_PROMPT_FILE: &str = r#"aliases = []
macros = []

[[triggers]]
name = "combat.outgoing_miss"
pattern = "^(You(?:r .+?)? )(misses|miss)( .+[!.])$"
priority = 7
enabled = true

[[triggers.actions]]
kind = "replace"
template = "$1$2$3"

[[triggers]]
name = "washdemo"
pattern = "You say"
priority = 0
enabled = true

[[triggers.actions]]
kind = "highlight"

[triggers.actions.style]
fg = "cyan"

[ui]
theme = "vellum"
"#;

    /// A folder shaped like James's: loadout mode, the capture in the
    /// catalog, Default drawing, Healer and Test-Prompt drawing nothing.
    fn james_like(catalog_triggers: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("profiles")).unwrap();
        std::fs::write(root.join("profiles.toml"), INDEX).unwrap();
        std::fs::write(
            root.join("catalog.toml"),
            format!("macros = []\nenabled_presets = []\n\n{catalog_triggers}{CATALOG_REST}"),
        )
        .unwrap();
        std::fs::write(root.join("loadouts.toml"), LOADOUTS).unwrap();
        std::fs::write(root.join("profiles/default.toml"), default_file()).unwrap();
        std::fs::write(root.join("profiles/Healer.toml"), HEALER_FILE).unwrap();
        std::fs::write(root.join("profiles/Test-Prompt.toml"), TEST_PROMPT_FILE).unwrap();
        dir
    }

    /// Every file under `root`, by path, with its bytes.
    fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.insert(path.clone(), std::fs::read(&path).unwrap());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, &mut out);
        out
    }

    /// Read the profile set in `root` and run the move over it, as
    /// launch does.
    fn read_and_run(root: &Path) -> Vec<String> {
        let mut set = ProfileSet::load_or_migrate(root.to_path_buf()).unwrap();
        run(&mut set, root)
    }

    fn profile(root: &Path, name: &str) -> ProfileConfig {
        ProfileConfig::load(&root.join("profiles").join(format!("{name}.toml"))).unwrap()
    }

    fn text(root: &Path, file: &str) -> String {
        std::fs::read_to_string(root.join(file)).unwrap()
    }

    fn catalog_trigger(root: &Path, name: &str) -> vosh_automation::trigger::Trigger {
        load_global_catalog(root)
            .unwrap()
            .triggers
            .into_iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("the catalog holds {name}"))
    }

    /// The capture the old trigger becomes.
    fn moved_capture(prompt: &PromptConfig) -> &vosh_prompt::config::RegexCapture {
        match &prompt.capture {
            CaptureConfig::Regex(capture) => capture,
            other => panic!("a regex capture, got {other:?}"),
        }
    }

    fn backups(root: &Path, file: &str) -> usize {
        std::fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{file}.bak."))
            })
            .count()
    }

    #[test]
    fn the_catalog_capture_moves_into_the_profile_that_draws() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();

        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);

        // Default draws the same design, now read through its own capture.
        let default = profile(root, "default").prompt_config();
        assert!(default.draw);
        assert_eq!(default.template, TEMPLATE);
        let capture = moved_capture(&default);
        assert_eq!(capture.lines, [PATTERN]);
        assert!(!capture.settle, "the old pattern waits for a line end");
        assert!(capture.names.is_empty());
        assert_eq!(capture.source, Some(CaptureSource::Migrated));
        // The [ui] copy stays for an older build.
        let file: toml::Table = text(root, "profiles/default.toml").parse().unwrap();
        assert_eq!(file["ui"]["prompt_template_enabled"].as_bool(), Some(true));
        assert_eq!(file["ui"]["prompt_template"].as_str(), Some(TEMPLATE));
        assert_eq!(file["prompt"]["capture"]["kind"].as_str(), Some("regex"));
        // The file as it was, kept beside it once.
        let before = before_prompt_editor_path(&root.join("profiles/default.toml"));
        assert_eq!(std::fs::read_to_string(before).unwrap(), default_file());
        assert_eq!(backups(&root.join("profiles"), "default.toml"), 1);

        // Healer and Test-Prompt are untouched. They show the game's
        // prompt from here on.
        assert_eq!(text(root, "profiles/Healer.toml"), HEALER_FILE);
        assert_eq!(text(root, "profiles/Test-Prompt.toml"), TEST_PROMPT_FILE);
        for name in ["Healer", "Test-Prompt"] {
            let path = root.join("profiles").join(format!("{name}.toml"));
            assert!(!before_prompt_editor_path(&path).exists(), "{name}");
        }

        // The catalog keeps the trigger, off, and leaves the rest alone.
        let capture_trigger = catalog_trigger(root, "prompt-capture");
        assert!(!capture_trigger.enabled);
        assert_eq!(capture_trigger.patterns[0].pattern, PATTERN);
        assert_eq!(capture_trigger.actions.len(), 2);
        assert!(catalog_trigger(root, "prompt-prefix").enabled);
        assert!(catalog_trigger(root, "combat.outgoing_miss").enabled);
        assert_eq!(load_global_catalog(root).unwrap().aliases.len(), 1);
        assert_eq!(backups(root, "catalog.toml"), 1);

        // Recorded, so it never runs again.
        let set = ProfileSet::load_or_migrate(root.to_path_buf()).unwrap();
        assert!(set.migrated(MIGRATION));
        assert!(
            text(root, "profiles.toml").contains("migrations = [\"prompt-capture-to-profile\"]")
        );
    }

    #[test]
    fn a_second_run_changes_nothing() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        let after_first = snapshot(root);
        let leftover = &read_and_run(root);
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(snapshot(root), after_first);
    }

    #[test]
    fn a_trigger_whose_script_does_more_stays_and_is_named() {
        let alarm = r#"[[triggers]]
name = "hp-alarm"
pattern = '<(?<hp>\d+)hp>'
priority = 50
enabled = true

[[triggers.actions]]
kind = "gag"

[[triggers.actions]]
kind = "script"
body = """
mud.set_prompt_var(\"hp\", captures[2])
if tonumber(captures[2]) < 100 then mud.send(\"flee\") end"""

"#;
        let dir = james_like(&format!("{alarm}{CAPTURE_TRIGGER}"));
        let root = dir.path();
        assert_eq!(
            read_and_run(root),
            [
                MOVED_INTO_DEFAULT.to_string(),
                "Vosh left the trigger hp-alarm as it was, since its script does more than \
                 read your prompt. It still hides the prompt in profiles that do not draw one."
                    .to_string(),
            ]
        );
        assert!(catalog_trigger(root, "hp-alarm").enabled);
        assert!(!catalog_trigger(root, "prompt-capture").enabled);
    }

    #[test]
    fn a_capture_trigger_under_another_name_moves() {
        let reader = r#"[[triggers]]
name = "vitals-reader"
pattern = '<(?<h>\d+)hp (?<m>\d+)m (?<v>\d+)mv>'
priority = 10
enabled = true

[[triggers.actions]]
kind = "script"
body = """
mud.set_prompt_var('hp', captures[2])
mud.set_prompt_var('mana', captures[3])
mud.set_prompt_var('move', captures[4])"""

"#;
        let dir = james_like(reader);
        let root = dir.path();
        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        let default = profile(root, "default").prompt_config();
        let capture = moved_capture(&default);
        assert_eq!(capture.lines, [r"<(?<h>\d+)hp (?<m>\d+)m (?<v>\d+)mv>"]);
        assert_eq!(
            capture.names,
            BTreeMap::from([
                ("h".to_string(), "hp".to_string()),
                ("m".to_string(), "mana".to_string()),
                ("v".to_string(), "move".to_string()),
            ])
        );
        assert!(!catalog_trigger(root, "vitals-reader").enabled);
    }

    /// Put the folder back the way an older build leaves it after the
    /// move: it reads [ui] alone and drops [prompt] and the migration
    /// list on its next saves.
    fn older_build_saves(root: &Path) {
        let path = root.join("profiles/default.toml");
        let mut file: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        file.remove("prompt");
        std::fs::write(&path, toml::to_string_pretty(&file).unwrap()).unwrap();
        let path = root.join("profiles.toml");
        let mut index: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        index.remove("migrations");
        std::fs::write(&path, toml::to_string_pretty(&index).unwrap()).unwrap();
    }

    /// What an older build does on quit: it saves the active profile
    /// without [prompt], and leaves profiles.toml as it is, since it
    /// writes the index only on a switch, a claim or an edit to the list.
    fn older_build_quits(root: &Path) {
        let path = root.join("profiles/default.toml");
        let mut file: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        file.remove("prompt");
        std::fs::write(&path, toml::to_string_pretty(&file).unwrap()).unwrap();
    }

    #[test]
    fn a_return_from_an_older_build_moves_the_capture_again() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        older_build_quits(root);
        assert!(ProfileSet::load_or_migrate(root.to_path_buf())
            .unwrap()
            .migrated(MIGRATION));
        assert!(profile(root, "default").prompt_config().capture.is_none());

        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        let default = profile(root, "default").prompt_config();
        assert_eq!(moved_capture(&default).lines, [PATTERN]);
        assert!(default.draw);
        assert_eq!(default.template, TEMPLATE);
        assert!(!catalog_trigger(root, "prompt-capture").enabled);
        // Healer and Test-Prompt stay untouched.
        assert_eq!(text(root, "profiles/Healer.toml"), HEALER_FILE);
        assert_eq!(text(root, "profiles/Test-Prompt.toml"), TEST_PROMPT_FILE);
        // The copy from the first run stays as it was.
        let before = before_prompt_editor_path(&root.join("profiles/default.toml"));
        assert_eq!(std::fs::read_to_string(before).unwrap(), default_file());
        // Back in step, so the next launch changes nothing.
        let after = snapshot(root);
        let leftover = &read_and_run(root);
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(snapshot(root), after);
    }

    #[test]
    fn a_capture_turned_on_again_in_an_older_build_moves_again() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        older_build_quits(root);
        // You turned it back on so the older build drew your prompt.
        let path = root.join("catalog.toml");
        let catalog = text(root, "catalog.toml").replacen("enabled = false", "enabled = true", 1);
        std::fs::write(&path, catalog).unwrap();
        assert!(catalog_trigger(root, "prompt-capture").enabled);

        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        let default = profile(root, "default").prompt_config();
        assert_eq!(moved_capture(&default).lines, [PATTERN]);
        assert!(!catalog_trigger(root, "prompt-capture").enabled);
        assert_eq!(text(root, "profiles/Healer.toml"), HEALER_FILE);
    }

    #[test]
    fn a_capture_you_turned_off_before_the_move_stays_off() {
        // The trigger off, its pattern on.
        let off = CAPTURE_TRIGGER.replacen("enabled = true", "enabled = false", 1);
        let dir = james_like(&off);
        let root = dir.path();
        assert!(!catalog_trigger(root, "prompt-capture").enabled);
        let default_before = text(root, "profiles/default.toml");

        let leftover = &read_and_run(root);
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(profile(root, "default").prompt_config().capture.is_none());
        assert_eq!(text(root, "profiles/default.toml"), default_before);
        assert!(!catalog_trigger(root, "prompt-capture").enabled);
        assert!(ProfileSet::load_or_migrate(root.to_path_buf())
            .unwrap()
            .migrated(MIGRATION));
    }

    #[test]
    fn a_rerun_after_an_older_build_takes_the_turned_off_capture_again() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        older_build_saves(root);
        assert!(profile(root, "default").prompt_config().capture.is_none());
        assert!(!catalog_trigger(root, "prompt-capture").enabled);

        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        let default = profile(root, "default").prompt_config();
        assert_eq!(moved_capture(&default).lines, [PATTERN]);
        assert!(default.draw);
        assert_eq!(default.template, TEMPLATE);
        assert!(!catalog_trigger(root, "prompt-capture").enabled);
        // The copy from the first run stays as it was.
        let before = before_prompt_editor_path(&root.join("profiles/default.toml"));
        assert_eq!(std::fs::read_to_string(before).unwrap(), default_file());
        assert!(ProfileSet::load_or_migrate(root.to_path_buf())
            .unwrap()
            .migrated(MIGRATION));
    }

    #[test]
    fn a_rolled_back_profile_takes_its_capture_again_while_another_keeps_its_own() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        // Healer read its prompt from the game's codes meanwhile.
        let healer_path = root.join("profiles/Healer.toml");
        let mut healer = ProfileConfig::load(&healer_path).unwrap();
        let codes = PromptConfig {
            capture: CaptureConfig::Aabahran(AabahranCapture::default()),
            ..PromptConfig::default()
        };
        healer.set_prompt(codes.clone());
        healer.save(&healer_path).unwrap();
        older_build_saves(root);
        let healer_before = text(root, "profiles/Healer.toml");

        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        let default = profile(root, "default").prompt_config();
        assert_eq!(moved_capture(&default).lines, [PATTERN]);
        assert_eq!(text(root, "profiles/Healer.toml"), healer_before);
        assert_eq!(profile(root, "Healer").prompt_config(), codes);
        assert!(ProfileSet::load_or_migrate(root.to_path_buf())
            .unwrap()
            .migrated(MIGRATION));
    }

    #[test]
    fn a_turned_off_capture_stays_off_for_a_profile_an_older_build_never_saved() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT]);
        // You took the capture out of Default in this build, and nothing
        // else draws or reads your prompt.
        let path = root.join("profiles/default.toml");
        let mut default = ProfileConfig::load(&path).unwrap();
        let mut prompt = default.prompt_config();
        prompt.capture = CaptureConfig::None;
        default.set_prompt(prompt);
        default.save(&path).unwrap();
        let before = snapshot(root);

        let leftover = &read_and_run(root);
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(snapshot(root), before);
        assert!(profile(root, "default").prompt_config().capture.is_none());
    }

    #[test]
    fn with_no_profile_drawing_every_profile_takes_the_capture() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        let off = default_file().replace(
            "prompt_template_enabled = true",
            "prompt_template_enabled = false",
        );
        std::fs::write(root.join("profiles/default.toml"), off).unwrap();

        assert_eq!(
            read_and_run(root),
            [
                "Vosh moved your prompt capture into the Default, Healer, and Test-Prompt \
              profiles. Profiles that do not draw a prompt now show the game's prompt."
            ]
        );
        for name in ["default", "Healer", "Test-Prompt"] {
            let prompt = profile(root, name).prompt_config();
            assert_eq!(moved_capture(&prompt).lines, [PATTERN], "{name}");
            assert!(!prompt.draw, "{name}");
        }
        // Default keeps its design for when you turn drawing on.
        assert_eq!(profile(root, "default").prompt_config().template, TEMPLATE);
        // Test-Prompt keeps its own triggers as they were.
        let test_prompt = profile(root, "Test-Prompt");
        assert_eq!(test_prompt.triggers.len(), 2);
        assert!(test_prompt.triggers.iter().all(|t| t.enabled));
    }

    #[test]
    fn per_profile_mode_moves_each_file_trigger_into_its_own_profile() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        std::fs::remove_file(root.join("catalog.toml")).unwrap();
        std::fs::remove_file(root.join("loadouts.toml")).unwrap();
        // Healer read its prompt with #prompt, which wrote the capture
        // into its own file.
        let healer = HEALER_FILE.replace("triggers = []\n", "") + "\n" + CAPTURE_TRIGGER;
        std::fs::write(root.join("profiles/Healer.toml"), healer).unwrap();
        let default_before = text(root, "profiles/default.toml");

        assert_eq!(
            read_and_run(root),
            [
                "Vosh moved your prompt capture into the Healer profile. Profiles that do not \
              draw a prompt now show the game's prompt."
            ]
        );
        let healer = profile(root, "Healer");
        assert_eq!(moved_capture(&healer.prompt_config()).lines, [PATTERN]);
        assert!(!healer.prompt_config().draw);
        let trigger = healer
            .triggers
            .iter()
            .find(|t| t.name == "prompt-capture")
            .expect("the trigger stays in the file");
        assert!(!trigger.enabled);
        assert_eq!(text(root, "profiles/default.toml"), default_before);
        assert!(!root.join("catalog.toml").exists());
    }

    #[test]
    fn a_profile_file_that_does_not_read_leaves_the_move_for_the_next_launch() {
        for name in ["default", "Test-Prompt"] {
            let dir = james_like(CAPTURE_TRIGGER);
            let root = dir.path();
            let path = root.join("profiles").join(format!("{name}.toml"));
            let good = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, format!("{good}\nnot [ toml")).unwrap();
            let before = snapshot(root);
            // Vosh cannot tell whether the file draws, so nothing moves
            // and nothing is written.
            assert!(read_and_run(root).is_empty(), "{name}");
            assert_eq!(snapshot(root), before, "{name}");
            assert!(catalog_trigger(root, "prompt-capture").enabled, "{name}");

            // Fixed, the next launch moves the capture where it belongs.
            std::fs::write(&path, good).unwrap();
            assert_eq!(read_and_run(root), [MOVED_INTO_DEFAULT], "{name}");
            assert_eq!(
                moved_capture(&profile(root, "default").prompt_config()).lines,
                [PATTERN]
            );
            assert_eq!(text(root, "profiles/Healer.toml"), HEALER_FILE);
            assert_eq!(text(root, "profiles/Test-Prompt.toml"), TEST_PROMPT_FILE);
        }
    }

    #[test]
    fn a_catalog_that_does_not_read_leaves_the_move_for_the_next_launch() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        std::fs::write(root.join("catalog.toml"), "not [ toml").unwrap();
        let before = snapshot(root);
        let leftover = &read_and_run(root);
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(snapshot(root), before);
    }

    #[tokio::test]
    async fn launch_moves_the_capture_before_the_profile_loads() {
        let dir = james_like(CAPTURE_TRIGGER);
        let root = dir.path();
        let state: crate::app::state::SharedState =
            std::sync::Arc::new(crate::app::state::AppState::default());
        crate::app::launch::load(&state, root).await;
        assert!(state
            .loadout_mode
            .load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(state.take_launch_messages(), [MOVED_INTO_DEFAULT]);

        let p = state.selected_profile().await;
        let prompt = &p.prompt;
        assert!(prompt.draw);
        assert_eq!(prompt.template, TEMPLATE);
        assert_eq!(moved_capture(prompt).lines, [PATTERN]);
        let capture = p
            .triggers
            .get("prompt-capture")
            .expect("the catalog trigger");
        assert!(!capture.enabled, "the catalog trigger hides nothing now");
        assert!(p.triggers.get("prompt-prefix").is_some_and(|t| t.enabled));
    }
}
