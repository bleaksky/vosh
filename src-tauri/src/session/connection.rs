//! All the session state the line pipeline changes, under one std lock
//! that lives as long as the session. That is the target you pick, its
//! quick keys and the characters in the room, which the commands share,
//! the room look and the end of a fight, which the loop follows line by
//! line, the tick's count and the prompt engine, which both read, and the
//! session's variables, its Lua engine, with the aliases its plugins make,
//! the ring of its `[lua]` lines and the macro recorder, and the key its
//! Lua stops go under. The split with
//! [`Session`](crate::sessions::Session) is by lock, not by meaning. The
//! engine, the variables, the recorder, the ring and the plugin aliases
//! belong to the session and outlive each connection, and they sit here
//! because a line changes them together with the rest, beside the
//! profile. The session keeps who it is and the facts leaf locks guard.
//! Each session holds its [`Connection`] behind [`SharedConnection`], and
//! the session loop holds a handle to it, so a command reads it straight
//! from the session and never asks the loop.
//!
//! Its lock comes after the session map, the profile lock and the profile
//! set, never before them. The session slot comes before it, since
//! `disconnect` holds the slot while the loop ends and clears it. No
//! holder awaits, and the only locks a holder takes are leaves: the Lua
//! limits mutex in the script crate for each Lua run, `UNREAD_FILES` in
//! `disk/atomic.rs` for `#profile save` and `#profile load`, and a try of
//! `PERSIST_LOCK` for `#profile save`. [`SharedConnection`] says how long
//! a holder keeps it.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use vosh_automation::alias::PluginAliases;
use vosh_automation::vars::{VarView, VariableStore};
use vosh_automation::StopKey;
use vosh_script::ScriptEngine;

use super::room_block::RoomBlock;
use crate::profile::live::Profile;
use crate::tick::TickRuntime;

/// All the session state the line pipeline changes. Each session holds
/// one, which lives as long as the session and outlives each connection
/// it makes, so a target you set offline carries into the next
/// connection, your quick keys last until the session closes, and the Lua
/// engine runs `#lua` and plugin aliases while no game listens.
#[derive(Debug, Default)]
pub(crate) struct Connection {
    /// Your target plus the quick keys that aim at it. The target clears
    /// on disconnect. The quick keys live in memory only. No profile file
    /// holds them, so a restart, like a new session, brings back the
    /// stock gg, xx, zz and tt slots, as HELP.md says.
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
    /// The round that started your fight came before the Char.Combat that
    /// names your opponent. A server that sends its prompt tick after the
    /// text of the pulse sends the first round of a fight while Char.Combat
    /// still names no one, so an attack line of yours starts the fight's
    /// lines until the prompt, GA or EOR that ends the pulse. Cleared with
    /// [`Connection::fight_tail`].
    pub(crate) fight_head: bool,
    /// The tick's running count. The profile keeps the tick settings,
    /// which each of its methods takes. The session starts the count as
    /// it connects and stops it as it ends.
    pub(crate) tick: TickRuntime,
    /// The custom prompt: the profile's `[prompt]` table compiled for the
    /// stage, and what the session feeds it, the values triggers write
    /// with `mud.set_prompt_var(name, value)`, the latest packet of each
    /// GMCP package, and the hidden state worked out from them. A profile
    /// switch keeps the packets and drops the values, and a disconnect
    /// clears both. The engine's table is the one that counts. The
    /// profile keeps a copy for its file, see
    /// [`crate::profile::live::Profile::prompt`] for the rule that keeps
    /// the two the same.
    pub(crate) prompt: vosh_prompt::PromptEngine,
    /// The session's Lua engine, which runs your scripts and the plugins
    /// the profile turns on. Each session runs its own, so a Lua global,
    /// a Lua trigger or a timer stays in the session that made it, while
    /// what Lua asks of the profile reaches every session on it. It keeps
    /// the latest GMCP packet of each package for a new handler, and
    /// forgets them as a connection ends. No file saves its state.
    pub(crate) script: ScriptEngine,
    /// The aliases the session's plugins made, beside the engine that
    /// runs them, which last while their plugin runs. No profile file
    /// holds them, so a switch or a save leaves them be, and a plugin that
    /// turns off takes its own.
    pub(crate) plugin_aliases: PluginAliases,
    /// The session's variables: those `#var`, `mud.set_var` and `tar`
    /// set and those the GMCP packages bind. They clear as the session
    /// connects, and no file saves them. A lookup reads them over the
    /// profile's through [`Connection::var_view`].
    pub(crate) vars: VariableStore,
    /// The macro recorder, `Some` between `#record <name>` and `#endrec`.
    /// It takes each line you type in this session, and `#endrec` saves
    /// them to the profile as an alias whose expansion is the `;`-joined
    /// sequence.
    pub(crate) recording_macro: Option<MacroRecorder>,
    /// The key the profile's trigger and alias stores hold this session's
    /// Lua stops under, made from the session's id. A trigger or an alias
    /// whose Lua Vosh stopped here stays on in every other session.
    pub(crate) stop_key: StopKey,
    /// What the alert presets follow on the connection: your name, the
    /// low latch on your health and whom you fight. See
    /// [`crate::alert::presets::PresetWatch`].
    pub(crate) preset_watch: crate::alert::presets::PresetWatch,
    /// What decides whether a drop redials: whether you play, a closing
    /// line, a quit of yours, and a character another session took. See
    /// [`crate::session::reconnect::LinkWatch`]. The loop takes it as the
    /// connection ends.
    pub(crate) link: super::reconnect::LinkWatch,
    /// What tells the rows the session logs apart, the Comm.Channel
    /// packets waiting for their line among it. See
    /// [`crate::session::log_kinds::LogKinds`]. It starts over at each
    /// connect.
    pub(crate) log_kinds: super::log_kinds::LogKinds,
    /// The newest `[lua]` lines the session printed and the lines you
    /// typed in the Scripts console, which the Scripts page shows. A
    /// disconnect keeps them, so the lines plugins print at launch and
    /// before a connect stay.
    pub(crate) lua_output: crate::script::output::LuaOutput,
    /// The panes the session's plugins draw with `mud.pane`, beside the
    /// aliases they make. A disconnect keeps them, since only the plugin
    /// that draws a pane changes or removes it.
    pub(crate) lua_panes: crate::script::panes::LuaPanes,
    /// The round trip to the game and the stalls since you connected,
    /// which the loop records every two seconds and `#lag` prints. See
    /// [`crate::session::round_trip`].
    pub(crate) round_trip: super::round_trip::RoundTrip,
    /// The players the session snoops, one tab each with its text. A
    /// link that ends marks them ended and keeps them, so only the
    /// session that closes drops them.
    pub(crate) snoops: super::snoop::Snoops,
}

