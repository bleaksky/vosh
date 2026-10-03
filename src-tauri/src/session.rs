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
//! - `connection` opens the plain or TLS socket.
//! - `read` is the socket read path, from each telnet event to what the
//!   end of a read sends.
//! - `lines` cuts what the game sends into lines and the partial after
//!   them.
//! - `steps` holds what the loop does under the profile lock for each
//!   line, prompt, partial and repaint.
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
//! - `room_block` finds the lines of a look that list what the room
//!   holds, and `highlight_ground` keeps the ground trigger colors must
//!   read on.
//! - `log_sink` holds the session log's row and the scrollback ring of a
//!   connection, and the lines the session captures as it ends.
//! - `perf` counts the work on the hot path.
//! - `tests` drives the steps the way the loop does.

mod batch;
mod conn;
mod connection;
pub(crate) mod echo;
pub(crate) mod effects;
mod gmcp;
mod gmcp_vars;
pub(crate) mod highlight_ground;
mod lines;
mod log_sink;
mod lua_timers;
mod perf;
pub(crate) mod prompt_view;
mod read;
pub(crate) mod room_block;
mod steps;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use vosh_protocol::telnet::{option as telnet_option, Negotiator};

use crate::app::events;
use crate::app::state::SharedState;

use conn::io_loop;
use connection::ConnectionError;
use log_sink::LogSink;

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
    pub quick_keys: Vec<crate::profile::QuickKey>,
}

impl TargetPayload {
    /// The target and quick keys `p` holds now.
    pub(crate) fn of(p: &crate::profile::Profile) -> Self {
        Self {
            name: p.target.name.clone(),
            room_idx: p.target.room_idx,
            quick_keys: p.target.quick_keys.clone(),
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

/// Connect to `host` on `port`, over TLS when `tls` says so, in place of
/// the session that runs, if any. The new connection starts without the
/// session variables, the character and the affects of the last one, and
/// every window hears who the session is for once it connects or fails
/// to.
pub(crate) async fn connect<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    host: String,
    port: u16,
    tls: bool,
) -> Result<(), String> {
    // Take any existing handle out under a brief lock and drop the lock
    // before doing the long-running connect. This lets `disconnect`
    // run concurrently to cancel a hung connect attempt.
    let old = {
        let mut current = state.session.lock().await;
        current.take()
    };
    if let Some(handle) = old {
        handle.shutdown().await;
    }

    // Clear session-scoped variables on reconnect; profile-scoped survive.
    state.profile.lock().await.vars.clear_session();

    // Remember the live connection target so the Char.Status-driven
    // auto-switch path can re-resolve against it once the MUD tells us
    // who we logged in as. Cleared in `disconnect`. Reset the
    // last-known character at the same time so a reconnect to a
    // different account triggers a fresh resolve.
    if let Ok(mut g) = state.current_connection.lock() {
        *g = Some((host.clone(), port));
    }
    if let Ok(mut g) = state.current_character.lock() {
        *g = None;
    }
    // The old session cleared the list as it ended. A new connection
    // starts with none until the MUD sends its own.
    state.last_affects.clear();
    crate::affect_full::connect(app, state);

    let scrollback_path = tauri::Manager::path(app)
        .app_data_dir()
        .ok()
        .map(|dir| crate::logs::scrollback_path(&dir));

    // Seed the negotiator with the most recently reported terminal
    // size so the initial `DO NAWS` reply during the handshake
    // carries the correct cols/rows. The default of (80, 24) is
    // applied only when the frontend never called
    // `session_set_window_size` before this connect.
    let initial_size = state.window_size.lock().map_or((80, 24), |g| *g);
    let target = (host.clone(), port);
    let known_host = crate::profile_set::is_forsaken_lands(&host);

    let spawned = spawn(
        app.clone(),
        state,
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
            let _ = app.emit(
                events::STATE,
                StatePayload::Disconnected {
                    reason: Some(e.to_string()),
                },
            );
            // Nothing reached the target, so nobody is logged in there.
            // A connect that raced this one keeps its own target.
            if let Ok(mut g) = state.current_connection.lock() {
                if g.as_ref() == Some(&target) {
                    *g = None;
                }
            }
            crate::characters::broadcast_session_identity(app, state).await;
            return Err(e.to_string());
        }
    };

    {
        let mut current = state.session.lock().await;
        if let Some(prev) = current.take() {
            // A concurrent connect raced us. Shut down our old handle.
            prev.shutdown().await;
        }
        *current = Some(handle);
    }
    crate::characters::broadcast_session_identity(app, state).await;
    Ok(())
}

/// End the session that runs, if any, and forget the live connection
/// and its character. Every window hears that no session is live.
pub(crate) async fn disconnect<R: tauri::Runtime>(app: &AppHandle<R>, state: &SharedState) {
    {
        let mut current = state.session.lock().await;
        if let Some(handle) = current.take() {
            handle.shutdown().await;
        }
    }
    if let Ok(mut g) = state.current_connection.lock() {
        *g = None;
    }
    if let Ok(mut g) = state.current_character.lock() {
        *g = None;
    }
    crate::characters::broadcast_session_identity(app, state).await;
}

/// Open a connection, install a parser plus negotiator, and spin up the IO
/// loop. The returned handle owns the outgoing channel; drop it to close.
///
/// `initial_window_size` is the (cols, rows) the negotiator should
/// carry into the first NAWS subnegotiation. The caller (typically
/// [`connect`]) reads this from `AppState.window_size` so the
/// server's first wrap-width decision is based on the actual
/// terminal geometry instead of the negotiator's 80×24 fallback.
///
/// `known_host` is whether the host is The Forsaken Lands, whose rules
/// the custom prompt follows. The caller says so, which lets a test have
/// a fake game on a local port count as it.
///
/// The session shares the live profile, the Lua timers, the log store and
/// the scrollback ring in `state` with the rest of the app.
pub(crate) async fn spawn<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: &SharedState,
    host: String,
    port: u16,
    tls: bool,
    known_host: bool,
    scrollback_path: Option<std::path::PathBuf>,
    initial_window_size: (u16, u16),
) -> Result<SessionHandle, ConnectionError> {
    emit_state(
        &app,
        StatePayload::Connecting {
            host: host.clone(),
            port,
            tls,
        },
    );

    let mut stream = connection::connect(&host, port, tls).await?;
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

    emit_state(
        &app,
        StatePayload::Connected {
            host: host.clone(),
            port,
            tls,
        },
    );

    let log_sink = LogSink::open(
        state.logs.clone(),
        state.scrollback.clone(),
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
        state.profile.clone(),
        state.script_timers.clone(),
        log_sink,
        negotiator,
        known_host,
    ));

    Ok(SessionHandle { tx_outgoing, task })
}

fn now_ms() -> i64 {
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

fn emit_state<R: tauri::Runtime>(app: &AppHandle<R>, payload: StatePayload) {
    if let Err(e) = app.emit(events::STATE, payload) {
        warn!(error = %e, "failed to emit session state");
    }
}

fn emit_input_mode<R: tauri::Runtime>(app: &AppHandle<R>, password: bool) {
    if let Err(e) = app.emit(events::INPUT_MODE, InputModePayload { password }) {
        warn!(error = %e, "failed to emit input mode");
    }
}

#[cfg(test)]
mod tests;
