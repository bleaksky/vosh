//! The sessions the app holds. Each [`Session`] keeps what one connection
//! holds apart from the profile, and [`Sessions`] lists them in order
//! with the one selected. The app starts with one session, selected, and
//! a command that names no session acts on the selected one.
//!
//! The map's lock comes ahead of every other lock. A step takes it only
//! to find, add or remove a session or read the selection, and no holder
//! awaits or takes another lock. No step takes it while it holds a
//! session slot, a profile or a connection, so each step resolves its
//! session before it takes any other lock.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::affects::snapshot::AffectsSnapshot;
use crate::logs::SharedScrollback;
use crate::script::SharedTimers;
use crate::session::connection::SharedConnection;
use crate::session::SessionHandle;

/// What a command says when it names a session Vosh does not hold.
pub(crate) const NO_SUCH_SESSION: &str = "Vosh has no such session.";

/// A session's number, which no other session of this run shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct SessionId(u32);

/// What one session holds apart from the profile: the task that runs its
/// connection, what that connection shares with the commands, its Lua
/// timers and scrollback, and what the app keeps about its connection,
/// the host and port, the character logged in, the terminal size and the
/// last affects.
pub(crate) struct Session {
    pub(crate) id: SessionId,
    /// The handle to the task that runs the connection, while one runs.
    /// It comes before every lock the task takes, since `disconnect`
    /// holds it while the task ends, and the task locks the profile, the
    /// connection, the log and the scrollback as it ends.
    pub(crate) slot: Mutex<Option<SessionHandle>>,
    /// What one connection holds apart from the profile, your target and
    /// the room list among it. It outlives each connection of the
    /// session, and the session loop holds a handle to it. See
    /// [`crate::session::connection`] for where its lock sits.
    pub(crate) connection: SharedConnection,
    /// The `mud.timer` timers the session loop fires.
    pub(crate) lua_timers: SharedTimers,
    /// The ring of recent lines, which launch reads from scrollback.txt
    /// and the session saves back to it as it ends.
    pub(crate) scrollback: SharedScrollback,
    /// Last terminal size the frontend reported, kept while no connection
    /// runs so a fresh connect seeds the telnet `Negotiator` with the real
    /// (cols, rows) instead of the 80×24 default. Without it the server's
    /// first NAWS reply carried 80 cols and wrapped early output (login
    /// banner, `who`, motd) until you nudged the window. The work inside
    /// the lock is two integer copies, so a std mutex fits.
    pub(crate) window_size: std::sync::Mutex<(u16, u16)>,
    /// The host and port of the live connection. Set when a connect
    /// starts, cleared on disconnect. The Char.Status login path reads
    /// it so the resolver knows which world's profile to pick. The work
    /// inside the lock is a clone, so a std mutex fits.
    pub(crate) current_connection: std::sync::Mutex<Option<(String, u16)>>,
    /// The last character name Char.Status or Char.Name gave on the live
    /// connection. Cleared on connect and disconnect. The game resends
    /// Char.Status on every vitals update, and only a new name is a login.
    pub(crate) current_character: std::sync::Mutex<Option<String>>,
    /// The last Char.Affects list of the connection, for a window that
    /// opens between ticks. Cleared on connect and when the connection
    /// ends.
    pub(crate) last_affects: AffectsSnapshot,
    /// The prompt card watches your prompt, so `session://prompt-state`
    /// follows each prompt the session reads.
    pub(crate) prompt_watch: AtomicBool,
    /// You are selecting text in xterm or reading back in its split, as
    /// the webview last said. A clock repaint of your prompt waits while
    /// it holds, so the row you select or read never moves.
    pub(crate) reader_busy: AtomicBool,
    /// The terminal lines the plugins printed as they loaded at launch,
    /// their `[lua]` errors and stops among them. Launch runs before any
    /// window listens, so they wait for the first connect or the first
    /// line you type, see [`crate::app::plugins::show_launch_lines`].
    pub(crate) launch_lua_lines: std::sync::Mutex<Vec<String>>,
}

impl Session {
    fn new(id: SessionId) -> Self {
        Self {
            id,
            slot: Mutex::new(None),
            connection: SharedConnection::default(),
            lua_timers: SharedTimers::default(),
            scrollback: SharedScrollback::default(),
            // Matches `Negotiator::default()`, so a connect that comes
            // before the page reports a size, such as an early automated
            // connect from a script, still gets a sensible baseline.
            window_size: std::sync::Mutex::new((80, 24)),
            current_connection: std::sync::Mutex::new(None),
            current_character: std::sync::Mutex::new(None),
            last_affects: AffectsSnapshot::default(),
            prompt_watch: AtomicBool::new(false),
            reader_busy: AtomicBool::new(false),
            launch_lua_lines: std::sync::Mutex::new(Vec::new()),
        }
    }
}

/// The sessions in the order the window lists them, and the one selected.
pub(crate) struct Sessions {
    list: Vec<Arc<Session>>,
    /// Always names a session in `list`.
    selected: SessionId,
}

impl Sessions {
    /// The session `id` names, while the list holds it.
    pub(crate) fn get(&self, id: SessionId) -> Option<Arc<Session>> {
        self.list.iter().find(|session| session.id == id).cloned()
    }

    /// The selected session.
    pub(crate) fn selected(&self) -> Arc<Session> {
        self.get(self.selected)
            .expect("the selected session is in the list")
    }
}

impl Default for Sessions {
    /// One session, selected.
    fn default() -> Self {
        let first = SessionId(1);
        Self {
            list: vec![Arc::new(Session::new(first))],
            selected: first,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionId, NO_SUCH_SESSION};
    use crate::app::state::AppState;

    #[test]
    fn a_command_acts_on_the_session_it_names_or_on_the_selected_one() {
        let state = AppState::default();
        let selected = state.selected_session().id;
        assert_eq!(state.session(None).map(|s| s.id), Ok(selected));
        assert_eq!(state.session(Some(selected)).map(|s| s.id), Ok(selected));
        assert_eq!(
            state.session(Some(SessionId(2))).map(|s| s.id),
            Err(NO_SUCH_SESSION.to_string())
        );
    }
}
