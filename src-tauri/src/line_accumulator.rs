//! Buffer incoming server bytes into complete lines for the session loop.
//! A line ends at `\n`. The bytes after the last `\n`, the partial, stay
//! here until a later read completes them, a GA or EOR ends them, or you
//! send a line.
//!
//! The partial never paints on its own. At the end of each read the
//! session decides: a partial the prompt capture settles on is your
//! prompt at once, and any other paints as a region (see
//! `vosh_prompt::stage`), so a partial that becomes a prompt in the same
//! read never flashes. The accumulator remembers the region the partial
//! was painted as, and the line that completes it carries that region so
//! the session replaces it with the processed line.

/// A complete line from the server, without its line end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) bytes: Vec<u8>,
    /// The region an earlier read painted this line's start as, which the
    /// processed line replaces.
    pub(crate) painted: Option<u64>,
}

/// The partial a GA or EOR ended, or a send took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Partial {
    pub(crate) bytes: Vec<u8>,
    /// The region it was painted as, when an earlier read painted it, and
    /// how many of its bytes that region holds. A partial that grew in
    /// the read that ends it holds more.
    pub(crate) painted: Option<(u64, usize)>,
}

#[derive(Debug, Default)]
pub(crate) struct LineAccumulator {
    /// Bytes since the last `\n`. The next chunk extends this until a
    /// newline arrives.
    buffer: Vec<u8>,
    /// The region the partial was painted as, and how many of its bytes.
    painted: Option<(u64, usize)>,
}