impl Connection {
    /// The connection ended, and your target with the variable that
    /// mirrors it, the room list, the room look and the fight's tail end
    /// with it. Your quick keys stay. Returns whether a target was set.
    pub(crate) fn clear_on_disconnect(&mut self) -> bool {
        let had = self.target.name.is_some();
        self.target.name = None;
        self.target.room_idx = None;
        self.vars.remove("target");
        self.room_chars.clear();
        self.room_block = RoomBlock::default();
        self.fight_tail = false;
        self.fight_head = false;
        self.preset_watch.reset();
        had
    }

    /// The session's variables over those of `profile`, the one it plays.
    pub(crate) fn var_view<'a>(&'a self, profile: &'a Profile) -> VarView<'a> {
        VarView {
            session: &self.vars,
            profile: &profile.vars,
        }
    }
}

/// The handle to the [`Connection`] that the session, its loop and the
/// commands share. The loop takes it for every line the game sends, right
/// after the profile lock, and an async mutex there costs each line a
/// poll and a share of the task's cooperative budget, about 3 percent of
/// the output throughput test. No step holds it across an await, and a
/// task's guard cannot cross one, so a plain mutex fits.
///
/// The price is that a waiter blocks its runtime thread instead of
/// yielding, for as long as the holder keeps the guard. A line the game
/// sends keeps it through its triggers and its Lua, each Lua call within
/// its time budget. A line you type keeps it through the whole input
/// pipeline, file work included: `#profile save` writes the profile file
/// and its backups, `#profile load` reads and parses one, and `#script
/// load` and `#script reload` read scripts. A profile switch keeps it
/// while it reads each plugin the next profile turns on and runs its
/// entry script, one Lua budget per plugin in a row. A command such as
/// `target_get` that comes in the middle waits on its thread through all
/// of it, so on a machine with few cores a switch with plugins can hold
/// up the other tasks until it ends.
#[derive(Debug, Clone)]
pub(crate) struct SharedConnection(Arc<Mutex<Connection>>);

impl SharedConnection {
    pub(crate) fn new(connection: Connection) -> Self {
        Self(Arc::new(Mutex::new(connection)))
    }

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
    /// with empty verbs; users edit via `#qkey <name> <verb>`. They
    /// belong to the session. Each new session starts with the stock
    /// slots, and they last until it closes. They sit here because every
    /// target payload carries them, and `target_get` and the end of a
    /// connection build one under this lock alone.
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

#[derive(Debug, Clone)]
pub(crate) struct MacroRecorder {
    pub(crate) name: String,
    pub(crate) commands: Vec<String>,
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
        c.fight_head = true;
        c.vars.set("target", "goblin");
        assert!(c.clear_on_disconnect(), "a target was set");
        assert_eq!(c.vars.get("target"), None);
        assert_eq!(c.target.name, None);
        assert_eq!(c.target.room_idx, None);
        let leftover = &c.room_chars;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(c.room_block, RoomBlock::default());
        assert!(!c.fight_tail);
        assert!(!c.fight_head);
        assert_eq!(c.target.quick_keys, [gg]);
        assert!(!c.clear_on_disconnect(), "no target is left");
    }
}
