//! Per-profile state. Owns the alias engine, variable store, and trigger
//! store; lives across reconnects so user customization survives disconnect
//! cycles.

use mudclient_alias::AliasStore;
use mudclient_script::ScriptEngine;
use mudclient_trigger::TriggerStore;
use mudclient_vars::VariableStore;

use crate::profile_config::UiConfig;
use crate::tick::TickRuntime;

#[derive(Debug, Default)]
pub(crate) struct Profile {
    pub(crate) aliases: AliasStore,
    pub(crate) vars: VariableStore,
    pub(crate) triggers: TriggerStore,
    pub(crate) tick: TickRuntime,
    pub(crate) script: ScriptEngine,
    pub(crate) ui: UiConfig,
}
