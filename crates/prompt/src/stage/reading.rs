//! How the stage reads your prompt. It offers each line to the capture,
//! holds the top lines of a prompt that spans lines until the rest
//! arrives, and settles the partial a read ends on.

use std::collections::BTreeSet;

use super::blocks::{Block, BlockLine, End};
use super::marks::mark;
use super::output::Output;
use super::Stage;
use crate::capture::Recognized;
use crate::design::FieldRef;

/// What the stage made of a line it was offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offer {
    /// Your prompt. The block, and the region an earlier read painted
    /// part of it as, which the drawn prompt replaces.
    Prompt(Block, Option<u64>),
    /// The top line of a prompt that spans lines. The stage holds it
    /// until the rest arrives, and writes nothing for it yet.
    Held,
    /// Not your prompt.
    Line,
}

/// A line the stage held that turned out not to start your prompt. It
/// goes through the Line pass as any other line, before the line that
/// released it. `painted` is the region it shows in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Released {
    pub raw: Vec<u8>,
    pub plain: String,
    pub painted: Option<u64>,
}

/// The lines [`Stage::offer`] released, then what it made of the line
/// offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offered {
    pub released: Vec<Released>,
    pub offer: Offer,
}

/// What the region of the held lines shows: how many held lines and how
/// long a partial after them. A region painted before the lines were
/// held shows neither, so it is painted again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HeldRegion {
    gen: u64,
    lines: usize,
    partial: usize,
}

/// Which lines above the last one your design hides: every one when it
/// reads `%{raw}`, else those that carry a value it reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Hides {
    pub(super) all: bool,
    fields: BTreeSet<String>,
}

impl Hides {
    pub(super) fn of(reads: &BTreeSet<FieldRef>) -> Self {
        Self {
            all: reads.iter().any(|f| f.name == "raw"),
            fields: reads.iter().map(|f| f.name.clone()).collect(),
        }
    }

    /// True when your design reads a value a line with these groups
    /// carries.
    pub(super) fn line(&self, groups: &[String]) -> bool {
        self.all
            || groups
                .iter()
                .any(|group| fields_of(group).iter().any(|f| self.fields.contains(*f)))
    }

    /// The lines of a prompt of `count` lines that the drawn prompt
    /// replaces, `groups` holding what each line reads: the last line,
    /// and each line above it that carries a value your design reads.
    pub(super) fn replaced(&self, count: usize, groups: &[Vec<String>]) -> Vec<usize> {
        (0..count)
            .filter(|&i| i + 1 == count || self.line(groups.get(i).map_or(&[][..], |g| &g[..])))
            .collect()
    }
}

/// The fields a capture group feeds, so a line that reads the tank's
/// health bar carries `tank_hp`.
fn fields_of(group: &str) -> Vec<&str> {
    match group {
        "tank_pct" | "tank_bar" => vec!["tank_hp", group],
        "hp_pct" | "maxhp" => vec!["hp", group],
        "mana_pct" | "maxmana" => vec!["mana", group],
        "move_pct" | "maxmove" => vec!["move", group],
        _ => vec![group],
    }
}

impl Stage {
    /// Read a line, or a partial, as your prompt on its own. `end` says
    /// how it ended. A partial with nothing after it is the prompt only
    /// when the capture settles. A prompt that settles ends the same
    /// wherever the reads split: a GA after it leaves the cursor after
    /// it, as when it settled at the end of a read, and a line end after
    /// it follows it.
    pub fn recognize(&self, raw: &[u8], plain: &str, end: End) -> Option<Block> {
        let recognizer = self.recognizer.as_ref()?;
        let read = match end {
            End::Settled => recognizer.partial(plain)?,
            End::Line | End::Marker | End::SettledLine => recognizer.line(plain)?,
        };
        let line = BlockLine {
            raw: raw.to_vec(),
            plain: plain.to_string(),
            end,
        };
        Some(self.block(vec![line], read))
    }

    /// Offer a complete line, or a partial a GA or EOR ended, to the
    /// stage. With lines held it first tries to finish a prompt that
    /// spans lines, and when that fails it releases them. Then it reads
    /// the line on its own, or holds it when it starts a prompt that
    /// spans lines. `painted` is the region an earlier read painted the
    /// line in.
    pub fn offer(&mut self, raw: &[u8], plain: &str, painted: Option<u64>, end: End) -> Offered {
        let line = BlockLine {
            raw: raw.to_vec(),
            plain: plain.to_string(),
            end,
        };
        let mut released = Vec::new();
        if !self.held.is_empty() {
            let (read, starts) = {
                let Some(recognizer) = self.recognizer.as_ref() else {
                    return Offered {
                        released: self.release(),
                        offer: Offer::Line,
                    };
                };
                let mut lines: Vec<&str> = self.held.iter().map(|l| l.plain.as_str()).collect();
                lines.push(plain);
                let read = recognizer.read(&lines);
                let starts = read.is_none() && end == End::Line && recognizer.starts(&lines);
                (read, starts)
            };
            if let Some(read) = read {
                let region = self.held_region.take().map(|r| r.gen);
                let mut lines = std::mem::take(&mut self.held);
                lines.push(line);
                return Offered {
                    released,
                    offer: Offer::Prompt(self.block(lines, read), region.or(painted)),
                };
            }
            if starts {
                self.held.push(line);
                self.mark_region_stale(painted);
                return Offered {
                    released,
                    offer: Offer::Held,
                };
            }
            released = self.release();
        }
        let (read, starts) = match self.recognizer.as_ref() {
            Some(recognizer) => {
                let read = recognizer.read(&[plain]);
                let starts = read.is_none() && end == End::Line && recognizer.starts(&[plain]);
                (read, starts)
            }
            None => (None, false),
        };
        let offer = if let Some(read) = read {
            Offer::Prompt(self.block(vec![line], read), painted)
        } else if starts {
            self.held.push(line);
            self.mark_region_stale(painted);
            Offer::Held
        } else {
            Offer::Line
        };
        Offered { released, offer }
    }