impl LineAccumulator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Append new bytes and return the lines they completed, in order.
    /// The first carries the region the partial it completed was painted
    /// as.
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<Line> {
        self.buffer.extend_from_slice(bytes);

        let mut lines = Vec::new();
        let mut start = 0;
        let mut last_consumed = None;

        for i in 0..self.buffer.len() {
            if self.buffer[i] != b'\n' {
                continue;
            }
            // ROM-derived MUDs (Aabahran, Forsaken) terminate lines with
            // `\n\r` instead of standard `\r\n`. Splitting on `\n` then
            // leaves the `\r` at the *start* of the next line. Strip both
            // leading and trailing `\r` so anchored trigger patterns work
            // against the actual line text and xterm doesn't see a stray
            // CR that would slam the cursor back to column zero.
            let mut line_start = start;
            let mut line_end = i;
            if line_end > line_start && self.buffer[line_end - 1] == b'\r' {
                line_end -= 1;
            }
            if line_start < line_end && self.buffer[line_start] == b'\r' {
                line_start += 1;
            }
            lines.push(Line {
                bytes: self.buffer[line_start..line_end].to_vec(),
                painted: self.painted.take().map(|(gen, _)| gen),
            });
            start = i + 1;
            last_consumed = Some(start);
        }

        if let Some(consumed) = last_consumed {
            self.buffer.drain(..consumed);
        }

        // ROM `\n\r` debris: when the remainder starts a brand-new line
        // (none of it painted yet) with the `\r` that belongs to the
        // previous line's terminator, drop it from the buffer. Left alone
        // it paints at the end of the read and slams the cursor back to
        // column zero right after whatever the read drew, the custom
        // prompt for one, so the next echo overwrites that row. A `\r`
        // inside a partial (deliberate overwrite) is untouched.
        if self.painted.is_none() && self.buffer.first() == Some(&b'\r') {
            self.buffer.remove(0);
        }

        lines
    }

    /// The partial, the bytes after the last line end. None when there is
    /// none.
    pub(crate) fn partial(&self) -> Option<&[u8]> {
        (!self.buffer.is_empty()).then_some(&self.buffer[..])
    }

    /// The region the partial was painted as, and how many of its bytes.
    pub(crate) fn painted(&self) -> Option<(u64, usize)> {
        self.painted
    }

    /// Note the region the partial is painted as now.
    pub(crate) fn set_painted(&mut self, painted: Option<(u64, usize)>) {
        self.painted = painted;
    }

    /// Take the partial out, as a GA or EOR does when it ends it, or the
    /// session does when it reads a partial as your prompt. None when
    /// there is none.
    pub(crate) fn take_partial(&mut self) -> Option<Partial> {
        let painted = self.painted.take();
        if self.buffer.is_empty() {
            return None;
        }
        Some(Partial {
            bytes: std::mem::take(&mut self.buffer),
            painted,
        })
    }

    /// Drop the partial without drawing it again, as a send does. Your
    /// typed echo already moved the cursor past it, so the next chunk from
    /// the server starts fresh instead of merging with it.
    pub(crate) fn forget_partial(&mut self) {
        self.buffer.clear();
        self.painted = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(bytes: &[u8]) -> Line {
        Line {
            bytes: bytes.to_vec(),
            painted: None,
        }
    }

    #[test]
    fn one_complete_line_no_partial() {
        let mut a = LineAccumulator::new();
        assert_eq!(a.feed(b"hello\n"), vec![line(b"hello")]);
        assert_eq!(a.partial(), None);
    }

    #[test]
    fn crlf_stripped() {
        let mut a = LineAccumulator::new();
        let lines = a.feed(b"hello\r\nworld\r\n");
        assert_eq!(lines, vec![line(b"hello"), line(b"world")]);
    }

    #[test]
    fn a_partial_waits_without_painting() {
        let mut a = LineAccumulator::new();
        let leftover = &a.feed(b"Login: ");
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(a.partial(), Some(&b"Login: "[..]));
        assert_eq!(a.painted(), None);
    }

    #[test]
    fn the_line_completing_a_painted_partial_carries_its_region() {
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"Login: ");
        a.set_painted(Some((7, 7)));
        let lines = a.feed(b"Bob\nnext\n");
        assert_eq!(
            lines,
            vec![
                Line {
                    bytes: b"Login: Bob".to_vec(),
                    painted: Some(7),
                },
                line(b"next"),
            ]
        );
        assert_eq!(a.painted(), None);
    }

    #[test]
    fn a_line_then_a_new_partial_in_one_chunk() {
        let mut a = LineAccumulator::new();
        assert_eq!(a.feed(b"first line\nLogin: "), vec![line(b"first line")]);
        assert_eq!(a.partial(), Some(&b"Login: "[..]));
    }

    #[test]
    fn a_partial_grows_and_keeps_its_region() {
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"Wel");
        a.set_painted(Some((3, 3)));
        let leftover = &a.feed(b"come ");
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(a.partial(), Some(&b"Welcome "[..]));
        assert_eq!(a.painted(), Some((3, 3)));
    }

    #[test]
    fn empty_line_kept() {
        let mut a = LineAccumulator::new();
        assert_eq!(a.feed(b"\n"), vec![line(b"")]);
    }

    #[test]
    fn taking_the_partial_hands_over_its_region() {
        let mut a = LineAccumulator::new();
        assert_eq!(a.take_partial(), None);
        let _ = a.feed(b"<10hp> ");
        assert_eq!(
            a.take_partial(),
            Some(Partial {
                bytes: b"<10hp> ".to_vec(),
                painted: None,
            })
        );
        let _ = a.feed(b"<10hp> ");
        a.set_painted(Some((4, 7)));
        assert_eq!(
            a.take_partial(),
            Some(Partial {
                bytes: b"<10hp> ".to_vec(),
                painted: Some((4, 7)),
            })
        );
        assert_eq!(a.partial(), None);
        assert_eq!(a.painted(), None);
        // A partial that grew after its paint says how much of it the
        // region holds, so the rest is not taken as on screen.
        let _ = a.feed(b"<10");
        a.set_painted(Some((5, 3)));
        let _ = a.feed(b"hp> ");
        assert_eq!(
            a.take_partial(),
            Some(Partial {
                bytes: b"<10hp> ".to_vec(),
                painted: Some((5, 3)),
            })
        );
    }

    #[test]
    fn forget_drops_the_partial_and_its_region() {
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"Login: ");
        a.set_painted(Some((1, 7)));
        a.forget_partial();
        assert_eq!(a.partial(), None);
        assert_eq!(a.feed(b"new\n"), vec![line(b"new")]);
    }

    #[test]
    fn rom_style_lf_cr_terminator() {
        // Aabahran / ROM 2.4 terminates lines with `\n\r` rather than
        // standard `\r\n`. Splitting on `\n` leaves a `\r` glued to the
        // start of the next line, which used to break `^`-anchored
        // trigger patterns. The accumulator strips it from line text AND
        // from the trailing partial: painted at the end of the read it
        // would slam the cursor to column zero right after in-batch
        // renders like the custom prompt, and the next echo would
        // overwrite that row.
        let mut a = LineAccumulator::new();
        let lines = a.feed(b"first\n\rsecond\n\rthird\n\r");
        assert_eq!(lines, vec![line(b"first"), line(b"second"), line(b"third")]);
        assert_eq!(a.partial(), None);
    }

    #[test]
    fn lf_cr_split_across_chunks() {
        // The `\r` that prefixes the next line might arrive in the next
        // network chunk. The accumulator should still strip it.
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"first\n");
        assert_eq!(a.feed(b"\rsecond\n\r"), vec![line(b"second")]);
        assert_eq!(a.partial(), None);
    }

    #[test]
    fn deliberate_carriage_return_inside_a_partial_survives() {
        // Only line-terminator debris is stripped: a server overwriting a
        // partial line with `\r` mid-stream keeps its carriage return.
        let mut a = LineAccumulator::new();
        let leftover = &a.feed(b"loading 1%\rloading 2%");
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(a.partial(), Some(&b"loading 1%\rloading 2%"[..]));
        // And a painted partial that the next read continues with a `\r`
        // keeps it too.
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"loading 1%");
        a.set_painted(Some((1, 10)));
        let _ = a.feed(b"\rloading 2%");
        assert_eq!(a.partial(), Some(&b"loading 1%\rloading 2%"[..]));
    }
}
