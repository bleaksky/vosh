//! The sessions the app holds. Each [`Session`] keeps who one session is,
//! with its [`Connection`], and points at the profile it plays, and
//! [`Sessions`] lists them in order with the one selected, beside the
//! profiles they play, each open once. The app starts with one session,
//! selected, and launch puts the sessions profiles.toml lists in its
//! place. A command that names no session acts on the selected one.
//!
//! The map's lock comes after a session's slot, the save lock and the
//! turn [`broadcast_sessions`] holds while it reads the rows and sends
//! them, and ahead of every other lock of the app. A step takes it only
//! to find, add or remove a session or an open profile or to read or
//! change the selection, and no holder awaits. A holder takes no other
//! lock but leaf locks, a session's profile pointer and an open profile's
//! name, and one more: a change of selection shows the grid of the
//! session it selects, which takes the native grid map and then the
//! pointer's state, and neither of those holders ever takes the session
//! map. No step takes it while it holds the loadouts, a profile, the
//! profile set or a connection, so each step resolves its session before
//! it takes any of them.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tracing::warn;
use vosh_automation::StopKey;

use crate::affects::full::AffectFull;
use crate::app::events::{broadcast, SESSIONS_CHANGED};
use crate::app::state::AppState;
use crate::logs::SharedScrollback;
use crate::profile::live::Profile;
use crate::profile::open::{OpenProfile, ProfileGuard};
use crate::profile::set::SessionEntry;
use crate::profile::worlds::{host_key, known_world, world_label};
use crate::script::SharedTimers;
use crate::session::connection::{Connection, SharedConnection};
use crate::session::last_packages::LastPackages;
use crate::session::SessionHandle;

/// What a command says when it names a session Vosh does not hold.
pub(crate) const NO_SUCH_SESSION: &str = "Vosh has no such session.";

/// What closing the only session says. The page closes the window
/// instead.
pub(crate) const ONLY_SESSION: &str =
    "You cannot close your only session. Close the window instead.";

/// Where a session connects: the host, the port and whether it uses TLS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Address {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) tls: bool,
}

/// A session's number, which no other session of this run shares. A
/// session restored at launch keeps the number it had, which names its
/// scrollback file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct SessionId(u32);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl SessionId {
    /// The session the app starts with.
    pub(crate) const FIRST: Self = Self(1);

    /// The session numbered `n`, for a test that writes a session list.
    #[cfg(test)]
    pub(crate) const fn numbered(n: u32) -> Self {
        Self(n)
    }

    /// The session numbered `n`, as a banner Vosh posted or a snoop
    /// window's label names it. It may name a session that has since
    /// closed, which a lookup then refuses.
    pub(crate) const fn from_number(n: u32) -> Self {
        Self(n)
    }

    /// The key the profile's stores hold the session's Lua stops under.
    pub(crate) fn stop_key(self) -> StopKey {
        StopKey(self.0)
    }
}

