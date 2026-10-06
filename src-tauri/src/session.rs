//! One task per connection. [`connect`] readies the app for a new
//! connection and calls [`spawn`], which connects and starts the loop.
//! The rest of the app reaches the task through its [`SessionHandle`],
//! and [`disconnect`] ends it. The payloads of the events the session
//! emits sit here too. Each file under `session/` does one job.
//!
//! - `conn` is the loop and the `Conn` that holds what it owns for a
//!   connection. It sends your lines, takes each socket read, repaints
//!   your prompt when a deadline passes, polls the tick and the timers,
//!   and ends the connection.
//! - `connection` holds the [`connection::Connection`], all the session
//!   state the line pipeline changes, under one lock: your target, the
//!   room list and look, the tick's count, the prompt engine, the session
//!   variables and the Lua engine.
//! - `socket` opens the plain or TLS socket.
//! - `read` is the socket read path, from each telnet event to what the
//!   end of a read sends.
//! - `lines` cuts what the game sends into lines and the partial after
//!   them.
//! - `steps` holds what the loop does under the connection lock, and
//!   the profile lock before it where a step needs both, for each line,
//!   prompt, partial and repaint.
//! - `batch` holds what one read writes, and the frame and log rows a
//!   burst of reads owes.
//! - `prompt_view` holds what your prompt shows and what the webview
//!   hears of it.
//! - `gmcp` handles each GMCP packet, and `gmcp_vars` binds the common
//!   packages to session variables.
//! - `effects` applies what Lua asks for and runs the lines a timer, the
//!   tick or `mud.input` fires, and `lua_timers` fires the Lua timers.
//! - `echo` keeps the text of a line you type while the server hides
//!   your input out of the log.
//! - `identity` says who is logged in, and sends it to every window.
//! - `last_packages` keeps the last affects, vitals and combat packets
//!   for a window that opens between them.
//! - `room_block` finds the lines of a look that list what the room
//!   holds, and `highlight_ground` keeps the ground trigger colors must
//!   read on.
//! - `log_sink` holds the session log's row and the scrollback ring of a
//!   connection, and the lines the session captures as it ends.
//! - `perf` counts the work on the hot path.
//! - `reconnect` decides whether a drop dials again, and runs the series
//!   of redials.
//! - `walk` is the walker, which sends the steps of a `#walk` one at a
//!   time.
//! - `tests` drives the steps the way the loop does.

mod batch;
mod conn;
pub(crate) mod connection;
pub(crate) mod echo;
pub(crate) mod effects;
mod gmcp;
mod gmcp_vars;
pub(crate) mod highlight_ground;
pub(crate) mod identity;
pub(crate) mod last_packages;
mod lines;
mod log_sink;
mod lua_timers;
mod perf;
pub(crate) mod prompt_view;
mod read;
pub(crate) mod reconnect;
pub(crate) mod room_block;
mod socket;
mod steps;
pub(crate) mod walk;

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use vosh_protocol::telnet::{option as telnet_option, Negotiator};

use crate::app::events;
use crate::app::state::SharedState;
use crate::disk::save::PERSIST_LOCK;
use crate::input::walk::WalkCommand;
use crate::sessions::{Address, Session};

use conn::io_loop;
use log_sink::LogSink;
use socket::ConnectionError;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StatePayload {
    Connecting { host: String, port: u16, tls: bool },
    Connected { host: String, port: u16, tls: bool },
    Disconnected { reason: Option<String> },
}

/// Emitted on `session://target` whenever the active user target
/// changes (set, cycled, cleared, or wiped by disconnect). The
/// frontend uses this to mark the targeted char in the room info
/// chips and to drive the `TargetBar` when no Char.Combat is active.
///
/// `room_idx` is the 1-based position in the latest `Room.Chars`
/// push that matches the user's target string (substring,
/// case-insensitive). It exists so the frontend doesn't have to
/// re-implement the matching logic for the chip `>` marker —
/// short keywords like "gris" won't equality-match
/// "The Baron Grisvald" but the backend already resolved the
/// pointer via substring.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct TargetPayload {
    pub name: Option<String>,
    pub room_idx: Option<usize>,
    /// Snapshot of the current quick-key bindings (name + verb).
    /// Frontend renders them next to the target name on the
    /// `TargetBar` so the user always sees which slots are armed.
    pub quick_keys: Vec<connection::QuickKey>,
}

