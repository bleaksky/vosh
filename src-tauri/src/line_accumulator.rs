//! Buffer incoming server bytes and emit complete lines for trigger
//! processing. A line ends at `\n`. Anything after the last `\n` stays
//! buffered until the next chunk arrives.
//!
//! Phase 4 will add prompt detection via the telnet GA and EOR commands so
//! prompts without a trailing newline still trigger.

#[derive(Debug, Default)]
pub(crate) struct LineAccumulator {
    buffer: Vec<u8>,
}

impl LineAccumulator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Append bytes and return any complete lines (trailing `\r\n` or `\n`
    /// stripped).
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.buffer.extend_from_slice(bytes);
        let mut lines = Vec::new();
        let mut start = 0;
        for (i, &b) in self.buffer.iter().enumerate() {
            if b == b'\n' {
                let end = if i > 0 && self.buffer[i - 1] == b'\r' {
                    i - 1
                } else {
                    i
                };
                lines.push(self.buffer[start..end].to_vec());
                start = i + 1;
            }
        }
        if start > 0 {
            self.buffer.drain(..start);
        }
        lines
    }

    /// Drop any buffered partial line. Call on disconnect.
    pub(crate) fn reset(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_complete_line() {
        let mut a = LineAccumulator::new();
        let lines = a.feed(b"hello\n");
        assert_eq!(lines, vec![b"hello".to_vec()]);
    }

    #[test]
    fn crlf_stripped() {
        let mut a = LineAccumulator::new();
        let lines = a.feed(b"hello\r\nworld\r\n");
        assert_eq!(lines, vec![b"hello".to_vec(), b"world".to_vec()]);
    }

    #[test]
    fn partial_buffered_until_newline() {
        let mut a = LineAccumulator::new();
        assert!(a.feed(b"part").is_empty());
        let lines = a.feed(b"ial\n");
        assert_eq!(lines, vec![b"partial".to_vec()]);
    }

    #[test]
    fn multiple_lines_in_one_chunk() {
        let mut a = LineAccumulator::new();
        let lines = a.feed(b"a\nb\nc\n");
        assert_eq!(lines, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    }

    #[test]
    fn empty_line_kept() {
        let mut a = LineAccumulator::new();
        let lines = a.feed(b"\n");
        assert_eq!(lines, vec![Vec::<u8>::new()]);
    }

    #[test]
    fn reset_drops_buffered_partial() {
        let mut a = LineAccumulator::new();
        let _ = a.feed(b"part");
        a.reset();
        let lines = a.feed(b"new\n");
        assert_eq!(lines, vec![b"new".to_vec()]);
    }
}
