//! The profile in memory. It holds what its file saves, plus what runs
//! with that profile and no file saves: the revision counters that move
//! when a group turns on or off, and its name. Each session keeps its
//! variables, its Lua engine, the aliases its plugins make and its macro
//! recorder on its [`Connection`](crate::session::connection::Connection).

use std::collections::{BTreeMap, BTreeSet};

use vosh_automation::alert::AlertParts;
use vosh_automation::alias::AliasStore;
use vosh_automation::trigger::TriggerStore;
use vosh_automation::vars::VariableStore;

use crate::loadouts::preset_edits::PresetEdits;
use crate::profile::file::{GroupFolders, OnSwitch, PluginsPersist};
use crate::profile::ui::UiConfig;
use crate::tick::TickSettings;

#[derive(Debug, Default)]
pub(crate) struct Profile {
    pub(crate) aliases: AliasStore,
    /// Your profile scoped variables, which the file saves and every
    /// session on the profile reads under its own.
    pub(crate) vars: VariableStore,
    pub(crate) triggers: TriggerStore,
    /// The tick settings the file saves, with the reset pattern compiled.
    pub(crate) tick: TickSettings,
    pub(crate) ui: UiConfig,
    pub(crate) plugins: PluginsPersist,
    /// Keyboard macro bindings. Each entry maps a canonical key
    /// string (e.g. "F1", "Ctrl+N", "Numpad7") to a command line
    /// (which may itself contain `;`-separated subcommands).
    pub(crate) macros: Vec<Macro>,
    /// Macro groups currently bulk-disabled. A `Macro` whose
    /// `group` is in this set is treated as not-bound — keypresses
    /// fall through as if no macro existed for that key. Mirrors
    /// the per-store `disabled_groups` machinery in `AliasStore` /
    /// `TriggerStore` but lives here directly because there is no
    /// `MacroStore` wrapper.
    pub(crate) disabled_macro_groups: BTreeSet<String>,
    /// Timer groups turned off, kept the way `disabled_macro_groups` is.
    /// A timer in one of them waits as a timer that is off does. Timers
    /// stay in the profile file in loadout mode too, so no loadout turns
    /// these on or off.
    pub(crate) disabled_timer_groups: BTreeSet<String>,
    /// Moves each time `#group` or a Lua `mud.set_group_enabled` turns a
    /// macro group on or off, see [`crate::script::toggle_group`].
    /// [`crate::app::events::ListRevisions`] reads it, so every path that
    /// runs lines or Lua tells the command line, which keeps its own map
    /// of the macro keys that fire. A Settings checkbox and a loadout
    /// switch tell it themselves. A profile switch or replace leaves this
    /// alone.
    pub(crate) macro_group_toggles: u64,
    /// Moves each time a group of any list turns on or off, through
    /// [`crate::script::set_list_group`]: `#group`, Lua, or the switch on a
    /// group heading in Settings. [`crate::app::events::ListRevisions`]
    /// reads it, so an open Settings page shows each switch as it stands.
    pub(crate) group_toggles: u64,
    /// The catalog groups each of this profile's folders became in the
    /// shared catalog, which `#group` follows. See [`GroupFolders`].
    pub(crate) group_folders: GroupFolders,
    /// The custom prompt's `[prompt]` table, which the profile file saves.
    /// It is a copy of the table the prompt engine on a session's
    /// connection runs, and the engine's is the one that counts and the
    /// one `prompt_config_get` answers with. One rule keeps the two the
    /// same. A table from outside the engine goes in through
    /// [`crate::prompt::take_config`], and a step that lets the engine
    /// change its table, as when it follows the game's prompt settings,
    /// copies it back through [`crate::prompt::keep_table`]. A step that
    /// skips the copy leaves the next save writing the old table. The
    /// engines of the other sessions on the profile take only what you
    /// choose, through [`crate::prompt::choose_in_other_sessions`].
    pub(crate) prompt: vosh_prompt::PromptConfig,
    /// Its name in the profile set, which `#profile save` and `#profile
    /// load` find its file by and the custom prompt draws for `%profile`
    /// as Vosh shows it. Its [`OpenProfile`] keeps the same name for the
    /// steps that hold no profile lock, and the two change together. None
    /// before any profile loads.
    ///
    /// [`OpenProfile`]: crate::profile::open::OpenProfile
    pub(crate) name: Option<String>,
    /// Interval timers: each fires its command every `interval_secs`
    /// while connected. Independent of the tick timer (one command on
    /// the game tick) and of Lua `mud.timer` (script callbacks). The
    /// per-timer next-fire deadlines live in the session loop, not
    /// here, so this stays a plain config mirror.
    pub(crate) timers: Vec<Timer>,
    /// What each alert preset does, by preset id, which the file saves
    /// under `[alerts]`. Whether a preset rings is in
    /// `ui.enabled_presets`, see [`crate::alert::presets`]. In loadout
    /// mode catalog.toml keeps them, beside the list of presets that are
    /// on.
    pub(crate) alerts: BTreeMap<String, AlertParts>,
    /// Your edits to the presets, which the file saves under
    /// `[preset_edits]`. In loadout mode catalog.toml keeps them, beside
    /// the list of presets that are on, as it keeps `alerts`.
    pub(crate) preset_edits: PresetEdits,
    /// Vosh dials again after the link drops while you play, see
    /// [`crate::session::reconnect`]. On at first.
    pub(crate) reconnect: OnSwitch,
}

