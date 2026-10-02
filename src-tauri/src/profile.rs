//! Per-profile state. Owns the alias engine, variable store, and trigger
//! store; lives across reconnects so user customization survives disconnect
//! cycles.

use std::collections::BTreeSet;

use vosh_alias::AliasStore;
use vosh_script::ScriptEngine;
use vosh_trigger::TriggerStore;
use vosh_vars::VariableStore;

use crate::profile_config::{GroupFolders, PluginsPersist, UiConfig};
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
    /// Moves each time `#group` or a Lua `mud.set_group_enabled` turns a
    /// macro group on or off, see [`crate::script_state::toggle_group`].
    /// [`crate::list_events::ListRevisions`] reads it, so every path that
    /// runs lines or Lua tells the command line, which keeps its own map
    /// of the macro keys that fire. A Settings checkbox and a loadout
    /// switch tell it themselves. A profile switch or replace leaves this
    /// alone.
    pub(crate) macro_group_toggles: u64,
    /// The catalog groups each of this profile's folders became in the
    /// shared catalog, which `#group` follows. See [`GroupFolders`].
    pub(crate) group_folders: GroupFolders,
    /// The custom prompt: the profile's `[prompt]` table, and what the
    /// session feeds it, the values triggers write with
    /// `mud.set_prompt_var(name, value)`, the latest packet of each GMCP
    /// package, and the hidden state worked out from them. The table lasts
    /// with the profile. A profile switch keeps the packets and drops the
    /// values, and a disconnect clears both.
    pub(crate) prompt: vosh_prompt::PromptEngine,
    /// The active profile's name as Vosh shows it, `Default` for the
    /// reserved default, which the custom prompt draws for `%profile`.
    /// Set at launch, on a switch and on a rename, so the session reads
    /// it without the profile set's lock. None before any profile loads.
    pub(crate) display_name: Option<String>,
    /// Interval timers: each fires its command every `interval_secs`
    /// while connected. Independent of the tick timer (one command on
    /// the game tick) and of Lua `mud.timer` (script callbacks). The
    /// per-timer next-fire deadlines live in the session loop, not
    /// here, so this stays a plain config mirror.
    pub(crate) timers: Vec<Timer>,
}

impl Profile {
    /// Take a `[prompt]` table. Nothing reads the live `[ui]` copy of its
    /// switch and design, and a save writes the file's copy from this
    /// table, see [`crate::profile_config::ProfileConfig::from_profile`].
    pub(crate) fn set_prompt_config(&mut self, config: vosh_prompt::PromptConfig) {
        self.prompt.set_config(config);
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

#[cfg(test)]
mod tests {
    use super::Macro;

    #[test]
    fn macro_without_enabled_reads_as_on() {
        let m: Macro = serde_json::from_str(r#"{"key":"F1","command":"kick"}"#).unwrap();
        assert!(m.enabled);
        assert_eq!(m.group, None);
    }

    #[test]
    fn macro_omits_enabled_while_on_and_keeps_it_off() {
        let on = Macro {
            key: "F1".into(),
            command: "kick".into(),
            group: None,
            enabled: true,
        };
        let json = serde_json::to_string(&on).unwrap();
        assert!(!json.contains("enabled"), "{json}");

        let off = Macro {
            enabled: false,
            ..on
        };
        let json = serde_json::to_string(&off).unwrap();
        assert!(json.contains(r#""enabled":false"#), "{json}");
        let back: Macro = serde_json::from_str(&json).unwrap();
        assert_eq!(back, off);
    }

    #[test]
    fn macro_enabled_round_trips_through_toml() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Holder {
            macros: Vec<Macro>,
        }
        let holder = Holder {
            macros: vec![
                Macro {
                    key: "F1".into(),
                    command: "kick".into(),
                    group: Some("combat".into()),
                    enabled: false,
                },
                Macro {
                    key: "F2".into(),
                    command: "bash".into(),
                    group: None,
                    enabled: true,
                },
            ],
        };
        let text = toml::to_string(&holder).unwrap();
        let back: Holder = toml::from_str(&text).unwrap();
        assert_eq!(back.macros, holder.macros);
    }
}