/// Who one session is, and the facts about it that leaf locks guard. The
/// split with [`Connection`] is by lock, not by meaning. All the session
/// state the line pipeline changes sits in its [`Connection`], under one
/// std lock that lives as long as the session. Here sit the name you gave
/// it, where it dials and the character it played last, which the row
/// keeps once the connection ends, the host, port and character of the
/// live connection, which a disconnect clears, the terminal size, the
/// last affects and their fulls, beside
/// the task that runs its connection, its Lua timers, its scrollback and
/// the count of what reached its terminal. It points at the profile it
/// plays.
pub(crate) struct Session {
    pub(crate) id: SessionId,
    /// The name you gave the session, which its row and a line in another
    /// session read in place of its character. A leaf lock, held for a
    /// copy.
    name: std::sync::Mutex<Option<String>>,
    /// Where the session dials: where it last connected, or where the
    /// session form set it to dial since. It outlives the connection, so
    /// the row still names the world once the session disconnects. A leaf
    /// lock, held for a copy.
    pub(crate) address: std::sync::Mutex<Option<Address>>,
    /// The profile the session plays. A leaf lock, held for a copy. A
    /// switch moves it while it holds the profile it leaves locked, see
    /// [`Session::lock_profile`].
    profile: std::sync::Mutex<Arc<OpenProfile>>,
    /// The handle to the task that runs the connection, while one runs.
    /// It comes before every lock the task takes, since `disconnect`
    /// holds it while the task ends, and the task locks the profile, the
    /// connection, the log and the scrollback as it ends.
    pub(crate) slot: Mutex<Option<SessionHandle>>,
    /// All the session state the line pipeline changes, your target, the
    /// room list and the Lua engine among it. It outlives each connection
    /// of the session, and the session loop holds a handle to it. See
    /// [`crate::session::connection`] for where its lock sits.
    pub(crate) connection: SharedConnection,
    /// The `mud.timer` timers the session loop fires. They sit apart from
    /// the engine, under a lock of their own, so the loop's poll finds
    /// none due without the profile lock that the engine's needs.
    pub(crate) lua_timers: SharedTimers,
    /// The ring of recent lines, which launch, or a restored session's
    /// first selection, reads from the session's scrollback file, and the
    /// session saves back to it as it ends.
    pub(crate) scrollback: SharedScrollback,
    /// Last terminal size the frontend reported, kept while no connection
    /// runs so a fresh connect seeds the telnet `Negotiator` with the real
    /// (cols, rows) instead of the 80×24 default. Without it the server's
    /// first NAWS reply carried 80 cols and wrapped early output (login
    /// banner, `who`, motd) until you nudged the window. The work inside
    /// the lock is two integer copies, so a std mutex fits.
    pub(crate) window_size: std::sync::Mutex<(u16, u16)>,
    /// The host and port of the live connection. Set when a connect
    /// starts, cleared on disconnect, where `address` keeps the last one.
    /// The Char.Status login path reads it so the resolver knows which
    /// world's profile to pick. The work inside the lock is a clone, so a
    /// std mutex fits.
    pub(crate) current_connection: std::sync::Mutex<Option<(String, u16)>>,
    /// The last character name Char.Status or Char.Name gave on the live
    /// connection. Cleared on connect and disconnect. The game resends
    /// Char.Status on every vitals update, and only a new name is a login.
    pub(crate) current_character: std::sync::Mutex<Option<String>>,
    /// The character the session played last, which its row names once
    /// the live connection has none: through a drop, every try of a
    /// redial and a disconnect, so two sessions that redial on one world
    /// still read apart (Sessions Q10, board 3). A login sets it, and a
    /// connect you start clears it, since the row then names the world
    /// until you log in. A leaf lock, held for a copy.
    played: std::sync::Mutex<Option<String>>,
    /// When the live link reached the game, in Unix ms, for the time
    /// online the row shows. A connect or a redial that reaches the game
    /// sets it, and the end of the link clears it. A leaf lock, held for
    /// a copy.
    since: std::sync::Mutex<Option<u64>>,
    /// The last Char.Affects, Char.Vitals and Char.Combat of the
    /// connection, for a window that opens between packets. Cleared on
    /// connect and when the connection ends.
    pub(crate) last_packages: LastPackages,
    /// How full each affect on the connection's character was cast, for
    /// the Affects pane's gauges. Forgotten on connect, and written to
    /// the file every session shares as the connection ends. See
    /// [`crate::affects::full`].
    pub(crate) affect_full: AffectFull,
    /// The prompt card watches your prompt, so `session://prompt-state`
    /// follows each prompt the session reads.
    pub(crate) prompt_watch: AtomicBool,
    /// A footer or the status line draws your vitals text, and at what
    /// width, so `session://vitals-text` follows what moves it.
    pub(crate) vitals_watch: crate::session::vitals_text::VitalsWatch,
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
    /// When each alert of the session last rang, for the 10 second cap. A
    /// leaf lock, taken alone once the profile and connection let go.
    alert_caps: std::sync::Mutex<crate::alert::Caps>,
    /// The logs the session opened since Vosh started, oldest first, by
    /// their ids in logs.sqlite, which the log view's This session reads.
    /// The newest is the open one while the connection runs. A leaf
    /// lock, held for a copy.
    logs: std::sync::Mutex<Vec<i64>>,
    /// The series of redials the session runs after a drop, if any. A
    /// leaf lock, held to start, take or wake one. See
    /// [`crate::session::reconnect`].
    redial: std::sync::Mutex<Option<crate::session::reconnect::Redial>>,
    /// A redial opened the connection that runs and the game's prompt
    /// has yet to come, which rings the Connection alert. Only
    /// [`crate::session::reconnect`] reads or sets it.
    pub(crate) awaiting_game_prompt: crate::session::reconnect::AwaitingPrompt,
}

