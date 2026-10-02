//! The private marks that start a region and start and end a lift, and
//! the bytes of a region that carries a lift.

use super::blocks::OpenLift;
use super::output::{escape_end, trailing_line_ends};

/// The private OSC Vosh marks regions with.
pub const MARK_OSC: u32 = 7717;

/// The mark that starts region `gen`, `ESC ] 7717 ; o ; G BEL`.
pub fn mark(gen: u64) -> Vec<u8> {
    format!("\x1b]{MARK_OSC};o;{gen}\x07").into_bytes()
}

/// The mark that starts lift `id`, the prompt a band goes under while
/// your prompt shows lifted: `ESC ] 7717 ; l ; L BEL`.
pub fn lift_start(id: u64) -> Vec<u8> {
    format!("\x1b]{MARK_OSC};l;{id}\x07").into_bytes()
}

/// The mark that ends lift `id`, right after its last visible byte:
/// `ESC ] 7717 ; e ; L BEL`. A repaint writes it again, and a renderer
/// takes the latest one as where the lift ends.
pub fn lift_end(id: u64) -> Vec<u8> {
    format!("\x1b]{MARK_OSC};e;{id}\x07").into_bytes()
}

/// `body` with the end mark of lift `id` after its last visible byte, so
/// before the line ends it finishes on. A body that finishes on no line
/// end and on a character other than a space gets one plain space after
/// the mark, so your echo starts a cell later and the band's 4 px reach
/// past the last glyph stays inside that cell instead of under your echo.
pub fn with_lift_end(body: &[u8], id: u64) -> Vec<u8> {
    let at = trailing_line_ends(body);
    let (shown, ends) = body.split_at(at);
    let mut out = shown.to_vec();
    out.extend(lift_end(id));
    if ends.is_empty() && last_shown_char(shown).is_some_and(|c| c != b' ') {
        out.push(b' ');
    }
    out.extend_from_slice(ends);
    out
}

/// The last byte of `bytes` that shows, escape sequences skipped.
fn last_shown_char(bytes: &[u8]) -> Option<u8> {
    let mut last = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i = escape_end(bytes, i);
            continue;
        }
        if !matches!(bytes[i], b'\r' | b'\n') {
            last = Some(bytes[i]);
        }
        i += 1;
    }
    last
}

/// Region `gen` holding `body`: its mark, then, while it carries a lift,
/// the lift's start when it sits inside the region, the body and the
/// lift's end.
pub(super) fn region_bytes(gen: u64, lift: Option<OpenLift>, body: &[u8]) -> Vec<u8> {
    let mut bytes = mark(gen);
    match lift {
        Some(lift) => {
            if lift.start_inside {
                bytes.extend(lift_start(lift.id));
            }
            bytes.extend(end_lift(body, lift));
        }
        None => bytes.extend_from_slice(body),
    }
    bytes
}

/// `body` with the end mark of `lift` and Lifted's space after it, right
/// before the line ends it finishes on.
pub(super) fn end_lift(body: &[u8], lift: OpenLift) -> Vec<u8> {
    with_lift_end(body, lift.id)
}
