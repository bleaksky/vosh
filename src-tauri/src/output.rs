//! The one path that writes text to the terminal. Every write, from a
//! read of the game, a slash command's echo or a timer, reaches the native
//! grid and xterm here, under one lock, so both renderers take the same
//! text in the same order. Each write names the session whose terminal
//! takes it.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::warn;
use vosh_prompt::stage::Output;

use crate::app::events;
use crate::sessions::{Session, SessionId};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OutputPayload {
    /// The session whose terminal takes the output. Every payload
    /// [`emit_counted`] sends names it, in the payload itself since this
    /// is the hot path. A payload built from the stage alone, as the
    /// session tests build them, names none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionId>,
    /// Output bytes as standard base64. A raw `Vec<u8>` serializes to a
    /// JSON array of decimal numbers (~4x the wire size, one number per
    /// byte, plus an N-element JS array to walk on the other side);
    /// base64 is a single compact string the webview decodes in one pass.
    pub b64: String,
    /// Replace a region an earlier payload marked, applied before `b64`.
    /// See `vosh_prompt::stage::Replace`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<ReplacePayload>,
    /// The live render for the region this payload leaves open, as
    /// base64, written back before anything else lands on the renderer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore: Option<String>,
    /// What the band above the command line shows from now on, as base64,
    /// while your prompt shows pinned. An empty string clears the band.
    /// Only the band reads it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    /// Where each piece of your design landed on the band `pin` shows,
    /// rows counted from the band's first. Absent when the band shows no
    /// design. See `vosh_prompt::stage::Output::pin_spans`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_spans: Option<Vec<vosh_prompt::Span>>,
    /// Line ends at the end of this payload that each renderer keeps back
    /// until the next write lands on it, as base64. See
    /// `vosh_prompt::stage::Output::hold`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<String>,
    /// While your prompt shows pinned, whether the pinned prompt's row is
    /// still where the next thing lands after this payload, so each
    /// renderer drops the line end that would end that row. See
    /// `vosh_prompt::stage::close_pin_row`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_row: Option<bool>,
    /// The bytes start a row of their own, so each renderer writes a line
    /// end first when its cursor sits past the start of a row. Absent
    /// when false. See `vosh_prompt::stage::Output::fresh`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fresh: Option<bool>,
    /// Which output of the prompt stage this is (`Output::id`). Each
    /// renderer keeps the newest it took, so text the webview writes
    /// itself can tell the session which output it follows. Absent on
    /// output from elsewhere, such as a slash command's echo, which the
    /// stage never sees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
}

/// `OutputPayload.replace`: region `gen`, its new bytes as base64, and
/// whether they are written on a new row when the region is closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReplacePayload {
    pub gen: u64,
    pub b64: String,
    pub fresh: bool,
    /// The lines the region's prompt shows right above it, which a change
    /// of where your prompt shows moves with it. See
    /// `vosh_prompt::stage::Above`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub above: Option<AbovePayload>,
    /// The end of the region the bytes leave out, as base64, which a
    /// renderer that writes them on a new row, or finds the region open
    /// with nothing held back, holds back in their place. See
    /// `vosh_prompt::stage::Replace::tail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail: Option<String>,
}

/// `ReplacePayload.above`: the lines' plain text, and what to write from
/// their first row, as base64, when a renderer finds them there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AbovePayload {
    pub plain: String,
    pub b64: String,
}

/// Standard base64 (RFC 4648, padded) encoder. Hand-rolled to keep the
/// output hot path dependency-free; the webview decodes with the
/// built-in `atob`.
pub(crate) fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

impl OutputPayload {
    /// Build the wire payload for an output, its bytes as base64.
    pub(crate) fn from_output(out: &Output) -> Self {
        Self {
            session: None,
            b64: base64_encode(&out.bytes),
            replace: out.replace.as_ref().map(|r| ReplacePayload {
                gen: r.gen,
                b64: base64_encode(&r.bytes),
                fresh: r.fresh,
                above: r.above.as_ref().map(|a| AbovePayload {
                    plain: a.plain.clone(),
                    b64: base64_encode(&a.bytes),
                }),
                tail: (!r.tail.is_empty()).then(|| base64_encode(&r.tail)),
            }),
            restore: out.restore.as_deref().map(base64_encode),
            pin: out.pin.as_deref().map(base64_encode),
            pin_spans: out.pin_spans.clone(),
            hold: (!out.hold.is_empty()).then(|| base64_encode(&out.hold)),
            pin_row: out.pin_row,
            fresh: out.fresh.then_some(true),
            id: None,
        }
    }
}

