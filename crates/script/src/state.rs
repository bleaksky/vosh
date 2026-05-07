//! Shared state stored as Lua app data. Holds pending side effects, the
//! callback registry, and a snapshot of session variables so synchronous
//! getters from inside Lua do not need to lock the Profile.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::RegistryKey;

use crate::actions::Action;

/// Per-engine bookkeeping. Lua references it via `app_data_ref` and we
/// access it from the Rust side via the `Arc<Mutex<>>` clone we hand out.
#[derive(Default)]
pub(crate) struct StateInner {
    /// Side effects queued by Lua API calls. Drained by the engine after
    /// each Lua callback returns.
    pub(crate) pending: Vec<Action>,
    /// Registry keys keyed by callback id. The id is what travels in
    /// `Action::SetLuaTrigger`, `SubscribeGmcp`, and `Timer`.
    pub(crate) callbacks: HashMap<i64, RegistryKey>,
    /// Snapshot of session and profile variables, refreshed by the
    /// caller before each Lua entry. `mud.var(name)` reads from here.
    pub(crate) var_snapshot: HashMap<String, String>,
}

/// Wrapper around the shared inner state. Stored as Lua app data so the
/// `mud.*` API can reach it through `lua.app_data_ref()`.
#[derive(Clone)]
pub(crate) struct EngineState {
    pub(crate) cell: Arc<Mutex<StateInner>>,
}

impl EngineState {
    pub(crate) fn new() -> Self {
        Self {
            cell: Arc::new(Mutex::new(StateInner::default())),
        }
    }
}
