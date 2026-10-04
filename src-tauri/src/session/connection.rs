//! What one connection shares with the commands: the target you pick, its
//! quick keys and the characters in the room. The app state holds the
//! [`Connection`] behind its own lock, and the session loop holds a handle
//! to it, so a command reads it without waiting on the loop.
//!
//! Its lock comes after the profile lock and the profile set, never before
//! them. A step that holds it takes no other lock and never awaits. The
//! session slot comes before it, since `disconnect` holds the slot while
//! the loop ends and clears it.

/// What one connection shares with the commands. It outlives each
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
}

impl Connection {
    /// The connection ended, and your target and the room list end with
    /// it. Your quick keys stay. Returns whether a target was set.
    pub(crate) fn clear_on_disconnect(&mut self) -> bool {
        let had = self.target.name.is_some();
        self.target.name = None;
        self.target.room_idx = None;
        self.room_chars.clear();
        had
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
    use super::{Connection, QuickKey, RoomChar};

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
        assert!(c.clear_on_disconnect(), "a target was set");
        assert_eq!(c.target.name, None);
        assert_eq!(c.target.room_idx, None);
        let leftover = &c.room_chars;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(c.target.quick_keys, [gg]);
        assert!(!c.clear_on_disconnect(), "no target is left");
    }
}
