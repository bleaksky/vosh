//! What one connection shares with the commands: the target you pick, its
//! quick keys and the characters in the room.

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
