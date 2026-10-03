//! The session log's row for a connection, and the lines the session
//! captures as it ends. The game's last partial line and the lines the
//! stage still holds would otherwise go with the loop, so they go in the
//! log and the scrollback ring first.

use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::Mutex;
use tracing::warn;

use crate::output::emit_output;
use crate::profile::Profile;

use super::lines::{LineAccumulator, Partial};
use super::now_ms;
use super::steps::end_held;

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

    /// Name the character the row belongs to, the first time Char.Status
    /// names one, so the prompt lookup can tell whose session it was.
    /// Char.Status comes again on later pulses, and those write nothing.
    pub(super) async fn name(&mut self, logs: &crate::logs::SharedLogStore, character: &str) {
        let Some(id) = self.id else {
            return;
        };
        if self.named {
            return;
        }
        self.named = true;
        let mut guard = logs.lock().await;
        if let Some(store) = guard.as_mut() {
            if let Err(e) = store.set_session_character(id, character) {
                warn!(error = %e, "failed to name the log session's character");
            }
        }
    }
}

/// Log the lines the stage still holds as the session ends, and keep them
/// for scrollback, through [`end_held`].
pub(super) async fn capture_held_lines(
    profile: &Arc<Mutex<Profile>>,
    logs: &crate::logs::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: &crate::logs::SharedScrollback,
) {
    let (log, kept) = end_held(&mut *profile.lock().await, log_session_id);
    if !kept.is_empty() {
        let mut ring = scrollback.lock().await;
        for text in kept {
            ring.push(text);
        }
    }
    if !log.is_empty() {
        let mut guard = logs.lock().await;
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
    logs: &crate::logs::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: &crate::logs::SharedScrollback,
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
    emit_output(app, out);
    scrollback.lock().await.push(bytes.clone());
    if let Some(sid) = log_session_id {
        let mut guard = logs.lock().await;
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
