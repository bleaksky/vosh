//! Shared state stored as Lua app data. Holds pending side effects, the
//! callback registry, and a snapshot of session variables so synchronous
//! getters from inside Lua do not need to lock the Profile.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::RegistryKey;

use crate::actions::Action;
use crate::limits::{ACTIONS_PER_CALL, CALL_BYTES};
use crate::owner::Owner;

/// One Lua function Vosh holds for later, and the owner of the call
/// that handed it over.
pub(crate) struct Callback {
    pub(crate) key: RegistryKey,
    pub(crate) owner: Owner,
}

/// The call running now: whose Lua it runs, how many actions and how
/// much text it queued, and whether it queued more than one call may.
pub(crate) struct CallInfo {
    pub(crate) owner: Owner,
    queued: usize,
    bytes: usize,
    /// It queued more than [`ACTIONS_PER_CALL`] actions.
    pub(crate) dropped: bool,
    /// It queued a piece of text past its size limit, or more than
    /// [`CALL_BYTES`] in all.
    pub(crate) text_dropped: bool,
    /// It gave a pane more than [`crate::limits::PANE_BLOCKS`] blocks.
    pub(crate) blocks_dropped: bool,
}

impl CallInfo {
    pub(crate) fn new(owner: Owner) -> Self {
        Self {
            owner,
            queued: 0,
            bytes: 0,
            dropped: false,
            text_dropped: false,
            blocks_dropped: false,
        }
    }
}

/// Per-engine bookkeeping. Lua references it via `app_data_ref` and we
/// access it from the Rust side via the `Arc<Mutex<>>` clone we hand out.
#[derive(Default)]
pub(crate) struct StateInner {
    /// Side effects queued by Lua API calls. Drained by the engine after
    /// each Lua callback returns.
    pub(crate) pending: Vec<Action>,
    /// The functions Vosh holds, keyed by callback id. The id is what
    /// travels in `Action::SetLuaTrigger`, `SubscribeGmcp`, and `Timer`.
    pub(crate) callbacks: HashMap<i64, Callback>,
    /// Callback id of each timer that has not fired yet, by timer id, so
    /// `mud.cancel_timer` can free the callback it will never run.
    pub(crate) timer_callbacks: HashMap<u32, i64>,
    /// Snapshot of session and profile variables, refreshed by the
    /// caller before each Lua entry. `mud.var(name)` reads from here.
    pub(crate) var_snapshot: HashMap<String, String>,
    /// The call running now, while one runs.
    pub(crate) call: Option<CallInfo>,
}

impl StateInner {
    /// The owner of the call running now. A function it hands over keeps
    /// this owner.
    pub(crate) fn owner(&self) -> Owner {
        self.call
            .as_ref()
            .map_or(Owner::Typed, |call| call.owner.clone())
    }

    /// Queue `action` for the call running now. A call may queue
    /// [`ACTIONS_PER_CALL`] actions with [`CALL_BYTES`] of text among
    /// them, and past either each one drops, with the function it would
    /// have registered. True when it queued.
    pub(crate) fn queue(&mut self, action: Action) -> bool {
        if let Some(call) = self.call.as_mut() {
            let bytes = action.text_len();
            if call.queued >= ACTIONS_PER_CALL {
                call.dropped = true;
            } else if call.bytes + bytes > CALL_BYTES {
                call.text_dropped = true;
            } else {
                call.queued += 1;
                call.bytes += bytes;
                self.pending.push(action);
                return true;
            }
            self.forget_registration(&action);
            return false;
        }
        self.pending.push(action);
        true
    }

    /// Note that the call running now asked for a piece of text past its
    /// size limit, which Vosh dropped before it copied it.
    pub(crate) fn drop_long_text(&mut self) {
        if let Some(call) = self.call.as_mut() {
            call.text_dropped = true;
        }
    }

    /// Note that the call running now gave a pane more than
    /// [`crate::limits::PANE_BLOCKS`] blocks, which Vosh dropped past the cap.
    pub(crate) fn drop_blocks(&mut self) {
        if let Some(call) = self.call.as_mut() {
            call.blocks_dropped = true;
        }
    }

    /// Free the function an action that never applies would have
    /// registered.
    pub(crate) fn forget_registration(&mut self, action: &Action) {
        match action {
            Action::SetLuaTrigger { callback_id, .. }
            | Action::SubscribeGmcp { callback_id, .. } => {
                self.callbacks.remove(callback_id);
            }
            Action::Timer {
                callback_id,
                timer_id,
                ..
            } => {
                self.callbacks.remove(callback_id);
                self.timer_callbacks.remove(timer_id);
            }
            _ => {}
        }
    }
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