/// Held across both halves of [`emit_output`], so the native grid and
/// xterm take the output of every caller in the same order.
static OUTPUT_ORDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Print `bytes` in the terminal of `session`. Every write to the
/// terminal pane goes through here or through the session's own batch: a
/// slash command's echo, the `#logs` reply, a timer's echo, and the rest.
/// Nothing else emits `session://output`. It moves the session's output
/// count, so it closes the session's open row.
pub(crate) fn emit_output<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    bytes: Vec<u8>,
) {
    let mut out = Output::new(false);
    out.text(&bytes);
    emit_counted(app, session, &out, true, false, true);
}

/// Print lines Vosh says about itself, such as a slash command's reply,
/// a `[walk]` line or a `[lua]` error, one to a row, in the terminal of
/// `session`. They start a row of their own: a game prompt that landed
/// after the echo of your line, or that the line came with no echo
/// after, ends its row first, and a terminal already at the start of a
/// row gets no blank one. They go through [`emit_output`]'s path like
/// every other terminal write, so the native renderer shows them too.
pub(crate) fn echo_lines<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    lines: &[String],
) {
    emit_lines(app, session, lines, true);
}

/// Print the echo of a command you send that Vosh draws itself, such as
/// a quick key's, then the lines after it, in the terminal of `session`.
/// The echo lands at the cursor, after the game's prompt, as the echo
/// the command line draws does.
pub(crate) fn echo_command<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    lines: &[String],
) {
    emit_lines(app, session, lines, false);
}

/// Print `lines`, each ended with a line end. `fresh` starts them on a
/// row of their own.
fn emit_lines<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    lines: &[String],
    fresh: bool,
) {
    if lines.is_empty() {
        return;
    }
    let mut buf = Vec::new();
    for line in lines {
        buf.extend_from_slice(line.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }
    let mut out = Output::new(false);
    out.text(&buf);
    out.fresh = fresh;
    emit_counted(app, session, &out, true, false, true);
}

/// Send a repaint of the open row of `session`. It leaves the output
/// count alone, since the row it writes is still the last thing on
/// screen.
pub(crate) fn emit_repaint<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, out: &Output) {
    let _ = emit_counted(app, session, out, false, true, true);
}

/// Send `out` to both renderers of `session` under [`OUTPUT_ORDER`].
/// `count` moves the session's output count, which a repaint of the open
/// row never does. `staged` says the prompt stage made `out`, so it
/// carries its id, which each renderer keeps as the newest it took. The
/// session task makes and sends those in order. Output from elsewhere can
/// take an id before an output of the session and still go out after it,
/// so it carries none. `frame` asks the native renderer for a frame at
/// once, while the session's grid shows. Returns the count after it.
pub(crate) fn emit_counted<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    out: &Output,
    count: bool,
    staged: bool,
    frame: bool,
) -> u64 {
    let id = staged.then(|| out.id());
    let payload = OutputPayload {
        session: Some(session.id),
        id,
        ..OutputPayload::from_output(out)
    };
    // The session loop and the command handlers write from different
    // tasks. Without the lock, two writes could reach the grid in one
    // order and xterm in the other.
    let _order = OUTPUT_ORDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let seen = if count {
        session.count_output()
    } else {
        session.output_count()
    };
    // Feed the session's native grid the same bytes xterm receives,
    // for every output path, then repaint. This is the single choke point
    // so nothing reaches xterm without also reaching the grid.
    // Word wrapped at the grid width, matching the frontend WordWrapper
    // that xterm receives this same stream through. The grid finds each
    // region in its own rows, as xterm does.
    #[cfg(any(native_surface, test))]
    crate::native::grid::feed_session_output(session.id, out, id);
    if frame {
        request_frame(app, session);
    }
    if let Err(e) = app.emit(events::OUTPUT, payload) {
        warn!(error = %e, "failed to emit session output");
    }
    seen
}

/// The test event a frame request sends, so a test can count frames.
#[cfg(test)]
pub(crate) const TEST_FRAME_EVENT: &str = "test://frame";

/// Ask the native renderer for a frame of what the grid of `session`
/// holds now. A session whose grid does not show asks for none, since
/// the frame draws only the grid that shows.
pub(crate) fn request_frame<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session) {
    #[cfg(any(native_surface, test))]
    if crate::native::grid::shown() != session.id {
        return;
    }
    #[cfg(not(any(native_surface, test)))]
    let _ = session;
    #[cfg(native_surface)]
    crate::native::surface::request_redraw();
    #[cfg(test)]
    let _ = app.emit(TEST_FRAME_EVENT, ());
    #[cfg(not(test))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::base64_encode;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_handles_raw_ansi_and_high_bytes() {
        // ESC [ 3 1 m  (a red SGR sequence) plus a high byte.
        assert_eq!(base64_encode(&[0x1b, 0x5b, 0x33, 0x31, 0x6d]), "G1szMW0=");
        assert_eq!(base64_encode(&[0xff, 0xfe, 0xfd]), "//79");
    }
}