impl TargetPayload {
    /// The target and quick keys `c` holds now.
    pub(crate) fn of(c: &connection::Connection) -> Self {
        Self {
            name: c.target.name.clone(),
            room_idx: c.target.room_idx,
            quick_keys: c.target.quick_keys.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RoutedPayload {
    pub pane: String,
    pub text: String,
}

/// Sent when the server's WILL/WONT ECHO negotiation flips. ROM derivatives
/// use this to ask for passwords: WILL ECHO means "the server is taking
/// over echoing, so don't show what the user types"; WONT ECHO means
/// "back to normal local echo." The frontend masks the input row when
/// password=true.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct InputModePayload {
    pub password: bool,
}

/// Bytes flowing to the server. The frontend echoes typed commands into the
/// terminal pane synchronously, so the `io_loop` only writes to the wire.
pub(crate) enum OutgoingMsg {
    /// Bytes for the wire. `masked` is true for a line typed into the
    /// masked password field, which the session log hides even when the
    /// server has already handed echo back.
    Send { bytes: Vec<u8>, masked: bool },
    /// Update the advertised terminal size and, if NAWS has already
    /// been negotiated, push a fresh NAWS subnegotiation to the wire
    /// so the server re-wraps its output at the new width.
    WindowSize { cols: u16, rows: u16 },
    /// Repaint the open row as the profile's `[prompt]` table now says,
    /// as a change to the switch or the design asks. Only the session
    /// task writes session output, so a command sends this instead.
    PromptRepaint,
    /// The webview wrote to the terminal itself, such as your typed echo
    /// or an error notice, so the open row is no longer the last thing on
    /// screen. `after` is the newest output of the prompt stage that the
    /// renderer that shows took before the text (`Output::id`), since
    /// the session can hear of the text after it wrote more.
    LocalWrite { after: u64 },
    /// A `#walk` you typed, or Esc, for the walker. It follows the bytes
    /// of its line.
    Walk(WalkCommand),
}

pub(crate) struct SessionHandle {
    tx_outgoing: mpsc::UnboundedSender<OutgoingMsg>,
    task: JoinHandle<()>,
}

impl SessionHandle {
    /// Send raw bytes to the connection. Returns false when the session has
    /// already been torn down.
    pub(crate) fn send(&self, bytes: Vec<u8>) -> bool {
        self.tx_outgoing
            .send(OutgoingMsg::Send {
                bytes,
                masked: false,
            })
            .is_ok()
    }

    /// Send a line typed into the masked password field. The session
    /// log keeps `> (hidden)` for it whatever the echo state is when it
    /// leaves. Returns false when the session has already been torn down.
    pub(crate) fn send_masked(&self, bytes: Vec<u8>) -> bool {
        self.tx_outgoing
            .send(OutgoingMsg::Send {
                bytes,
                masked: true,
            })
            .is_ok()
    }

    /// Push a terminal resize event into the session task so it can
    /// update the negotiator and emit a NAWS subnegotiation.
    pub(crate) fn set_window_size(&self, cols: u16, rows: u16) -> bool {
        self.tx_outgoing
            .send(OutgoingMsg::WindowSize { cols, rows })
            .is_ok()
    }

    /// Tell the session the webview wrote to the terminal itself, on a
    /// renderer whose newest output of the prompt stage was `after`. That
    /// closes the open row when the row came no later. Returns false when
    /// the session has already been torn down.
    pub(crate) fn local_write(&self, after: u64) -> bool {
        self.tx_outgoing
            .send(OutgoingMsg::LocalWrite { after })
            .is_ok()
    }

    /// Repaint the open row as the `[prompt]` table now says. Returns
    /// false when the session has already been torn down.
    pub(crate) fn prompt_repaint(&self) -> bool {
        self.tx_outgoing.send(OutgoingMsg::PromptRepaint).is_ok()
    }

    /// Hand the walker a `#walk` you typed, or Esc. Returns false when
    /// the session has already been torn down.
    pub(crate) fn walk(&self, command: WalkCommand) -> bool {
        self.tx_outgoing.send(OutgoingMsg::Walk(command)).is_ok()
    }

    /// True once the session loop has ended, so nothing sent reaches the
    /// game. The loop says it disconnected just before it ends, so a test
    /// waits on this to know the session is gone. Such a test also
    /// accepts an empty slot, in case the ended session cleared it.
    #[cfg(test)]
    pub(crate) fn has_ended(&self) -> bool {
        self.tx_outgoing.is_closed()
    }

    pub(crate) async fn shutdown(self) {
        drop(self.tx_outgoing);
        let _ = self.task.await;
    }
}

/// Connect `session` to `host` on `port`, over TLS when `tls` says so,
/// in place of the connection it runs, if any, and in place of a series
/// of redials it waits on. The new connection starts without the session
/// variables, the character and the affects of the last one, and every
/// window hears who the session is for once it connects or fails to. The
/// row forgets the character the session played, so it names the world
/// until you log in, where each try of a redial keeps it.
pub(crate) async fn connect<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    host: String,
    port: u16,
    tls: bool,
) -> Result<(), String> {
    // The connection that runs ends first, so a drop it ends on cannot
    // start a series after the cancel below.
    let old = session.slot.lock().await.take();
    if let Some(handle) = old {
        handle.shutdown().await;
    }
    reconnect::cancel(app, session).await;
    session.forget_played();
    dial(app, state, session, host, port, tls, false).await
}

/// The body of [`connect`], which a redial runs too, since it must not
/// end the series it runs in. A redial that fails says nothing of it to
/// the page, `quiet`, since the series prints a line for each try.
pub(crate) async fn dial<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    host: String,
    port: u16,
    tls: bool,
    quiet: bool,
) -> Result<(), String> {
    // Take any existing handle out under a brief lock and drop the lock
    // before doing the long-running connect. This lets `disconnect`
    // run concurrently to cancel a hung connect attempt.
    let old = {
        let mut current = session.slot.lock().await;
        current.take()
    };
    if let Some(handle) = old {
        handle.shutdown().await;
    }

    // This session's variables clear on reconnect, and the profile's,
    // which other sessions read too, survive.
    session.connection.lock().vars.clear();

    // Remember the live connection target so the Char.Status-driven
    // auto-switch path can re-resolve against it once the MUD tells us
    // who we logged in as. Cleared in `disconnect`. Reset the
    // last-known character at the same time so a reconnect to a
    // different account triggers a fresh resolve.
    if let Ok(mut g) = session.current_connection.lock() {
        *g = Some((host.clone(), port));
    }
    if let Ok(mut g) = session.current_character.lock() {
        *g = None;
    }
    let address = Address {
        host: host.clone(),
        port,
        tls,
    };
    let moved = session
        .address
        .lock()
        .is_ok_and(|mut g| g.replace(address.clone()) != Some(address));
    if moved {
        // A launch restores the session where it last connected.
        let _persist_guard = PERSIST_LOCK.lock().await;
        crate::profile::set::save_sessions(state).await;
    }
    // The old connection cleared the packets as it ended. A new
    // connection starts with none until the MUD sends its own.
    session.last_packages.clear();
    crate::affects::full::connect(app, session);

    let scrollback_path = state
        .app_data
        .get()
        .map(|dir| crate::disk::paths::scrollback_path(dir, session.id));

    // Seed the negotiator with the most recently reported terminal
    // size so the initial `DO NAWS` reply during the handshake
    // carries the correct cols/rows. The default of (80, 24) is
    // applied only when the frontend never called
    // `session_set_window_size` before this connect.
    let initial_size = session.window_size.lock().map_or((80, 24), |g| *g);
    let target = (host.clone(), port);
    let known_host = crate::profile::worlds::is_forsaken_lands(&host);

    let spawned = spawn(
        app.clone(),
        state,
        session,
        host,
        port,
        tls,
        known_host,
        scrollback_path,
        initial_size,
    )
    .await;
    let handle = match spawned {
        Ok(handle) => handle,
        Err(e) => {
            // Surface the disconnected state so the UI does not stay stuck
            // on "connecting...". The frontend listens for session://state.
            emit_state(
                app,
                session,
                StatePayload::Disconnected {
                    reason: (!quiet).then(|| e.to_string()),
                },
            );
            // Nothing reached the target, so nobody is logged in there.
            // A connect that raced this one keeps its own target.
            if let Ok(mut g) = session.current_connection.lock() {
                if g.as_ref() == Some(&target) {
                    *g = None;
                }
            }
            crate::session::identity::broadcast_session_identity(app, state, session).await;
            return Err(e.to_string());
        }
    };
    #[cfg(test)]
    if quiet {
        reconnect::hold_try(state);
    }

    {
        let mut current = session.slot.lock().await;
        // `session_close` takes the session out of the map before its
        // disconnect takes the slot. A close that came while this connect
        // ran found the slot empty, so the connection ends here, or it
        // would run on with nothing to reach it.
        if state.session(Some(session.id)).is_err() {
            handle.shutdown().await;
            return Err(crate::sessions::NO_SUCH_SESSION.to_string());
        }
        if let Some(prev) = current.take() {
            // A concurrent connect raced us. Shut down our old handle.
            prev.shutdown().await;
        }
        *current = Some(handle);
    }
    crate::session::identity::broadcast_session_identity(app, state, session).await;
    Ok(())
}

