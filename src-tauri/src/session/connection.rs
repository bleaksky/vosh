//! What one connection holds apart from the profile: the target you pick,
//! its quick keys and the characters in the room, which the commands
//! share, the room look and the end of a fight, which the loop follows
//! line by line, and the tick's count and the prompt engine, which both
//! read. The app state holds the [`Connection`] behind a lock of its own,
//! [`SharedConnection`], and the session loop holds a handle to it, so a
//! command reads it without waiting on the loop.
//!
//! Its lock comes after the profile lock and the profile set, never before
//! them. A step that holds it takes no other lock and never awaits. The
//! session slot comes before it, since `disconnect` holds the slot while
//! the loop ends and clears it.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::room_block::RoomBlock;
use crate::tick::TickRuntime;

/// What one connection holds apart from the profile. It outlives each
/// connection, so a target you set offline carries into the next one and
/// your quick keys last until you quit.
#[derive(Debug, Default)]
pub(crate) struct Connection {
    /// Your target plus the quick keys that aim at it. The target clears
    /// on disconnect. The quick keys live in memory only. No profile file
    /// holds them, so a restart brings back the stock gg, xx, zz and tt
    /// slots, as HELP.md says.
    pub(crate) target: TargetState,
    /// The latest Room.Chars list, so `tar` can pick a character by its
    /// place or by part of its name without asking the page.
    pub(crate) room_chars: Vec<RoomChar>,
    /// The room look the session is following, which tells the lines
    /// that list a room's things and people apart for Room triggers. It
    /// resets on a disconnect.
    pub(crate) room_block: RoomBlock,
    /// The round that ended your fight is still coming. `stop_fighting`
    /// (fight.c:10278) writes Char.Combat `{}` straight to the socket in
    /// the middle of the round, and the round's text waits for the end of
    /// the pulse, so the last attacks, the death and the experience all
    /// come after it. Set when a Char.Combat with no target follows one
    /// that named one, and cleared by the prompt, GA or EOR that ends the
    /// pulse, and on a disconnect.
    pub(crate) fight_tail: bool,
    /// The tick's running count. The profile keeps the tick settings,
    /// which each of its methods takes. The session starts the count as
    /// it connects and stops it as it ends.
    pub(crate) tick: TickRuntime,
    /// The custom prompt: the profile's `[prompt]` table compiled for the
    /// stage, and what the session feeds it, the values triggers write
    /// with `mud.set_prompt_var(name, value)`, the latest packet of each
    /// GMCP package, and the hidden state worked out from them. A profile
    /// switch keeps the packets and drops the values, and a disconnect
    /// clears both.
    pub(crate) prompt: vosh_prompt::PromptEngine,
}

impl Connection {
    /// The connection ended, and your target, the room list, the room
    /// look and the fight's tail end with it. Your quick keys stay.
    /// Returns whether a target was set.
    pub(crate) fn clear_on_disconnect(&mut self) -> bool {
        let had = self.target.name.is_some();
        self.target.name = None;
        self.target.room_idx = None;
        self.room_chars.clear();
        self.room_block = RoomBlock::default();
        self.fight_tail = false;
        had
    }
}

/// The handle to the [`Connection`] that the app state, the session loop
/// and the commands share. The loop takes it for every line the game
/// sends, right after the profile lock, and an async mutex there costs
/// each line a poll and a share of the task's cooperative budget, about 3
/// percent of P2. No step holds it across an await, and a task's guard
/// cannot cross one, so a plain mutex fits. A command that finds it held
/// waits on its thread for one step, which the Lua time budget bounds.
#[derive(Debug, Default, Clone)]
pub(crate) struct SharedConnection(Arc<Mutex<Connection>>);

impl SharedConnection {
    /// Lock the connection. A step that panicked while it held the lock
    /// left the connection as the step had it, and the next holder takes
    /// it as it stands.
    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TargetState {
    /// User-selected target name, e.g. "The Baron Grisvald". Empty
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
    use super::{Connection, QuickKey, RoomBlock, RoomChar};

    #[test]
    fn a_disconnect_keeps_the_quick_keys_and_clears_the_rest() {
        let gg = QuickKey {
            name: "gg".into(),
            verb: "kill".into(),
        };
        let mut c = Connection::default();
        c.target.name = Some("goblin".into());
        c.target.room_idx = Some(1);
        c.target.quick_keys = vec![gg.clone()];
        c.room_chars = vec![RoomChar {
            name: "a goblin".into(),
            npc: true,
        }];
        c.room_block.room_chars(1);
        assert_ne!(c.room_block, RoomBlock::default());
        c.fight_tail = true;
        assert!(c.clear_on_disconnect(), "a target was set");
        assert_eq!(c.target.name, None);
        assert_eq!(c.target.room_idx, None);
        let leftover = &c.room_chars;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(c.room_block, RoomBlock::default());
        assert!(!c.fight_tail);
        assert_eq!(c.target.quick_keys, [gg]);
        assert!(!c.clear_on_disconnect(), "no target is left");
    }
}