impl Session {
    fn new(id: SessionId, profile: Arc<OpenProfile>) -> Self {
        Self {
            id,
            name: std::sync::Mutex::new(None),
            address: std::sync::Mutex::new(None),
            profile: std::sync::Mutex::new(profile),
            slot: Mutex::new(None),
            connection: SharedConnection::new(Connection {
                stop_key: id.stop_key(),
                ..Connection::default()
            }),
            lua_timers: SharedTimers::default(),
            scrollback: SharedScrollback::default(),
            // Matches `Negotiator::default()`, so a connect that comes
            // before the page reports a size, such as an early automated
            // connect from a script, still gets a sensible baseline.
            window_size: std::sync::Mutex::new((80, 24)),
            current_connection: std::sync::Mutex::new(None),
            current_character: std::sync::Mutex::new(None),
            played: std::sync::Mutex::new(None),
            since: std::sync::Mutex::new(None),
            last_packages: LastPackages::default(),
            affect_full: AffectFull::default(),
            prompt_watch: AtomicBool::new(false),
            vitals_watch: crate::session::vitals_text::VitalsWatch::default(),
            reader_busy: AtomicBool::new(false),
            launch_lua_lines: std::sync::Mutex::new(Vec::new()),
            output_count: AtomicU64::new(0),
            alert_caps: std::sync::Mutex::new(crate::alert::Caps::default()),
            logs: std::sync::Mutex::new(Vec::new()),
            redial: std::sync::Mutex::new(None),
            awaiting_game_prompt: crate::session::reconnect::AwaitingPrompt::default(),
        }
    }

