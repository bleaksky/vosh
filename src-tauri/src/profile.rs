//! Per-profile state. Owns the alias engine, variable store, and trigger
//! store; lives across reconnects so user customization survives disconnect
//! cycles.

use mudclient_alias::AliasStore;
use mudclient_trigger::TriggerStore;
use mudclient_vars::VariableStore;

#[derive(Debug, Default)]
pub(crate) struct Profile {
    pub(crate) aliases: AliasStore,
    pub(crate) vars: VariableStore,
    pub(crate) triggers: TriggerStore,
}
