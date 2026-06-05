//! Per-profile state. Owns the alias engine, variable store, and trigger
//! store; lives across reconnects so user customization survives disconnect
//! cycles.

use std::collections::{BTreeMap, BTreeSet};

use vosh_alias::AliasStore;
use vosh_script::ScriptEngine;
use vosh_trigger::TriggerStore;
use vosh_vars::VariableStore;

use crate::profile_config::{PluginsPersist, UiConfig};
use crate::tick::TickRuntime;

#[derive(Debug, Default)]
pub(crate) struct Profile {
    pub(crate) aliases: AliasStore,
    pub(crate) vars: VariableStore,
    pub(crate) triggers: TriggerStore,
    pub(crate) tick: TickRuntime,
    pub(crate) script: ScriptEngine,
    pub(crate) ui: UiConfig,
    pub(crate) plugins: PluginsPersist,
    /// Active macro recorder. `Some` between `#record <name>` and
    /// `#endrec`; commands typed in that window get captured into the
    /// buffer and on stop saved as an alias whose expansion is the
    /// `;`-joined sequence.
    pub(crate) recording_macro: Option<MacroRecorder>,
    /// User-controlled target state plus configured quick-key verbs.
    /// `name` clears on disconnect; `quick_keys` persist via
    /// `ProfileConfig` so verb bindings survive restarts.
    pub(crate) target: TargetState,
    /// Latest `Room.Chars` snapshot — kept here so `tar` slash
    /// commands can resolve a numeric index or partial name without
    /// round-tripping through the frontend.
    pub(crate) room_chars: Vec<RoomChar>,
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
    /// Live "prompt vars" — written by user triggers via
    /// `mud.set_prompt_var(name, value)` and read with priority
    /// over GMCP by the vitals template resolver. Session-only;
    /// reset on reconnect like vitals snapshots.
    pub(crate) prompt_vars: BTreeMap<String, String>,
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
}

#[derive(Debug, Clone)]
pub(crate) struct MacroRecorder {
    pub(crate) name: String,
    pub(crate) commands: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TargetState {
    /// User-selected target name, e.g. "The Baron Helgardium". Empty
    /// when no target is set. Session-only — cleared on disconnect.
    pub(crate) name: Option<String>,
    /// 1-based index into `room_chars` for the current target; `None`
    /// when the target isn't in the current room (or no target set).
    pub(crate) room_idx: Option<usize>,
    /// Configurable quick-key slots. Defaults to `gg`/`xx`/`zz`/`tt`
    /// with empty verbs; users edit via `#qkey <name> <verb>`.
    pub(crate) quick_keys: Vec<QuickKey>,
}

impl Default for TargetState {
    fn default() -> Self {
        Self {
            name: None,
            room_idx: None,
            quick_keys: Self::default_quick_keys(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct QuickKey {
    pub(crate) name: String,
    pub(crate) verb: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoomChar {
    pub(crate) name: String,
    pub(crate) npc: bool,
}

impl TargetState {
    pub(crate) fn default_quick_keys() -> Vec<QuickKey> {
        ["gg", "xx", "zz", "tt"]
            .iter()
            .map(|name| QuickKey {
                name: (*name).to_string(),
                verb: String::new(),
            })
            .collect()
    }
}
