//! The sessions the app holds. Each [`Session`] keeps what one connection
//! holds apart from the profile, and [`Sessions`] lists them in order
//! with the one selected. The app starts with one session, selected, and
//! a command that names no session acts on the selected one.
//!
//! The map's lock comes ahead of every other lock of the app. A step
//! takes it only to find, add or remove a session or to read or change
//! the selection, and no holder awaits. A holder takes no other lock but
//! one: a change of selection shows the grid of the session it selects,
//! which takes the native grid map and then the pointer's state, and
//! neither of those holders ever takes the session map. No step takes it
//! while it holds a session slot, a profile or a connection, so each step
//! resolves its session before it takes any other lock.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tracing::warn;

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

impl SessionId {
    /// The session the app starts with.
    pub(crate) const FIRST: Self = Self(1);
}

/// What one session holds apart from the profile: the task that runs its
/// connection, what that connection shares with the commands, its Lua
/// timers and scrollback, the count of what reached its terminal, and
/// what the app keeps about its connection, the host and port, the
/// character logged in, the terminal size and the last affects.
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
    /// The terminal lines the plugins printed as they loaded into the
    /// session's engine, at launch or as the session opened, their
    /// `[lua]` errors and stops among them. No terminal shows the session
    /// yet, so they wait for the first connect or the first line you
    /// type, see [`crate::app::plugins::show_launch_lines`].
    pub(crate) launch_lua_lines: std::sync::Mutex<Vec<String>>,
    /// How many outputs reached the session's terminal, repaints aside.
    /// The session loop notes it after each of its writes, and a count
    /// that moved since means output from elsewhere, such as a slash
    /// command's echo, landed after the open row and closed it. Output in
    /// another session never moves it.
    output_count: AtomicU64,
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
            output_count: AtomicU64::new(0),
        }
    }

    /// How many outputs reached the session's terminal so far, see
    /// [`Session::count_output`].
    pub(crate) fn output_count(&self) -> u64 {
        self.output_count.load(Ordering::Acquire)
    }

    /// Count one output that reached the session's terminal. Returns the
    /// count after it.
    pub(crate) fn count_output(&self) -> u64 {
        self.output_count.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Send `event` with `payload`, which serializes as an object, and
    /// the session's id beside its fields, `{session, ..payload}`, so the
    /// page can tell which session it came from.
    pub(crate) fn emit<R: tauri::Runtime, T: Serialize>(
        &self,
        app: &AppHandle<R>,
        event: &str,
        payload: &T,
    ) {
        let named = Named {
            session: self.id,
            payload,
        };
        if let Err(e) = app.emit(event, &named) {
            warn!(error = %e, event, "failed to emit a session event");
        }
    }
}

/// A session event's payload with the session that sent it.
#[derive(Serialize)]
struct Named<'a, T> {
    session: SessionId,
    #[serde(flatten)]
    payload: &'a T,
}

/// The sessions in the order the window lists them, and the one selected.
pub(crate) struct Sessions {
    list: Vec<Arc<Session>>,
    /// Always names a session in `list`.
    selected: SessionId,
    /// The number the next session takes. It only grows, so a late event
    /// from a session that closed never names a newer one.
    next: u32,
}

impl Sessions {
    /// Add a session after the others, with nothing connected, and return
    /// it. It plays the live profile, the one every session plays.
    pub(crate) fn open(&mut self) -> Arc<Session> {
        let session = Arc::new(Session::new(SessionId(self.next)));
        self.next += 1;
        self.list.push(session.clone());
        session
    }

