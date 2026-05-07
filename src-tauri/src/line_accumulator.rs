//! Buffer incoming server bytes and emit a sequence of operations for the
//! session loop. Complete lines (terminated by `\n`) flow through trigger
//! processing. Partial bytes after the last `\n` (typically a prompt waiting
//! for input) display immediately so the user sees them without waiting for
//! a newline that may never come.
//!
//! When a partial that has already been displayed later completes into a
//! full line, the resulting [`ChunkOp::LineComplete`] carries `clear_first`
//! so the session loop can wipe the on-screen partial before drawing the
//! trigger-processed line.
//!
//! Phase 4 will add prompt detection via the telnet GA and EOR commands so
//! prompts can flow through trigger processing too.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChunkOp {
    /// A complete line ready for trigger processing. `clear_first` is true
    /// when the partial portion of this line was already shown raw and the
    /// session loop must clear the current terminal line before writing the
    /// processed result.
    LineComplete { bytes: Vec<u8>, clear_first: bool },
    /// New partial bytes to render directly to the terminal without trigger
    /// processing.
    RawDisplay(Vec<u8>),
}

#[derive(Debug, Default)]
pub(crate) struct LineAccumulator {
    /// Bytes since the last `\n`. The next chunk extends this until a
    /// newline arrives.
    buffer: Vec<u8>,
    /// How many bytes of `buffer` have already been emitted as `RawDisplay`.
    displayed_len: usize,
    /// Wall-clock instant of the last byte appended to `buffer`. The
    /// session loop polls this on each tick; when the partial has been
    /// idle past a threshold, it gets flushed as a prompt even if the
    /// server never sent EOR or GA.
    last_byte_at: Option<tokio::time::Instant>,
}

impl LineAccumulator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Append new bytes and return the operations the session loop should
    /// perform in order.
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<ChunkOp> {
        if !bytes.is_empty() {
            self.last_byte_at = Some(tokio::time::Instant::now());
        }
        let prev_displayed = self.displayed_len;
        self.buffer.extend_from_slice(bytes);

        let mut ops = Vec::new();
        let mut start = 0;
        let mut first_complete = true;
        let mut last_consumed = None;

        for i in 0..self.buffer.len() {
            if self.buffer[i] != b'\n' {
                continue;
            }
            let end = if i > start && self.buffer[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            let line_bytes = self.buffer[start..end].to_vec();
            let clear_first = first_complete && prev_displayed > 0;
            ops.push(ChunkOp::LineComplete {
                bytes: line_bytes,
                clear_first,
            });
            first_complete = false;
            start = i + 1;
            last_consumed = Some(start);
        }

        if let Some(consumed) = last_consumed {
            self.buffer.drain(..consumed);
            self.displayed_len = 0;
        }

        if self.buffer.len() > self.displayed_len {
            let new_to_show = self.buffer[self.displayed_len..].to_vec();
            ops.push(ChunkOp::RawDisplay(new_to_show));
            self.displayed_len = self.buffer.len();
        }

        ops
    }

    /// Drop any buffered partial line. Call on disconnect.
    pub(crate) fn reset(&mut self) {
        self.buffer.clear();
        self.displayed_len = 0;
        self.last_byte_at = None;
    }

    /// Time since the last byte was appended, if a partial is still buffered.
    pub(crate) fn idle_since(&self, now: tokio::time::Instant) -> Option<std::time::Duration> {
        if self.buffer.is_empty() {
            return None;
        }
        self.last_byte_at
            .map(|at| now.saturating_duration_since(at))
    }

    /// Take the current partial out of the buffer and return it as bytes.
    /// Used when the telnet GA or EOR command arrives, telling us the
    /// partial is in fact a complete prompt that should sit on its own
    /// line. Returns None when no partial is buffered.
    ///
    /// `displayed` reports how many bytes had already been shown raw, so
    /// the caller knows whether the line needs the clear-and-rewrite
    /// treatment when re-emitting through trigger processing.
    pub(crate) fn flush_partial(&mut self) -> Option<(Vec<u8>, bool)> {
        if self.buffer.is_empty() {
            return None;
        }
        let already_shown = self.displayed_len > 0;
        let bytes = std::mem::take(&mut self.buffer);
        self.displayed_len = 0;
        self.last_byte_at = None;
        Some((bytes, already_shown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(bytes: &[u8], clear_first: bool) -> ChunkOp {
        ChunkOp::LineComplete {
            bytes: bytes.to_vec(),
            clear_first,
        }
    }

    fn raw(bytes: &[u8]) -> ChunkOp {
        ChunkOp::RawDisplay(bytes.to_vec())
    }

    #[test]
    fn one_complete_line_no_partial() {
        let mut a = LineAccumulator::new();
        let ops = a.feed(b"hello\n");
        assert_eq!(ops, vec![line(b"hello", false)]);
    }

    #[test]
    fn crlf_stripped() {
        let mut a = LineAccumulator::new();
        let ops = a.feed(b"hello\r\nworld\r\n");
        assert_eq!(ops, vec![line(b"hello", false), line(b"world", false)]);
    }

    #[test]
    fn partial_emits_raw_immediately() {
        let mut a = LineAccumulator::new();
        let ops = a.feed(b"Login: ");
        assert_eq!(ops, vec![raw(b"Login: ")]);
    }

    #[test]
    fn partial_then_completion_carries_clear_first() {
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"Login: ");
        let ops = a.feed(b"Bob\n");
        assert_eq!(ops, vec![line(b"Login: Bob", true)]);
    }

    #[test]
    fn line_then_new_partial_in_one_chunk() {
        let mut a = LineAccumulator::new();
        let ops = a.feed(b"first line\nLogin: ");
        assert_eq!(ops, vec![line(b"first line", false), raw(b"Login: ")]);
    }

    #[test]
    fn multiple_complete_lines_only_first_clears() {
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"part");
        let ops = a.feed(b"ial\nmore\nlast\n");
        assert_eq!(
            ops,
            vec![
                line(b"partial", true),
                line(b"more", false),
                line(b"last", false),
            ]
        );
    }

    #[test]
    fn extending_partial_emits_only_new_bytes() {
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"Wel");
        let ops = a.feed(b"come ");
        assert_eq!(ops, vec![raw(b"come ")]);
    }

    #[test]
    fn empty_line_kept() {
        let mut a = LineAccumulator::new();
        let ops = a.feed(b"\n");
        assert_eq!(ops, vec![line(b"", false)]);
    }

    #[test]
    fn reset_drops_buffered_partial() {
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"Login: ");
        a.reset();
        let ops = a.feed(b"new\n");
        assert_eq!(ops, vec![line(b"new", false)]);
    }
}
