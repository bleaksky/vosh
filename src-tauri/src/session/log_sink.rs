//! The log sink of a connection. It holds the session log's row for the
//! connection and the scrollback ring, which it saves as the session
//! ends. The lines the session captures as it ends go through it too. The
//! game's last partial line and the lines the stage still holds would
//! otherwise go with the loop, so they go in the log and the scrollback
//! ring first.

use std::path::PathBuf;

use tauri::AppHandle;
use tracing::warn;

use crate::logs::{SharedLogStore, SharedScrollback};
use crate::output::emit_output;
use crate::sessions::Session;

use super::connection::SharedConnection;
use super::lines::{LineAccumulator, Partial};
use super::now_ms;
use super::steps::end_held;

/// One connection's log sink. The log store and the scrollback ring are
/// the app's, shared with the commands that read them. The row and the
/// file the ring is saved to are this connection's.
pub(super) struct LogSink {
    pub(super) logs: SharedLogStore,
    session: LogSession,
    pub(super) scrollback: SharedScrollback,
    scrollback_path: Option<PathBuf>,
}

impl LogSink {
    /// Open the log's row for `session`'s connection to `host` on
    /// `port`, so every row the session writes attaches to it, and note it
    /// on the session. With `logged` false, which Log sessions off gives,
    /// with no log store, or with a row that fails to open, the session
    /// writes no rows.
    pub(super) async fn open(
        logs: SharedLogStore,
        logged: bool,
        session: &Session,
        scrollback_path: Option<PathBuf>,
        host: &str,
        port: u16,
    ) -> Self {
        let id = if logged {
            let mut guard = logs.lock().await;
            match guard.as_mut() {
                Some(store) => match store.start_session(host, port, now_ms()) {
                    Ok(id) => Some(id),
                    Err(e) => {
                        warn!(error = %e, "failed to open log session");
                        None
                    }
                },
                None => None,
            }
        } else {
            None
        };
        if let Some(id) = id {
            session.note_log(id);
        }
        Self {
            logs,
            session: LogSession::new(id),
            scrollback: session.scrollback.clone(),
            scrollback_path,
        }
    }

    /// The log's row for this connection, if the session logs.
    pub(super) fn id(&self) -> Option<i64> {
        self.session.id
    }

    /// The row and the character to name on it, see [`LogSession::name`].
    pub(super) fn name(&mut self, character: &str) -> Option<(i64, String)> {
        self.session.name(character)
    }

    /// Close the log's row, then save the scrollback ring so the next
    /// launch can restore it. A failure here only warns, so the session
    /// still ends and says so, and a ring it could not save reads as
    /// changed, so the next pass or the quit tries again.
    pub(super) async fn close(self) {
        if let Some(sid) = self.session.id {
            let mut guard = self.logs.lock().await;
            if let Some(store) = guard.as_mut() {
                if let Err(e) = store.end_session(sid, now_ms()) {
                    warn!(error = %e, "log end_session failed");
                }
            }
        }
        if let Some(path) = self.scrollback_path {
            let snapshot = self.scrollback.lock().await.snapshot();
            let written = tokio::task::spawn_blocking(move || {
                crate::logs::write_scrollback(&path, &snapshot)
            })
            .await
            .unwrap_or(false);
            if !written {
                self.scrollback.lock().await.mark_changed();
            }
        }
    }
}

/// The log's row for this connection, and whether it names the
/// character yet.
pub(super) struct LogSession {
    pub(super) id: Option<i64>,
    named: bool,
}

impl LogSession {
    pub(super) fn new(id: Option<i64>) -> Self {
        Self { id, named: false }
    }

    /// The row and the character it belongs to, the first time
    /// Char.Status names one, so the prompt lookup can tell whose session
    /// it was. The name waits with the burst's rows, so a busy log never
    /// holds the loop for it. Char.Status comes again on later pulses,
    /// and those name nothing.
    pub(super) fn name(&mut self, character: &str) -> Option<(i64, String)> {
        let id = self.id?;
        if std::mem::replace(&mut self.named, true) {
            return None;
        }
        Some((id, character.to_string()))
    }
}

/// Log the lines the stage still holds as the session ends, and keep them
/// for scrollback, through [`end_held`].
pub(super) async fn capture_held_lines(connection: &SharedConnection, log_sink: &LogSink) {
    let (log, kept) = end_held(&mut connection.lock(), log_sink.id());
    if !kept.is_empty() {
        let mut ring = log_sink.scrollback.lock().await;
        for text in kept {
            ring.push(text);
        }
    }
    if !log.is_empty() {
        let mut guard = log_sink.logs.lock().await;
        if let Some(store) = guard.as_mut() {
            if let Err(e) = store.append_batch(&log) {
                warn!(error = %e, "disconnect held lines log append failed");
            }
        }
    }
}

/// Flush a partial line still buffered when the session ends so the MUD's
/// final output (a logout banner on `quit`, most often) is captured rather
/// than dropped with the session loop's accumulator. The end of its
/// read painted it, so display only needs the terminating newline. The
/// value of this pass is logging it and pushing it into the scrollback
/// ring that the dump persists.
pub(super) async fn capture_pending_line<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    log_sink: &LogSink,
    accumulator: &mut LineAccumulator,
) {
    let Some(Partial { bytes, painted }) = accumulator.take_partial() else {
        return;
    };
    let plain = vosh_protocol::ansi::plain_text(&bytes);
    // Terminate the line on screen. Write only what the end of its read
    // did not paint, to avoid printing the goodbye twice.
    let shown = painted.map_or(0, |(_, len)| len.min(bytes.len()));
    let mut out = Vec::with_capacity(bytes.len() - shown + 2);
    out.extend_from_slice(&bytes[shown..]);
    out.extend_from_slice(b"\r\n");
    emit_output(app, session, out);
    log_sink.scrollback.lock().await.push(bytes.clone());
    if let Some(sid) = log_sink.id() {
        let mut guard = log_sink.logs.lock().await;
        if let Some(store) = guard.as_mut() {
            if let Err(e) = store.append_batch(&[vosh_log::LogEntry {
                session_id: sid,
                ts_ms: now_ms(),
                text: plain,
                raw: Some(bytes),
            }]) {
                warn!(error = %e, "disconnect partial log append failed");
            }
        }
    }
}
