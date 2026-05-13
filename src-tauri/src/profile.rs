//! Per-profile state. Owns the alias engine, variable store, and trigger
//! store; lives across reconnects so user customization survives disconnect
//! cycles.

use mudclient_alias::AliasStore;
use mudclient_script::ScriptEngine;
use mudclient_trigger::TriggerStore;
use mudclient_vars::VariableStore;

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
}

#[derive(Debug, Clone)]
pub(crate) struct MacroRecorder {
    pub(crate) name: String,
    pub(crate) commands: Vec<String>,
}