/// End the connection `session` runs, if any, and the series of redials
/// it waits on, and forget the connection and its character. Every
/// window hears that no connection is live, and the rows, where the
/// session's row still names the character it played.
pub(crate) async fn disconnect<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
) {
    // The series ends first, so a try that connects as you disconnect
    // cannot put its link in the slot after the take below.
    reconnect::cancel(app, session).await;
    {
        let mut current = session.slot.lock().await;
        if let Some(handle) = current.take() {
            handle.shutdown().await;
        }
    }
    // A link that dropped as it ended may have started a series.
    reconnect::cancel(app, session).await;
    if let Ok(mut g) = session.current_connection.lock() {
        *g = None;
    }
    if let Ok(mut g) = session.current_character.lock() {
        *g = None;
    }
    crate::session::identity::broadcast_session_identity(app, state, session).await;
    crate::sessions::broadcast_sessions(app, state);
}

/// Open a connection, install a parser plus negotiator, and spin up the IO
/// loop. The returned handle owns the outgoing channel; drop it to close.
///
/// `initial_window_size` is the (cols, rows) the negotiator should
/// carry into the first NAWS subnegotiation. The caller (typically
/// [`connect`]) reads this from `Session::window_size` so the
/// server's first wrap-width decision is based on the actual
/// terminal geometry instead of the negotiator's 80×24 fallback.
///
/// `known_host` is whether the host is The Forsaken Lands, whose rules
/// the custom prompt follows. The caller says so, which lets a test have
/// a fake game on a local port count as it.
///
/// The loop shares the log store in `state`, and the profile `session`
/// plays, its connection's target and room list, its Lua timers and its
/// scrollback ring, with the rest of the app.
pub(crate) async fn spawn<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    host: String,
    port: u16,
    tls: bool,
    known_host: bool,
    scrollback_path: Option<std::path::PathBuf>,
    initial_window_size: (u16, u16),
) -> Result<SessionHandle, ConnectionError> {
    emit_state(
        &app,
        session,
        StatePayload::Connecting {
            host: host.clone(),
            port,
            tls,
        },
    );

    let mut stream = socket::connect(&host, port, tls).await?;
    info!(%host, port, tls, "session connected");

    // Seed the negotiator with the size we already know about so the
    // first `DO NAWS` from the server gets a correct subneg. Otherwise
    // the 80×24 default carries through until you nudge the window,
    // and `who` output wraps mid-sentence.
    let mut negotiator = Negotiator::new();
    negotiator.set_window_size(initial_window_size.0, initial_window_size.1);
    // Proactively ask for end-of-record so the server marks each prompt.
    // Without this, ROM derivatives that gate EOR on negotiation never
    // send the byte, and we have to merge the prompt with the next room
    // line. Other negotiations stay reactive in handle_event. The
    // negotiator keeps the ask, so the WILL EOR that answers it gets no
    // answer back (RFC 1143).
    let ask = negotiator.ask(telnet_option::EOR);
    if let Err(e) = stream.write_all(&ask).await {
        warn!(error = %e, "failed to send initial DO EOR");
    }
    let _ = stream.flush().await;

    // The row counts the time online from here, which a redial that
    // reaches the game starts again.
    let since = u64::try_from(now_ms()).unwrap_or_default();
    session.set_since(Some(since));
    emit_state(
        &app,
        session,
        StatePayload::Connected {
            host: host.clone(),
            port,
            tls,
        },
    );
    // What the plugins printed at launch, if nothing showed it yet.
    crate::app::plugins::show_launch_lines(&app, session);

    let log_sink = LogSink::open(
        state.logs.clone(),
        session.scrollback.clone(),
        scrollback_path,
        &host,
        port,
    )
    .await;

    let (tx_outgoing, rx_outgoing) = mpsc::unbounded_channel::<OutgoingMsg>();
    let task = tokio::spawn(io_loop(
        app,
        stream,
        rx_outgoing,
        Arc::clone(session),
        log_sink,
        negotiator,
        known_host,
    ));

    Ok(SessionHandle { tx_outgoing, task })
}

/// The wall clock, in milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// `session://prompt-gag-without-reader`: a trigger hid your prompt and
/// set prompt values while this profile reads no prompt, so Vosh drew
/// nothing in its place.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct GagWithoutReaderPayload {
    pub trigger: String,
}

/// Tell the page where the connection of `session` stands, and every
/// window the rows, since a connect that starts or ends changes what the
/// session's row shows. Call it with no profile or connection held.
fn emit_state<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, payload: StatePayload) {
    session.emit(app, events::STATE, &payload);
    if let Some(state) = app.try_state::<SharedState>() {
        crate::sessions::broadcast_sessions(app, &state);
    }
}

fn emit_input_mode<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, password: bool) {
    session.emit(app, events::INPUT_MODE, &InputModePayload { password });
}

#[cfg(test)]
mod tests;