impl Profile {
    /// Whether the interval timer `timer` fires: it is on and its group,
    /// if it has one, is on.
    pub(crate) fn timer_fires(&self, timer: &Timer) -> bool {
        timer.enabled
            && timer.group.as_deref().map_or(true, |g| {
                g.is_empty() || !self.disabled_timer_groups.contains(g)
            })
    }
}

/// One keyboard binding: a canonical key string mapped to a
/// command line. Both halves are user-supplied via the Settings
/// macros tab; the canonical key string is produced by the
/// frontend (`KeyboardEvent` -> normalized identifier) so the
/// backend never has to know about browser key codes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Macro {
    pub(crate) key: String,
    pub(crate) command: String,
    /// Optional group tag for bulk on/off. Mirrors `Alias.group`
    /// and `Trigger.group`. Defaults to `None`; the wire format
    /// omits the field when unset so legacy profile.toml files
    /// keep loading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<String>,
    /// Off keeps the binding but lets the key fall through as if it
    /// were not bound, the way a disabled group does. Defaults to on,
    /// and the wire format omits the field while on so older
    /// profile.toml files and older builds read the same shape.
    #[serde(
        default = "default_macro_enabled",
        skip_serializing_if = "is_macro_enabled"
    )]
    pub(crate) enabled: bool,
    /// The id of the preset that added this macro, None for one of yours.
    /// A preset macro on a key one of yours uses is held off with
    /// `enabled` false, see [`hold_taken_keys`]. Left out of the file while None,
    /// and 0.8.1 skips the key, so it still reads the file.
    ///
    /// [`hold_taken_keys`]: crate::loadouts::presets::hold_taken_keys
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) preset: Option<String>,
}

fn default_macro_enabled() -> bool {
    true
}

// serde's skip_serializing_if hands the field by reference.
fn is_macro_enabled(enabled: &bool) -> bool {
    *enabled
}

/// One interval timer: fire `command` every `interval_secs` seconds
/// while connected. `id` is a stable handle assigned by the backend so
/// runtime next-fire deadlines survive edits. Authored via the Settings
/// timers tab.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Timer {
    pub(crate) id: u32,
    /// Optional label shown in the UI. Empty is fine.
    #[serde(default)]
    pub(crate) name: String,
    pub(crate) interval_secs: u32,
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) enabled: bool,
    /// Optional group tag, as on a macro, which `#group` and the switch
    /// on its heading in Settings turn on and off with the rest of the
    /// group. Left out of the file while unset, so older builds read the
    /// same shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<String>,
}
