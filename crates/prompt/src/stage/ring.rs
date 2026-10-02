//! The candidates ring, one entry per send and per GA or EOR, which the
//! prompt card reads to show and check your prompt.

use super::blocks::Block;
use super::{Stage, RING};

/// One entry of the candidates ring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Which entry this is. Entries count up from 1 and never start over,
    /// so an id the card holds never names a later entry.
    pub id: u64,
    /// As the game sent it, colors included, before any gag.
    pub raw: Vec<u8>,
    pub plain: String,
    /// Milliseconds since the epoch.
    pub at_ms: i64,
    /// Vosh read it as your prompt.
    pub recognized: bool,
    /// Drawing was on.
    pub draw: bool,
    /// The profile had a capture.
    pub capture: bool,
}

/// A line the ring may record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Seen {
    raw: Vec<u8>,
    plain: String,
    recognized: bool,
}

impl Stage {
    /// Note a complete line that is not your prompt for the candidates
    /// ring.
    pub(super) fn note_line(&mut self, raw: &[u8], plain: &str) {
        self.unrecorded = true;
        if !plain.trim().is_empty() {
            // Reuse the buffers, since this runs for every line.
            let seen = self.last_line.get_or_insert_with(Seen::default);
            seen.raw.clear();
            seen.raw.extend_from_slice(raw);
            seen.plain.clear();
            seen.plain.push_str(plain);
        }
    }

    pub(super) fn note_recognized(&mut self, block: Block) {
        self.recognized += 1;
        self.unrecorded = true;
        let mut raw = Vec::new();
        let mut plain = String::new();
        for (i, line) in block.lines.iter().enumerate() {
            if i > 0 {
                raw.extend_from_slice(b"\r\n");
                plain.push('\n');
            }
            raw.extend_from_slice(&line.raw);
            plain.push_str(&line.plain);
        }
        self.pending = Some(Seen {
            raw,
            plain,
            recognized: true,
        });
        self.last_raw = Some(block);
    }

    /// Record one candidate, on every send and every GA or EOR: the
    /// latest recognized prompt since the last entry, else `partial`,
    /// else the latest complete line. When nothing came in since the last
    /// entry, nothing is recorded. `draw` and `capture` say whether
    /// drawing was on and whether the profile had a capture.
    pub(crate) fn record(
        &mut self,
        partial: Option<(&[u8], &str)>,
        at_ms: i64,
        draw: bool,
        capture: bool,
    ) {
        if !self.unrecorded {
            return;
        }
        self.unrecorded = false;
        let partial = partial
            .filter(|(_, plain)| !plain.trim().is_empty())
            .map(|(raw, plain)| Seen {
                raw: raw.to_vec(),
                plain: plain.to_string(),
                recognized: false,
            });
        let Some(seen) = self
            .pending
            .take()
            .or(partial)
            .or_else(|| self.last_line.clone())
        else {
            return;
        };
        if self.ring.len() == RING {
            self.ring.pop_front();
        }
        self.candidates += 1;
        self.ring.push_back(Candidate {
            id: self.candidates,
            raw: seen.raw,
            plain: seen.plain,
            at_ms,
            recognized: seen.recognized,
            draw,
            capture,
        });
    }

    /// The candidates ring, oldest first.
    pub fn ring(&self) -> impl Iterator<Item = &Candidate> {
        self.ring.iter()
    }

    /// The ring entry with this id, while the ring still holds it.
    pub fn candidate(&self, id: u64) -> Option<&Candidate> {
        self.ring.iter().find(|c| c.id == id)
    }
}