    /// The profile the session plays. A session launch restored plays
    /// defaults that wait under its last profile's name and never save,
    /// until its first selection opens or joins that profile, see
    /// [`OpenProfile::waiting`].
    pub(crate) fn profile(&self) -> Arc<OpenProfile> {
        self.profile
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Lock the profile the session plays. A switch moves the session
    /// while it holds the profile it leaves, so a step that waited for
    /// that lock finds the session gone from it and takes the next one.
    pub(crate) async fn lock_profile(&self) -> ProfileGuard {
        loop {
            let guard = ProfileGuard::lock(self.profile()).await;
            let plays = Arc::ptr_eq(
                guard.open(),
                &self
                    .profile
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            );
            if plays {
                return guard;
            }
        }
    }

    /// Point the session at `open`. Call with the profile it plays now
    /// locked, and `open` too, so no step of the session runs between
    /// the two.
    pub(crate) fn play(&self, open: Arc<OpenProfile>) {
        *self
            .profile
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = open;
    }

    /// The logs the session opened since Vosh started, oldest first.
    pub(crate) fn logs(&self) -> Vec<i64> {
        self.logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The session's connection opened log `id`.
    pub(crate) fn note_log(&self, id: i64) {
        self.logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(id);
    }

    /// The name you gave the session, if any.
    pub(crate) fn name(&self) -> Option<String> {
        self.name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Name the session `name`, or with none, or a blank one, read its
    /// character again.
    pub(crate) fn rename(&self, name: Option<&str>) {
        let name = name.map(str::trim).filter(|name| !name.is_empty());
        *self
            .name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = name.map(str::to_string);
    }

    /// The character logged in on the live connection, if any. A drop
    /// keeps it until the next connect.
    pub(crate) fn character(&self) -> Option<String> {
        self.current_character
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The character the session's row names: the one logged in on the
    /// live connection, else the one it played last.
    pub(crate) fn played(&self) -> Option<String> {
        self.character().or_else(|| {
            self.played
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        })
    }

    /// A login named `character`, which the row keeps until the next
    /// connect you start.
    pub(crate) fn note_played(&self, character: &str) {
        *self
            .played
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(character.to_string());
    }

    /// A connect you started, which plays no character until you log in,
    /// so the row names the world it dials.
    pub(crate) fn forget_played(&self) {
        *self
            .played
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    /// Note when the live link reached the game, or None as it ends, see
    /// [`Session::since`].
    pub(crate) fn set_since(&self, since: Option<u64>) {
        *self
            .since
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = since;
    }

    /// Whether the session's connection runs. The loop marks its tick
    /// count in session as it starts and out as it ends, see
    /// [`crate::tick::TickRuntime::in_session`]. Takes the connection
    /// lock.
    pub(crate) fn connected(&self) -> bool {
        self.connection.lock().tick.in_session
    }

    /// Where the session dials, if anywhere yet.
    fn address(&self) -> Option<Address> {
        self.address
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// What a line in another session or a banner calls this one, as its
    /// row reads among `others`, the other open sessions. See
    /// [`label_of`].
    pub(crate) fn label(&self, others: &[Arc<Session>]) -> Option<String> {
        let address = self.address();
        let at = address.as_ref().map(|a| (a.host.as_str(), a.port));
        let own = address.as_ref().map(|a| host_key(&a.host));
        let shares_host = own.is_some_and(|own| {
            others
                .iter()
                .filter_map(|other| other.address())
                .any(|other| host_key(&other.host) == own)
        });
        label_of(
            self.name().as_deref(),
            self.played().as_deref(),
            at,
            shares_host,
        )
    }

    /// The session's row in the list the window shows. Takes the
    /// connection lock, so call it with no profile held.
    pub(crate) fn row(&self, selected: bool) -> SessionRow {
        let address = self.address();
        SessionRow {
            id: self.id,
            name: self.name(),
            character: self.played(),
            host: address.as_ref().map(|a| a.host.clone()),
            port: address.as_ref().map(|a| a.port),
            tls: address.is_some_and(|a| a.tls),
            profile: self.profile().name(),
            connected: self.connected(),
            since: *self
                .since
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            selected,
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

    /// The host and port of the live connection, which a drop keeps until
    /// the next connect.
    pub(crate) fn live_address(&self) -> Option<(String, u16)> {
        self.current_connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Keep `redial`, the series of redials the session now runs.
    pub(crate) fn start_redial(&self, redial: crate::session::reconnect::Redial) {
        *self
            .redial
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(redial);
    }

    /// Take the series of redials the session runs, to end it.
    pub(crate) fn take_redial(&self) -> Option<crate::session::reconnect::Redial> {
        self.redial
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    /// Take the series of redials the session runs when `matches` holds
    /// for it, to end it.
    pub(crate) fn take_redial_if(
        &self,
        matches: impl FnOnce(&crate::session::reconnect::Redial) -> bool,
    ) -> Option<crate::session::reconnect::Redial> {
        let mut redial = self
            .redial
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if redial.as_ref().is_some_and(matches) {
            redial.take()
        } else {
            None
        }
    }

    /// Dial at once in the series the session runs. Returns false when it
    /// runs none.
    pub(crate) fn redial_now(&self) -> bool {
        let redial = self
            .redial
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match redial.as_ref() {
            Some(redial) if !redial.ended() => {
                redial.now();
                true
            }
            _ => false,
        }
    }

    /// Forget the caps of the Lua `owner`, whose alerts ended.
    pub(crate) fn forget_alert_owner(&self, owner: &str) {
        self.alert_caps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .forget_owner(owner);
    }

    /// Whether the alert counted under `cap` may ring at `now`, under the
    /// 10 second cap, and if so, mark that it rang.
    pub(crate) fn allow_alert(&self, cap: &str, now: tokio::time::Instant) -> bool {
        self.alert_caps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .allow(cap, now)
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
        emit_for(app, self.id, event, payload);
    }

    /// Send `event` with `data` and the session's id beside it,
    /// `{session, data}`. A GMCP packet, the prompt values and the
    /// affect fulls go this way, since a `session` field among their own
    /// keys would read as one more package field, value or affect.
    pub(crate) fn emit_data<R: tauri::Runtime, T: Serialize>(
        &self,
        app: &AppHandle<R>,
        event: &str,
        data: &T,
    ) {
        let payload = Data {
            session: self.id,
            data,
        };
        if let Err(e) = app.emit(event, &payload) {
            warn!(error = %e, event, "failed to emit a session event");
        }
    }
}

/// Send `event` with `payload` for the session `session`, as
/// [`Session::emit`] does, from a task that holds only its id.
pub(crate) fn emit_for<R: tauri::Runtime, T: Serialize>(
    app: &AppHandle<R>,
    session: SessionId,
    event: &str,
    payload: &T,
) {
    let named = Named { session, payload };
    if let Err(e) = app.emit(event, &named) {
        warn!(error = %e, event, "failed to emit a session event");
    }
}

/// What a session goes by, the name its row reads: the name you gave it,
/// else its character, else where it dials, `at`. A known world shows a
/// port that is not its own, like `The Forsaken Lands 1825`, and any
/// other host shows its port only while another open session shares the
/// host, `shares_host`. None with no name, character or place, where the
/// row reads New session. The twin of `sessionLabel` in
/// src/lib/sessionLabel.ts, held to it by
/// fixtures/session-labels/cases.json.
pub(crate) fn label_of(
    name: Option<&str>,
    character: Option<&str>,
    at: Option<(&str, u16)>,
    shares_host: bool,
) -> Option<String> {
    let who = [name, character]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|who| !who.is_empty());
    if let Some(who) = who {
        return Some(who.to_string());
    }
    let (host, port) = at?;
    Some(if known_world(host).is_none() && shares_host {
        format!("{} {port}", host.trim())
    } else {
        world_label(host, port)
    })
}

/// Tell every window each session's row, in list order with the
/// selected one marked, after a step that changed what a row shows. It
/// reads the rows, which take each session's connection lock, so call it
/// with no profile or connection held.
pub(crate) fn broadcast_sessions<R: tauri::Runtime>(app: &AppHandle<R>, state: &AppState) {
    // Two steps that broadcast at once could each read the rows and send
    // them in the other order, which would leave every window on the
    // older rows. One broadcast at a time sends the newest rows last.
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    broadcast(app, SESSIONS_CHANGED, &state.session_rows());
}

/// One session as the window lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SessionRow {
    pub(crate) id: SessionId,
    /// The name you gave it.
    pub(crate) name: Option<String>,
    /// The character it plays, or played last, see [`Session::played`].
    pub(crate) character: Option<String>,
    /// Where it dials, None before its first connect or address.
    pub(crate) host: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) tls: bool,
    /// The profile it plays, None only before launch loads one.
    pub(crate) profile: Option<String>,
    pub(crate) connected: bool,
    /// When the live link reached the game, in Unix ms, None while no
    /// link runs.
    pub(crate) since: Option<u64>,
    pub(crate) selected: bool,
}

/// A session event's payload with the session that sent it.
#[derive(Serialize)]
struct Named<'a, T> {
    session: SessionId,
    #[serde(flatten)]
    payload: &'a T,
}

/// A session event's data beside the session that sent it.
#[derive(Serialize)]
struct Data<'a, T> {
    session: SessionId,
    data: &'a T,
}

/// The sessions in the order the window lists them, the one selected,
/// and the profiles they play.
pub(crate) struct Sessions {
    list: Vec<Arc<Session>>,
    /// Always names a session in `list`.
    selected: SessionId,
    /// The number the next session takes. It only grows, so a late event
    /// from a session that closed never names a newer one.
    next: u32,
    /// The profiles the sessions play, each once, in the order they
    /// opened.
    profiles: Vec<Arc<OpenProfile>>,
    /// The place the next profile to open takes in that order.
    next_profile: u64,
    /// The profile Settings holds unsaved edits on, which stays open
    /// when its last session leaves it, until Save or Discard lets go.
    edit_hold: Option<Arc<OpenProfile>>,
}

impl Sessions {
    /// Add a session after the others, with nothing connected, that
    /// plays `profile`, and return it.
    pub(crate) fn open(&mut self, profile: Arc<OpenProfile>) -> Arc<Session> {
        let session = Arc::new(Session::new(SessionId(self.next), profile));
        self.next += 1;
        self.list.push(session.clone());
        session
    }

    /// Move the session `id` to the place `to` in the list, or to the end
    /// when `to` lies past it. The others keep their order, and the
    /// selection stays on the session it names.
    pub(crate) fn move_to(&mut self, id: SessionId, to: usize) -> Result<(), &'static str> {
        let from = self
            .list
            .iter()
            .position(|session| session.id == id)
            .ok_or(NO_SUCH_SESSION)?;
        let session = self.list.remove(from);
        self.list.insert(to.min(self.list.len()), session);
        Ok(())
    }

    /// Take the session `id` out of the list and return it. When it was
    /// selected, the session after it is selected, or the one before it
    /// when it was last, and its grid shows. The list never gives up its
    /// only session.
    pub(crate) fn close(&mut self, id: SessionId) -> Result<Arc<Session>, &'static str> {
        let at = self
            .list
            .iter()
            .position(|session| session.id == id)
            .ok_or(NO_SUCH_SESSION)?;
        if self.list.len() == 1 {
            return Err(ONLY_SESSION);
        }
        let closed = self.list.remove(at);
        if self.selected == id {
            let next = self.list[at.min(self.list.len() - 1)].id;
            self.select(next);
        }
        Ok(closed)
    }

    /// The open profile named `name`, while a session plays it or
    /// Settings holds unsaved edits on it.
    pub(crate) fn profile(&self, name: &str) -> Option<Arc<OpenProfile>> {
        self.profiles
            .iter()
            .find(|open| open.name().as_deref() == Some(name))
            .cloned()
    }

    /// The profiles the sessions play, in the order they opened.
    pub(crate) fn profiles(&self) -> Vec<Arc<OpenProfile>> {
        self.profiles.clone()
    }

    /// Keep `profile`, which `name` names in the profile set, open, for a
    /// session to play.
    pub(crate) fn add_profile(&mut self, name: &str, profile: Profile) -> Arc<OpenProfile> {
        let open = Arc::new(OpenProfile::new(
            self.next_profile,
            Some(name.to_string()),
            profile,
        ));
        self.next_profile += 1;
        self.profiles.push(open.clone());
        open
    }

    /// How many sessions play `open`.
    pub(crate) fn players(&self, open: &Arc<OpenProfile>) -> usize {
        self.list
            .iter()
            .filter(|session| Arc::ptr_eq(&session.profile(), open))
            .count()
    }

    /// Close `open` when no session plays it and Settings holds no
    /// unsaved edits on it. Returns whether it closed, which a profile a
    /// restored session waits on never does, since it never opened.
    pub(crate) fn close_unplayed(&mut self, open: &Arc<OpenProfile>) -> bool {
        if self.players(open) > 0 || !self.is_open(open) || self.holds_edits(open) {
            return false;
        }
        self.profiles.retain(|kept| !Arc::ptr_eq(kept, open));
        true
    }

    /// Whether Settings holds unsaved edits on `open`.
    pub(crate) fn holds_edits(&self, open: &Arc<OpenProfile>) -> bool {
        self.edit_hold
            .as_ref()
            .is_some_and(|held| Arc::ptr_eq(held, open))
    }

    /// The profile Settings holds unsaved edits on, see
    /// [`Sessions::hold_edits`].
    pub(crate) fn edit_hold(&self) -> Option<Arc<OpenProfile>> {
        self.edit_hold.clone()
    }

    /// Hold the open profile named `name` for the unsaved edits Settings
    /// keeps on it, or let go with None. A name no open profile has holds
    /// nothing. Returns the profile let go when no session plays it, which
    /// closes here.
    pub(crate) fn hold_edits(&mut self, name: Option<&str>) -> Option<Arc<OpenProfile>> {
        let next = name.and_then(|name| self.profile(name));
        let left = std::mem::replace(&mut self.edit_hold, next)?;
        self.close_unplayed(&left).then_some(left)
    }

    /// Whether `open` is one of the profiles the sessions play, rather
    /// than one a restored session waits on, see [`Sessions::restore`].
    pub(crate) fn is_open(&self, open: &Arc<OpenProfile>) -> bool {
        self.profiles.iter().any(|kept| Arc::ptr_eq(kept, open))
    }

    /// The profiles restored sessions wait on under the name `name`.
    pub(crate) fn waiting_on(&self, name: &str) -> Vec<Arc<OpenProfile>> {
        self.list
            .iter()
            .map(|session| session.profile())
            .filter(|open| !self.is_open(open) && open.name().as_deref() == Some(name))
            .collect()
    }

    /// Put the sessions `entries` lists, in order and none connected, in
    /// place of the one the app starts with, and select the one `selected`
    /// names, or else the first. The selected session plays the profile
    /// the app starts on, which launch then loads. Each other one waits
    /// under the name of the profile it last played, see
    /// [`OpenProfile::waiting`]. An entry whose id an earlier one took is
    /// left out.
    pub(crate) fn restore(&mut self, entries: &[SessionEntry], selected: Option<SessionId>) {
        let Some(first) = entries.first() else {
            return;
        };
        let selected = selected
            .filter(|id| entries.iter().any(|entry| entry.id == *id))
            .unwrap_or(first.id);
        let starting = self.selected().profile();
        let mut list: Vec<Arc<Session>> = Vec::new();
        for entry in entries {
            if list.iter().any(|session| session.id == entry.id) {
                continue;
            }
            let profile = if entry.id == selected {
                starting.clone()
            } else {
                let id = self.next_profile;
                self.next_profile += 1;
                Arc::new(OpenProfile::waiting(id, &entry.profile))
            };
            let session = Session::new(entry.id, profile);
            session.rename(entry.name.as_deref());
            *session
                .address
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = entry
                .host
                .clone()
                .zip(entry.port)
                .map(|(host, port)| Address {
                    host,
                    port,
                    tls: entry.tls,
                });
            list.push(Arc::new(session));
        }
        self.next = list
            .iter()
            .map(|session| session.id.0.saturating_add(1))
            .max()
            .unwrap_or(self.next);
        self.list = list;
        self.selected = selected;
        #[cfg(any(native_surface, test))]
        crate::native::grid::show(selected);
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

    /// Every session but `id`, in order.
    pub(crate) fn others(&self, id: SessionId) -> Vec<Arc<Session>> {
        self.list
            .iter()
            .filter(|session| session.id != id)
            .cloned()
            .collect()
    }

    /// Every session in order, with the id of the selected one.
    pub(crate) fn in_order(&self) -> (Vec<Arc<Session>>, SessionId) {
        (self.list.clone(), self.selected)
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
    /// One session, selected, on the defaults until launch loads a
    /// profile.
    fn default() -> Self {
        let defaults = Arc::new(OpenProfile::new(0, None, Profile::default()));
        Self {
            list: vec![Arc::new(Session::new(SessionId::FIRST, defaults.clone()))],
            selected: SessionId::FIRST,
            next: 2,
            profiles: vec![defaults],
            next_profile: 1,
            edit_hold: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::{json, Value};
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tauri::{App, Listener};

    use super::{label_of, Session, SessionId, SessionRow, NO_SUCH_SESSION};
    use crate::app::events;
    use crate::app::state::AppState;
    use crate::profile::live::Profile;
    use crate::profile::worlds::host_key;
    use crate::session::{StatePayload, TargetPayload};

    /// A session `id` that plays the defaults.
    fn on_defaults(id: SessionId) -> Session {
        let defaults = crate::profile::open::OpenProfile::new(0, None, Profile::default());
        Session::new(id, Arc::new(defaults))
    }

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
        let session = on_defaults(SessionId(7));
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
    fn emit_data_names_the_session_beside_a_map_or_null_it_leaves_whole() {
        let app = app();
        let vars = hear(&app, events::PROMPT_VARS);
        let session = on_defaults(SessionId(7));
        let map = json!({"hp": "800", "session": "?"});
        session.emit_data(app.handle(), events::PROMPT_VARS, &map);
        session.emit_data(app.handle(), events::PROMPT_VARS, &Value::Null);
        assert_eq!(
            *vars.lock().unwrap(),
            [
                json!({"session": 7, "data": {"hp": "800", "session": "?"}}),
                json!({"session": 7, "data": null}),
            ]
        );
    }

    #[test]
    fn an_echo_in_one_session_leaves_the_other_sessions_count_alone() {
        // The echo reaches the session's grid too, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let app = app();
        let outputs = hear(&app, events::OUTPUT);
        let (one, two) = (on_defaults(SessionId(1)), on_defaults(SessionId(2)));
        crate::output::echo_lines(app.handle(), &two, &["You wave.".to_string()]);
        assert_eq!((one.output_count(), two.output_count()), (0, 1));
        assert_eq!(outputs.lock().unwrap()[0]["session"], 2);
    }

    #[test]
    fn a_label_names_the_session_or_its_character_or_else_the_world_with_its_port() {
        let session = on_defaults(SessionId(2));
        assert_eq!(session.label(&[]), None);
        let at = |host: &str, port| {
            Some(super::Address {
                host: host.into(),
                port,
                tls: false,
            })
        };
        *session.address.lock().unwrap() = at("play.theforsakenlands.com", 1825);
        assert_eq!(
            session.label(&[]).as_deref(),
            Some("The Forsaken Lands 1825")
        );
        *session.address.lock().unwrap() = at("play.theforsakenlands.com", 1848);
        assert_eq!(session.label(&[]).as_deref(), Some("The Forsaken Lands"));
        // Another host shows its port while another session shares it.
        *session.address.lock().unwrap() = at("mud.example.org", 4000);
        let other = Arc::new(on_defaults(SessionId(3)));
        assert_eq!(
            session.label(std::slice::from_ref(&other)).as_deref(),
            Some("mud.example.org")
        );
        *other.address.lock().unwrap() = at("MUD.example.org.", 4001);
        assert_eq!(
            session.label(&[other]).as_deref(),
            Some("mud.example.org 4000")
        );
        *session.current_character.lock().unwrap() = Some("Builder".into());
        assert_eq!(session.label(&[]).as_deref(), Some("Builder"));
        session.rename(Some("  Build port  "));
        assert_eq!(session.label(&[]).as_deref(), Some("Build port"));
        // A blank name clears it, and the character shows again.
        session.rename(Some(" "));
        assert_eq!(session.name(), None);
        assert_eq!(session.label(&[]).as_deref(), Some("Builder"));
    }

    /// fixtures/session-labels/cases.json, which `sessionLabel` in
    /// src/lib/sessionLabel.ts runs too.
    #[derive(serde::Deserialize)]
    struct LabelCases {
        cases: Vec<LabelCase>,
    }

    #[derive(serde::Deserialize)]
    struct LabelCase {
        name: String,
        session: LabelRow,
        others: Vec<Place>,
        label: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct LabelRow {
        name: Option<String>,
        character: Option<String>,
        host: Option<String>,
        port: Option<u16>,
    }

    #[derive(serde::Deserialize)]
    struct Place {
        host: Option<String>,
    }

    #[test]
    fn a_label_reads_as_the_page_names_the_session() {
        let text = include_str!("../../fixtures/session-labels/cases.json");
        let cases: LabelCases = serde_json::from_str(text).expect("the label cases");
        assert_ne!(cases.cases.len(), 0);
        for case in cases.cases {
            let row = &case.session;
            let at = row.host.as_deref().zip(row.port);
            let shares_host = row.host.as_deref().is_some_and(|own| {
                case.others
                    .iter()
                    .filter_map(|other| other.host.as_deref())
                    .any(|other| host_key(other) == host_key(own))
            });
            let label = label_of(
                row.name.as_deref(),
                row.character.as_deref(),
                at,
                shares_host,
            );
            assert_eq!(label, case.label, "{}", case.name);
        }
    }

    #[test]
    fn a_row_names_the_character_it_played_until_a_connect_you_start() {
        let session = on_defaults(SessionId(2));
        *session.current_character.lock().unwrap() = Some("Tolliver".into());
        session.note_played("Tolliver");
        assert_eq!(session.row(false).character.as_deref(), Some("Tolliver"));
        // A drop keeps the live character, and a try of a redial or a
        // disconnect forgets it, where the row keeps it.
        *session.current_character.lock().unwrap() = None;
        assert_eq!(session.character(), None);
        assert_eq!(session.row(false).character.as_deref(), Some("Tolliver"));
        assert_eq!(session.label(&[]).as_deref(), Some("Tolliver"));
        // A connect you start names the world until you log in.
        session.forget_played();
        assert_eq!(session.row(false).character, None);
        // The live character wins over the one played before.
        session.note_played("Tolliver");
        *session.current_character.lock().unwrap() = Some("Orla".into());
        assert_eq!(session.played().as_deref(), Some("Orla"));
    }

    #[test]
    fn the_rows_list_the_sessions_in_order_with_where_each_last_connected() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let state = AppState::default();
        let one = state.selected_session();
        let two = state.open_session(one.profile());
        one.rename(Some("Main"));
        *two.address.lock().unwrap() = Some(super::Address {
            host: "play.theforsakenlands.com".into(),
            port: 1825,
            tls: true,
        });
        *two.current_character.lock().unwrap() = Some("Builder".into());
        assert_eq!(state.select_session(two.id), Ok(()));
        let row = |id, name: Option<&str>, character: Option<&str>, port: Option<u16>, selected| {
            SessionRow {
                id,
                name: name.map(str::to_string),
                character: character.map(str::to_string),
                host: port.map(|_| "play.theforsakenlands.com".to_string()),
                port,
                tls: port.is_some(),
                profile: None,
                connected: false,
                since: None,
                selected,
            }
        };
        assert_eq!(
            state.session_rows(),
            [
                row(one.id, Some("Main"), None, None, false),
                row(two.id, None, Some("Builder"), Some(1825), true),
            ]
        );
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
        let one = state.selected_session();
        let two = state.open_session(one.profile());
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
        let defaults = state.selected_session().profile();
        let two = state.open_session(defaults.clone()).id;
        let three = state.open_session(defaults).id;
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

    #[test]
    fn a_move_keeps_the_others_in_order_and_the_selection_where_it_was() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let state = AppState::default();
        let one = state.selected_session().id;
        let profile = state.selected_session().profile();
        let two = state.open_session(profile.clone()).id;
        let three = state.open_session(profile).id;
        assert_eq!(state.select_session(two), Ok(()));
        let order = || -> Vec<(SessionId, bool)> {
            let rows = state.session_rows();
            rows.iter().map(|row| (row.id, row.selected)).collect()
        };
        assert_eq!(state.move_session(three, 0), Ok(()));
        assert_eq!(order(), [(three, false), (one, false), (two, true)]);
        // A place past the end moves it last.
        assert_eq!(state.move_session(three, 9), Ok(()));
        assert_eq!(order(), [(one, false), (two, true), (three, false)]);
        assert_eq!(state.move_session(two, 0), Ok(()));
        assert_eq!(order(), [(two, true), (one, false), (three, false)]);
        assert_eq!(
            state.move_session(SessionId(9), 0),
            Err(NO_SUCH_SESSION.to_string())
        );
        assert_eq!(crate::native::grid::shown(), two);
    }

    #[test]
    fn closing_a_session_hands_the_selection_on_and_never_closes_the_only_one() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let state = AppState::default();
        let one = state.selected_session();
        let (two, three) = (
            state.open_session(one.profile()).id,
            state.open_session(one.profile()).id,
        );
        assert_eq!(state.select_session(two), Ok(()));
        assert_eq!(state.close_session(two).map(|s| s.id), Ok(two));
        assert_eq!(state.selected_session().id, three);
        assert_eq!(crate::native::grid::shown(), three);
        assert_eq!(state.close_session(three).map(|s| s.id), Ok(three));
        assert_eq!(state.selected_session().id, one.id);
        assert_eq!(
            state.close_session(one.id).map(|s| s.id),
            Err(super::ONLY_SESSION.to_string())
        );
        assert_eq!(
            state.close_session(two).map(|s| s.id),
            Err(NO_SUCH_SESSION.to_string())
        );
    }
}