    /// Select the session `id` names, and show its grid in the place of
    /// the grid that showed. Returns false, and keeps the selection, when
    /// the list does not hold it.
    pub(crate) fn select(&mut self, id: SessionId) -> bool {
        let held = self.list.iter().any(|session| session.id == id);
        if held && id != self.selected {
            self.selected = id;
            #[cfg(any(native_surface, test))]
            crate::native::grid::show(id);
            #[cfg(native_surface)]
            crate::native::surface::grid_shown();
        }
        held
    }

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
        Self {
            list: vec![Arc::new(Session::new(SessionId::FIRST))],
            selected: SessionId::FIRST,
            next: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::{json, Value};
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tauri::{App, Listener};

    use super::{Session, SessionId, NO_SUCH_SESSION};
    use crate::app::events;
    use crate::app::state::AppState;
    use crate::session::{StatePayload, TargetPayload};

    fn app() -> App<MockRuntime> {
        mock_builder()
            .build(mock_context(noop_assets()))
            .expect("a mock app")
    }

    /// Every payload of `event` the app sends from now on.
    fn hear(app: &App<MockRuntime>, event: &'static str) -> Arc<Mutex<Vec<Value>>> {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let keep = heard.clone();
        app.listen_any(event, move |e| {
            let payload = serde_json::from_str(e.payload()).expect("a JSON payload");
            keep.lock().expect("the payloads").push(payload);
        });
        heard
    }

    #[test]
    fn emit_names_the_session_beside_a_structs_fields_and_an_enums_kind() {
        let app = app();
        let states = hear(&app, events::STATE);
        let targets = hear(&app, events::TARGET);
        let session = Session::new(SessionId(7));
        session.emit(
            app.handle(),
            events::STATE,
            &StatePayload::Connected {
                host: "localhost".into(),
                port: 4000,
                tls: false,
            },
        );
        session.emit(
            app.handle(),
            events::TARGET,
            &TargetPayload {
                name: Some("goblin".into()),
                room_idx: Some(2),
                quick_keys: Vec::new(),
            },
        );
        assert_eq!(
            *states.lock().unwrap(),
            [json!({
                "session": 7,
                "kind": "connected",
                "host": "localhost",
                "port": 4000,
                "tls": false,
            })]
        );
        assert_eq!(
            *targets.lock().unwrap(),
            [json!({"session": 7, "name": "goblin", "room_idx": 2, "quick_keys": []})]
        );
    }

    #[test]
    fn an_echo_in_one_session_leaves_the_other_sessions_count_alone() {
        // The echo reaches the session's grid too, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let app = app();
        let outputs = hear(&app, events::OUTPUT);
        let (one, two) = (Session::new(SessionId(1)), Session::new(SessionId(2)));
        crate::output::echo_lines(app.handle(), &two, &["You wave.".to_string()]);
        assert_eq!((one.output_count(), two.output_count()), (0, 1));
        assert_eq!(outputs.lock().unwrap()[0]["session"], 2);
    }

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

    #[test]
    fn an_echo_in_a_session_behind_reaches_its_grid_and_asks_for_no_frame() {
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let app = app();
        let frames = hear(&app, crate::output::TEST_FRAME_EVENT);
        let state = AppState::default();
        let (one, two) = (state.selected_session(), state.open_session());
        crate::output::echo_lines(app.handle(), &two, &["You wave.".to_string()]);
        assert_eq!(frames.lock().unwrap().len(), 0);
        crate::output::echo_lines(app.handle(), &one, &["You nod.".to_string()]);
        assert_eq!(frames.lock().unwrap().len(), 1);
        let first_row = |id| crate::native::grid::screen_rows(id).map(|r| r.rows[0].clone());
        assert_eq!(first_row(one.id).as_deref(), Some("You nod."));
        assert_eq!(first_row(two.id).as_deref(), Some("You wave."));
        // Once selected, the second session's grid shows and its echo
        // asks for a frame.
        assert_eq!(state.select_session(two.id), Ok(()));
        crate::output::echo_lines(app.handle(), &two, &["You bow.".to_string()]);
        assert_eq!(frames.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_new_session_takes_the_next_number_and_a_selection_needs_one_vosh_holds() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let state = AppState::default();
        let (two, three) = (state.open_session().id, state.open_session().id);
        assert_eq!((two, three), (SessionId(2), SessionId(3)));
        assert_eq!(state.selected_session().id, SessionId(1));
        assert_eq!(state.select_session(three), Ok(()));
        assert_eq!(state.session(None).map(|s| s.id), Ok(three));
        assert_eq!(
            state.select_session(SessionId(9)),
            Err(NO_SUCH_SESSION.to_string())
        );
        assert_eq!(state.selected_session().id, three);
    }
}