    /// Read the partial a read ended on, after any held lines, as your
    /// prompt now, which only a shape that settles does. Returns the
    /// block and the region the held lines show in, which the drawn
    /// prompt replaces.
    pub fn settle(&mut self, raw: &[u8], plain: &str) -> Option<(Block, Option<u64>)> {
        let read = {
            let recognizer = self.recognizer.as_ref()?;
            let mut lines: Vec<&str> = self.held.iter().map(|l| l.plain.as_str()).collect();
            lines.push(plain);
            recognizer.read_partial(&lines)?
        };
        let region = self.held_region.take().map(|r| r.gen);
        let mut lines = std::mem::take(&mut self.held);
        lines.push(BlockLine {
            raw: raw.to_vec(),
            plain: plain.to_string(),
            end: End::Settled,
        });
        Some((self.block(lines, read), region))
    }

    /// True when the partial a read ended on, after any held lines, can
    /// still grow into your prompt. The session then holds it up to
    /// [`HOLD_MS`] before painting it raw, so a prompt a read split never
    /// flashes. Complete lines never wait.
    ///
    /// [`HOLD_MS`]: super::HOLD_MS
    pub fn live(&mut self, plain: &str) -> bool {
        let held: Vec<&str> = self.held.iter().map(|l| l.plain.as_str()).collect();
        self.recognizer
            .as_mut()
            .is_some_and(|recognizer| recognizer.live(&held, plain))
    }

    /// Lines are held for the rest of a prompt that spans lines.
    pub fn holds(&self) -> bool {
        !self.held.is_empty()
    }

    /// Hand back the held lines, the first in the region they show in.
    /// A GA or EOR with no partial after them ends them, since the prompt
    /// they started never came. A send, a local write or the end of the
    /// session lets them go as they show.
    pub fn release(&mut self) -> Vec<Released> {
        let mut region = self.held_region.take().map(|r| r.gen);
        std::mem::take(&mut self.held)
            .into_iter()
            .map(|line| Released {
                raw: line.raw,
                plain: line.plain,
                painted: region.take(),
            })
            .collect()
    }

    /// A held line an earlier read painted part of: its region shows
    /// that part, not the held lines, so the end of the read paints
    /// them again over it.
    fn mark_region_stale(&mut self, painted: Option<u64>) {
        if self.held_region.is_none() {
            if let Some(gen) = painted {
                self.held_region = Some(HeldRegion {
                    gen,
                    lines: usize::MAX,
                    partial: 0,
                });
            }
        }
    }

    /// Paint the held lines at the end of a read, and `partial` after
    /// them when the read ended on one, as one region a later read
    /// replaces. Nothing when the region already shows them. Returns the
    /// region and the partial's length, for the line accumulator.
    pub(super) fn paint_held(&mut self, out: &mut Output, partial: &[u8]) -> (u64, usize) {
        let want = (self.held.len(), partial.len());
        if let Some(region) = self.held_region {
            if (region.lines, region.partial) == want {
                return (region.gen, partial.len());
            }
        }
        // Held lines painted right after a run of repeated lines follow
        // the run, as a partial does, so the prompt they start can leave
        // the run where it is.
        let follows = match self.held_region {
            Some(old) => self.run_partial(out, old.gen),
            None => self.run_open_after(out),
        };
        let gen = self.next_gen();
        let mut bytes = mark(gen);
        for line in &self.held {
            bytes.extend_from_slice(&line.raw);
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(partial);
        match self.held_region {
            Some(old) => out.replace(old.gen, bytes, true),
            None => {
                out.push(&bytes);
                out.closed = true;
            }
        }
        if follows {
            self.run_followed_by(out, gen);
        }
        self.open = None;
        self.unrecorded = true;
        self.held_region = Some(HeldRegion {
            gen,
            lines: want.0,
            partial: want.1,
        });
        (gen, partial.len())
    }

    /// The end of a read that ended on no partial. Held lines paint as a
    /// region the next read replaces when it finishes the prompt.
    pub fn end_read(&mut self, out: &mut Output) {
        self.sync(out);
        if !self.held.is_empty() {
            self.paint_held(out, b"");
        }
    }

    /// A block of `lines` that `read` read, the last line's end as the
    /// shape says: a GA after a prompt that settles leaves the cursor
    /// after it, and a line end after one follows it.
    fn block(&self, mut lines: Vec<BlockLine>, read: Recognized) -> Block {
        if let Some(last) = lines.last_mut() {
            last.end = match last.end {
                End::Marker if read.settle => End::Settled,
                End::Line if read.settle => End::SettledLine,
                end => end,
            };
        }
        Block {
            replaced: self.hides.replaced(lines.len(), &read.lines),
            lines,
            values: read.values,
            afk: read.afk,
            groups: read.lines,
        }
    }
}
