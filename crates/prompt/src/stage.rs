//! The stage: what Vosh writes to the terminal around your prompt
//! (section 4 of the build spec).
//!
//! Everything a socket read writes goes out as one [`Output`], so a prompt
//! that arrives in one read never flashes. The stage decides the bytes
//! around a prompt Vosh reads: the drawn prompt in place of the game's, the
//! game's prompt as sent while drawing is off, and a partial line painted
//! at the end of a read that a later read completes.
//!
//! Anything Vosh may later replace is a region. A region starts with the
//! private mark `ESC ] 7717 ; o ; G BEL`, where G is a generation from
//! [`Stage::next_gen`]. A later output replaces it with [`Replace`], and each
//! renderer finds the region in its own buffer, checks nothing was written
//! after it, and only then erases from its start and writes the new bytes.
//! The stage never counts rows, since the two renderers wrap at different
//! widths and your typed echo reaches the webview before the backend
//! knows of it (D22).
//!
//! The open row is the drawn prompt while it is the last thing on screen.
//! Only it is ever repainted. Any other output, a send, a local write, a
//! window size change and a disconnect close it.
//!
//! The stage also keeps the candidates ring, one entry per send and per GA
//! or EOR, which the prompt card reads to show and check your prompt.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::aabahran::Who;
use crate::capture::{Recognized, Recognizer};
use crate::config::CaptureConfig;
use crate::template::FieldRef;

/// The private OSC Vosh marks regions with.
pub const MARK_OSC: u32 = 7717;

/// How many candidates the ring keeps.
pub const RING: usize = 32;

/// The mark that starts region `gen`, `ESC ] 7717 ; o ; G BEL`.
pub fn mark(gen: u64) -> Vec<u8> {
    format!("\x1b]{MARK_OSC};o;{gen}\x07").into_bytes()
}

/// Replace region `gen` with `bytes`. When the region is still open, the
/// renderer moves to its start, erases to the end of the screen and
/// writes `bytes`. When anything was written after it, a `fresh` replace
/// writes `bytes` on a new row, and any other is dropped. A mark inside
/// `bytes` starts a new region. Without one, no region is open after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replace {
    pub gen: u64,
    pub bytes: Vec<u8>,
    pub fresh: bool,
}

/// What Vosh writes to the terminal for one socket read, or for one
/// repaint. A renderer applies `replace` first, then `bytes`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub replace: Option<Replace>,
    pub bytes: Vec<u8>,
    /// The live render for the region this output leaves open, written
    /// back before anything else lands, when the region shows a preview.
    pub restore: Option<Vec<u8>>,
    /// The open row is no longer the last thing on screen.
    closed: bool,
}

impl Output {
    /// An empty output. `other` says output from elsewhere, such as a
    /// slash command's echo, reached the terminal since the session last
    /// wrote, which closes the open row.
    pub fn new(other: bool) -> Self {
        Self {
            closed: other,
            ..Self::default()
        }
    }

    /// Nothing to write.
    pub fn is_empty(&self) -> bool {
        self.replace.is_none() && self.bytes.is_empty() && self.restore.is_none()
    }

    /// Write bytes that are not a region, such as a line as the Line
    /// pass left it. They close the open row.
    pub fn text(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.bytes.extend_from_slice(bytes);
        self.closed = true;
    }

    /// Write `bytes` as region `gen`, such as the partial a read ended on.
    /// It closes the open row.
    pub fn region(&mut self, gen: u64, bytes: &[u8]) {
        self.bytes.extend(mark(gen));
        self.bytes.extend_from_slice(bytes);
        self.closed = true;
    }

    /// Replace region `gen`, which an earlier output wrote. Before
    /// anything else in this output it rides as [`Output::replace`], and
    /// the renderer decides whether the region is still open. After other
    /// bytes the region is closed, so `fresh` bytes follow on a new row
    /// and anything else is dropped.
    pub fn replace(&mut self, gen: u64, bytes: Vec<u8>, fresh: bool) {
        if self.is_empty() {
            self.replace = Some(Replace { gen, bytes, fresh });
        } else if fresh && !bytes.is_empty() {
            self.new_row();
            self.bytes.extend(bytes);
        } else {
            return;
        }
        self.closed = true;
    }

    /// End the row the cursor sits on, unless this output already left it
    /// at the start of one.
    fn new_row(&mut self) {
        if self.is_empty() || !self.at_row_start() {
            self.bytes.extend_from_slice(b"\r\n");
        }
    }

    /// True when what this output wrote last ends a row. Marks write
    /// nothing, so they do not count.
    fn at_row_start(&self) -> bool {
        let last = if self.bytes.is_empty() {
            self.replace.as_ref().map_or(&[][..], |r| &r.bytes[..])
        } else {
            &self.bytes[..]
        };
        without_trailing_marks(last).ends_with(b"\n")
    }
}

/// `bytes` without the marks at its end.
fn without_trailing_marks(mut bytes: &[u8]) -> &[u8] {
    let prefix = format!("\x1b]{MARK_OSC};o;");
    while bytes.ends_with(b"\x07") {
        let Some(start) = find_last(bytes, prefix.as_bytes()) else {
            break;
        };
        let tail = &bytes[start + prefix.len()..bytes.len() - 1];
        if tail.is_empty() || !tail.iter().all(u8::is_ascii_digit) {
            break;
        }
        bytes = &bytes[..start];
    }
    bytes
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

/// How a line of a recognized prompt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// A line end the prompt waits for, as one ending in `%c` does.
    Line,
    /// A GA or EOR ended the partial.
    Marker,
    /// The partial settled at the end of a read, with nothing after it,
    /// or a GA or EOR came after a prompt that settles.
    Settled,
    /// The prompt settles, so it was whole before the line end that came
    /// after it. The line end starts the game's next row, so it follows
    /// the prompt as it would had the prompt settled at the end of a
    /// read.
    SettledLine,
}

impl End {
    /// What ends the line when it shows as sent. A GA ends the row, as it
    /// always did, unless the prompt settles and so keeps the cursor after
    /// it.
    fn terminator(self) -> &'static [u8] {
        match self {
            End::Line | End::Marker | End::SettledLine => b"\r\n",
            End::Settled => b"",
        }
    }
}

/// One line of a recognized prompt, as the game sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLine {
    pub raw: Vec<u8>,
    pub plain: String,
    pub end: End,
}

/// A prompt the stage read, as the game sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub lines: Vec<BlockLine>,
    /// The lines the drawn prompt replaces, by index. The drawn prompt
    /// always replaces the final line. A line above it shows as the game
    /// sent it unless your design reads a value it carries (D7).
    pub replaced: Vec<usize>,
    /// What each group read, by variable.
    pub values: BTreeMap<String, String>,
    /// The game's away prompt. It shows as sent, even while Vosh draws.
    pub afk: bool,
}

impl Block {
    /// The line Prompts triggers run on.
    pub fn final_line(&self) -> &BlockLine {
        self.lines
            .last()
            .expect("a recognized block has at least one line")
    }

    /// The block as sent, colors included, for `%{raw}`.
    pub fn raw_text(&self) -> String {
        let lines: Vec<String> = self
            .lines
            .iter()
            .map(|l| String::from_utf8_lossy(&l.raw).into_owned())
            .collect();
        lines.join("\r\n")
    }

    /// The lines above the final one that show as the game sent them
    /// while Vosh draws, each with its line end.
    fn heads_shown(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let last = self.lines.len().saturating_sub(1);
        for (index, line) in self.lines.iter().enumerate().take(last) {
            if !self.replaced.contains(&index) {
                out.extend_from_slice(&line.raw);
                out.extend_from_slice(b"\r\n");
            }
        }
        out
    }

    /// Every line above the final one, as the game sent it, each with its
    /// line end, for a block that shows as sent.
    fn heads(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let last = self.lines.len().saturating_sub(1);
        for line in self.lines.iter().take(last) {
            out.extend_from_slice(&line.raw);
            out.extend_from_slice(b"\r\n");
        }
        out
    }

    /// The lines the drawn prompt replaced, as the game showed them, for
    /// drawing off.
    pub fn shown(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for index in &self.replaced {
            if let Some(line) = self.lines.get(*index) {
                out.extend_from_slice(&line.raw);
                out.extend_from_slice(line.end.terminator());
            }
        }
        out
    }
}

/// The drawn prompt while it is the last thing on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRow {
    pub gen: u64,
    /// What the row shows: the drawn prompt, or with drawing off the lines
    /// it replaced.
    pub body: Vec<u8>,
}

/// One entry of the candidates ring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
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
struct HeldRegion {
    gen: u64,
    lines: usize,
    partial: usize,
}

/// Which lines above the last one your design hides: every one when it
/// reads `%{raw}`, else those that carry a value it reads (D7).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Hides {
    all: bool,
    fields: BTreeSet<String>,
}

impl Hides {
    fn of(reads: &BTreeSet<FieldRef>) -> Self {
        Self {
            all: reads.iter().any(|f| f.name == "raw"),
            fields: reads.iter().map(|f| f.name.clone()).collect(),
        }
    }

    /// True when your design reads a value a line with these groups
    /// carries.
    fn line(&self, groups: &[String]) -> bool {
        self.all
            || groups
                .iter()
                .any(|group| fields_of(group).iter().any(|f| self.fields.contains(*f)))
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

/// A line the ring may record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Seen {
    raw: Vec<u8>,
    plain: String,
    recognized: bool,
}

/// What the stage keeps between reads.
#[derive(Debug, Clone, Default)]
pub struct Stage {
    recognizer: Option<Recognizer>,
    /// The last generation handed out. It never starts over, so a region
    /// from an earlier connection never shares a number with a new one.
    gen: u64,
    open: Option<OpenRow>,
    /// The last recognized prompt, as sent.
    last_raw: Option<Block>,
    ring: VecDeque<Candidate>,
    /// Output the ring has not recorded yet came in.
    unrecorded: bool,
    /// The latest recognized prompt since the last ring entry.
    pending: Option<Seen>,
    /// The latest complete line that was not blank.
    last_line: Option<Seen>,
    /// Triggers that hid a prompt this session while nothing read it.
    gag_reported: BTreeSet<String>,
    /// Line triggers that matched a prompt Vosh read this session.
    line_triggers: BTreeSet<String>,
    /// Prompts recognized this session.
    recognized: u64,
    /// Which lines above the last one your design hides.
    hides: Hides,
    /// The top lines of a prompt that spans lines, held until the rest
    /// arrives.
    held: Vec<BlockLine>,
    /// The region the held lines show in, once painted.
    held_region: Option<HeldRegion>,
}

impl Stage {
    /// Compile the capture a profile's `[prompt]` table holds, for a
    /// mortal. See [`Stage::set_capture_for`].
    pub fn set_capture(&mut self, capture: &CaptureConfig) {
        self.set_capture_for(capture, Who::default());
    }

    /// Compile the capture a profile's `[prompt]` table holds. `who`
    /// decides what Aabahran's `%u` and `%s` print.
    pub fn set_capture_for(&mut self, capture: &CaptureConfig, who: Who) {
        self.recognizer = Recognizer::compile_for(capture, who);
    }

    /// The compiled capture.
    pub fn recognizer(&self) -> Option<&Recognizer> {
        self.recognizer.as_ref()
    }

    /// Take the fields your design reads, which decide the lines above
    /// the last one it hides (D7).
    pub fn set_reads(&mut self, reads: &BTreeSet<FieldRef>) {
        self.hides = Hides::of(reads);
    }

    /// Something reads your prompt. Without it Vosh recognizes nothing,
    /// hides nothing and draws nothing.
    pub fn has_recognizer(&self) -> bool {
        self.recognizer.is_some()
    }

    /// A connection opened or closed. The regions, the ring and what the
    /// session noted go. The capture, what the design reads and the
    /// generation count stay.
    pub fn reset(&mut self) {
        *self = Self {
            recognizer: self.recognizer.take(),
            gen: self.gen,
            hides: std::mem::take(&mut self.hides),
            ..Self::default()
        };
    }

    /// A new generation for a region.
    pub fn next_gen(&mut self) -> u64 {
        self.gen += 1;
        self.gen
    }

    /// The open row, when the drawn prompt is the last thing on screen.
    pub fn open_row(&self) -> Option<&OpenRow> {
        self.open.as_ref()
    }

    /// The last recognized prompt, as sent.
    pub fn last_raw(&self) -> Option<&Block> {
        self.last_raw.as_ref()
    }

    /// Close the open row, as a send, a local write, a window size change
    /// or other output does.
    pub fn close(&mut self) {
        self.open = None;
    }

    /// Catch up with `out` before it goes out, so bytes written after
    /// the open row close it. The session calls it at the end of every
    /// read.
    pub fn finish(&mut self, out: &Output) {
        self.sync(out);
    }

    /// Bytes written after the open row close it.
    fn sync(&mut self, out: &Output) {
        if out.closed {
            self.open = None;
        }
    }

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

    /// Lines are held for the rest of a prompt that spans lines.
    pub fn holds(&self) -> bool {
        !self.held.is_empty()
    }

    /// Hand back the held lines, the first in the region they show in.
    /// A GA or EOR with no partial after them ends them, since the prompt
    /// they started never came.
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

    /// Forget the held lines, which stay on screen as painted, as a send,
    /// a local write or other output does. What comes next is read on
    /// its own.
    pub fn forget_held(&mut self) {
        self.held.clear();
        self.held_region = None;
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
    fn paint_held(&mut self, out: &mut Output, partial: &[u8]) -> (u64, usize) {
        let want = (self.held.len(), partial.len());
        if let Some(region) = self.held_region {
            if (region.lines, region.partial) == want {
                return (region.gen, partial.len());
            }
        }
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
                out.bytes.extend(bytes);
                out.closed = true;
            }
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
        let count = lines.len();
        let replaced = (0..count)
            .filter(|&i| {
                i + 1 == count
                    || self
                        .hides
                        .line(read.lines.get(i).map_or(&[][..], |g| &g[..]))
            })
            .collect();
        Block {
            lines,
            replaced,
            values: read.values,
            afk: read.afk,
        }
    }

    /// Draw `rendered` in place of `block`, as the open row with no line
    /// end, so the cursor sits after it like a game's prompt. A prompt
    /// whole before the line end that came after it keeps that line end
    /// in the row. `painted` is the region an earlier read painted the
    /// block's partial as, which the drawn prompt replaces. `before` goes
    /// first, such as lines a Prompts trigger's script echoed.
    pub fn draw(
        &mut self,
        out: &mut Output,
        block: Block,
        painted: Option<u64>,
        before: &[u8],
        rendered: &str,
    ) {
        self.sync(out);
        let gen = self.next_gen();
        let body = drawn(&block, rendered);
        let mut bytes = before.to_vec();
        bytes.extend(block.heads_shown());
        bytes.extend(mark(gen));
        bytes.extend_from_slice(&body);
        put(out, painted, bytes, true);
        out.closed = false;
        self.open = Some(OpenRow { gen, body });
        self.note_recognized(block);
    }

    /// Show `block` as sent, drawing off. `display` is what Prompts
    /// triggers left of its final line, None when one hid it.
    pub fn show(
        &mut self,
        out: &mut Output,
        block: Block,
        painted: Option<u64>,
        before: &[u8],
        display: Option<&[u8]>,
    ) {
        self.sync(out);
        let mut bytes = before.to_vec();
        bytes.extend(block.heads());
        if let Some(display) = display {
            bytes.extend_from_slice(display);
            bytes.extend_from_slice(block.final_line().end.terminator());
        }
        write(out, &mut self.open, painted, bytes);
        self.note_recognized(block);
    }

    /// Write a complete line that is not your prompt, as the Line pass
    /// left it. `painted` is the region an earlier read painted its start
    /// as. `bytes` is empty when a trigger hid the line.
    pub fn line(
        &mut self,
        out: &mut Output,
        raw: &[u8],
        plain: &str,
        painted: Option<u64>,
        bytes: &[u8],
    ) {
        self.sync(out);
        write(out, &mut self.open, painted, bytes.to_vec());
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

    /// Paint the partial a read ended on as a region a later read can
    /// replace. `painted` is the region and length an earlier read painted
    /// it as. A partial that grew is painted again whole. Returns the
    /// region it is now.
    pub fn paint_partial(
        &mut self,
        out: &mut Output,
        raw: &[u8],
        painted: Option<(u64, usize)>,
    ) -> Option<(u64, usize)> {
        self.sync(out);
        self.unrecorded = true;
        if !self.held.is_empty() {
            return Some(self.paint_held(out, raw));
        }
        match painted {
            Some((gen, len)) if len == raw.len() => Some((gen, len)),
            Some((old, _)) => {
                let gen = self.next_gen();
                let mut bytes = mark(gen);
                bytes.extend_from_slice(raw);
                out.replace(old, bytes, true);
                self.open = None;
                Some((gen, raw.len()))
            }
            None if raw.is_empty() => None,
            None => {
                let gen = self.next_gen();
                out.region(gen, raw);
                self.open = None;
                Some((gen, raw.len()))
            }
        }
    }

    /// A GA or EOR ended a partial Vosh does not read as your prompt. It
    /// shows as Prompts triggers left it and the row ends, as a GA always
    /// ended it. `display` is None when a trigger hid it. `painted` is the
    /// region and length an earlier read painted it as. A partial that
    /// grew in the read the GA came in replaces that region whole.
    pub fn end_partial(
        &mut self,
        out: &mut Output,
        raw: &[u8],
        painted: Option<(u64, usize)>,
        before: &[u8],
        display: Option<&[u8]>,
    ) {
        self.sync(out);
        self.unrecorded = true;
        let whole = painted.is_some_and(|(_, len)| len == raw.len());
        if whole && before.is_empty() && display == Some(raw) {
            self.open = None;
            // Already on screen as it is. Only the row ends.
            out.new_row();
            out.closed = true;
            return;
        }
        let painted = painted.map(|(gen, _)| gen);
        let mut bytes = before.to_vec();
        if let Some(display) = display {
            bytes.extend_from_slice(display);
            bytes.extend_from_slice(b"\r\n");
        }
        write(out, &mut self.open, painted, bytes);
    }

    /// Repaint the open row as the `[prompt]` table now says: `rendered`
    /// while drawing is on, else the lines the drawn prompt replaced, as
    /// the game sent them. Nothing when no row is open or it already shows
    /// that. A repaint is dropped by a renderer that wrote anything after
    /// the row.
    pub fn repaint(&mut self, out: &mut Output, rendered: Option<&str>) {
        self.sync(out);
        let Some(open) = &self.open else {
            return;
        };
        let Some(block) = &self.last_raw else {
            return;
        };
        let body = match rendered {
            Some(rendered) => drawn(block, rendered),
            None => block.shown(),
        };
        if body == open.body {
            return;
        }
        let old = open.gen;
        let gen = self.next_gen();
        let mut bytes = mark(gen);
        bytes.extend_from_slice(&body);
        out.replace(old, bytes, false);
        out.closed = false;
        self.open = Some(OpenRow { gen, body });
    }

    fn note_recognized(&mut self, block: Block) {
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
    pub fn record(
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
        self.ring.push_back(Candidate {
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

    /// Note that `trigger` hid a line and set prompt values while nothing
    /// reads your prompt, so Vosh drew nothing in its place. True the
    /// first time this session, when the session tells the webview.
    pub fn gag_without_reader(&mut self, trigger: &str) -> bool {
        self.gag_reported.insert(trigger.to_string())
    }

    /// Note Line triggers that matched a line Vosh read as your prompt,
    /// which they no longer see (D6).
    pub fn line_triggers_matched<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        self.line_triggers
            .extend(names.into_iter().map(str::to_string));
    }

    /// The Line triggers that matched your prompt this session, in name
    /// order. None until Vosh read a prompt this session.
    pub fn line_trigger_notice(&self) -> Option<Vec<String>> {
        (self.recognized > 0).then(|| self.line_triggers.iter().cloned().collect())
    }
}

/// What the open row holds for `block` drawn as `rendered`: the design,
/// and the line end that came after a prompt that was whole before it.
fn drawn(block: &Block, rendered: &str) -> Vec<u8> {
    let mut body = rendered.as_bytes().to_vec();
    if block.final_line().end == End::SettledLine {
        body.extend_from_slice(b"\r\n");
    }
    body
}

/// Write `bytes` over the region `painted`, or as they are.
fn put(out: &mut Output, painted: Option<u64>, bytes: Vec<u8>, fresh: bool) {
    match painted {
        Some(gen) => out.replace(gen, bytes, fresh),
        None => out.text(&bytes),
    }
}

/// Write what is not a region: over the region `painted`, or as it is.
/// It closes the open row, unless it writes nothing at all, as a hidden
/// line that was never painted does.
fn write(out: &mut Output, open: &mut Option<OpenRow>, painted: Option<u64>, bytes: Vec<u8>) {
    if bytes.is_empty() && painted.is_none() {
        return;
    }
    let fresh = !bytes.is_empty();
    put(out, painted, bytes, fresh);
    *open = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RegexCapture;

    /// The capture the migration writes for James, unanchored.
    const JAMES: &str = r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";
    const PROMPT: &str = "[1020/1020hp 800/800mn 930/930mv]";
    /// A prompt with no line end, read by a capture that settles.
    const SETTLES: &str = r"^<(?<hp>\d+)hp> $";

    fn stage(pattern: &str, settle: bool) -> Stage {
        let mut stage = Stage::default();
        stage.set_capture(&CaptureConfig::Regex(RegexCapture {
            lines: vec![pattern.to_string()],
            settle,
            ..RegexCapture::default()
        }));
        stage
    }

    fn with(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    fn read(stage: &Stage, text: &str, end: End) -> Block {
        stage
            .recognize(text.as_bytes(), text, end)
            .expect("the prompt")
    }

    #[test]
    fn a_mark_is_a_private_osc_with_the_generation() {
        assert_eq!(mark(7), b"\x1b]7717;o;7\x07");
        assert_eq!(mark(12_345), b"\x1b]7717;o;12345\x07");
    }

    #[test]
    fn a_line_prompt_draws_as_the_open_row_with_no_line_end() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        out.text(b"You are hungry.\r\n");
        let block = read(&stage, PROMPT, End::Line);
        assert_eq!(block.values["hp"], "1020");
        stage.draw(&mut out, block, None, b"", "DRAWN");
        assert_eq!(
            out.bytes,
            with(&[b"You are hungry.\r\n", &mark(1), b"DRAWN"])
        );
        assert_eq!(out.replace, None);
        assert_eq!(
            stage.open_row(),
            Some(&OpenRow {
                gen: 1,
                body: b"DRAWN".to_vec()
            })
        );
        assert_eq!(
            stage.last_raw().map(Block::shown),
            Some(b"[1020/1020hp 800/800mn 930/930mv]\r\n".to_vec())
        );
    }

    #[test]
    fn with_drawing_off_the_prompt_shows_as_sent() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        let colored = "\x1b[37m[1020/1020hp 800/800mn 930/930mv]\x1b[0m";
        let block = stage
            .recognize(colored.as_bytes(), PROMPT, End::Line)
            .expect("the prompt");
        stage.show(&mut out, block, None, b"", Some(colored.as_bytes()));
        assert_eq!(out.bytes, with(&[colored.as_bytes(), b"\r\n"]));
        assert_eq!(stage.open_row(), None);
        // A Prompts trigger that hides it leaves nothing.
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.show(&mut out, block, None, b"", None);
        assert!(out.is_empty());
    }

    #[test]
    fn echoes_land_where_the_prompt_was_before_the_drawn_prompt() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"low on mana\r\n", "DRAWN");
        assert_eq!(out.bytes, with(&[b"low on mana\r\n", &mark(1), b"DRAWN"]));
        assert_eq!(stage.open_row().map(|r| r.gen), Some(1));
    }

    #[test]
    fn a_prompt_split_across_reads_replaces_the_painted_start() {
        let mut stage = stage(JAMES, false);
        // The first read ends partway through the prompt.
        let mut first = Output::new(false);
        let painted = stage.paint_partial(&mut first, b"[1020/1020hp 800", None);
        assert_eq!(painted, Some((1, 16)));
        assert_eq!(first.bytes, with(&[&mark(1), b"[1020/1020hp 800"]));
        // The next read completes it, and the drawn prompt replaces the
        // painted start in the same payload.
        let mut second = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut second, block, Some(1), b"", "DRAWN");
        assert_eq!(
            second.replace,
            Some(Replace {
                gen: 1,
                bytes: with(&[&mark(2), b"DRAWN"]),
                fresh: true,
            })
        );
        assert!(second.bytes.is_empty());
        assert_eq!(stage.open_row().map(|r| r.gen), Some(2));
    }

    #[test]
    fn a_painted_start_after_other_output_draws_on_a_new_row() {
        let mut stage = stage(JAMES, false);
        let mut first = Output::new(false);
        let painted = stage.paint_partial(&mut first, b"[1020", None);
        assert_eq!(painted, Some((1, 5)));

        // A GMCP handler's echo came first in the next read.
        let mut second = Output::new(false);
        second.text(b"\r\nThe moon rises.\r\n");
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut second, block, Some(1), b"", "DRAWN");
        assert_eq!(second.replace, None);
        assert_eq!(
            second.bytes,
            with(&[b"\r\nThe moon rises.\r\n", &mark(2), b"DRAWN"])
        );

        // Text that leaves the cursor mid row gets a line end first.
        let mut third = Output::new(false);
        third.text(b"mid row");
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut third, block, Some(9), b"", "DRAWN");
        assert_eq!(third.bytes, with(&[b"mid row\r\n", &mark(3), b"DRAWN"]));
    }

    #[test]
    fn a_line_completing_a_painted_partial_replaces_it() {
        let mut stage = stage(JAMES, false);
        let mut first = Output::new(false);
        let _ = stage.paint_partial(&mut first, b"You are hun", None);
        let mut second = Output::new(false);
        stage.line(
            &mut second,
            b"You are hungry.",
            "You are hungry.",
            Some(1),
            b"You are hungry.\r\n",
        );
        stage.line(&mut second, b"next", "next", None, b"next\r\n");
        assert_eq!(
            second.replace,
            Some(Replace {
                gen: 1,
                bytes: b"You are hungry.\r\n".to_vec(),
                fresh: true,
            })
        );
        assert_eq!(second.bytes, b"next\r\n");

        // A trigger that hides the completed line erases the painted
        // start, and a renderer that wrote after it drops the erase.
        let mut third = Output::new(false);
        let _ = stage.paint_partial(&mut third, b"spam", None);
        let mut fourth = Output::new(false);
        stage.line(&mut fourth, b"spam spam", "spam spam", Some(2), b"");
        assert_eq!(
            fourth.replace,
            Some(Replace {
                gen: 2,
                bytes: Vec::new(),
                fresh: false,
            })
        );
    }

    #[test]
    fn a_growing_partial_is_painted_again_whole() {
        let mut stage = stage(JAMES, false);
        let mut first = Output::new(false);
        let painted = stage.paint_partial(&mut first, b"<10", None);
        let mut second = Output::new(false);
        let painted = stage.paint_partial(&mut second, b"<10hp> ", painted);
        assert_eq!(painted, Some((2, 7)));
        assert_eq!(
            second.replace,
            Some(Replace {
                gen: 1,
                bytes: with(&[&mark(2), b"<10hp> "]),
                fresh: true,
            })
        );
        // A read that adds nothing to it writes nothing.
        let mut third = Output::new(false);
        assert_eq!(
            stage.paint_partial(&mut third, b"<10hp> ", painted),
            painted
        );
        assert!(third.is_empty());
        // An empty partial paints nothing.
        assert_eq!(stage.paint_partial(&mut third, b"", None), None);
        assert!(third.is_empty());
    }

    #[test]
    fn a_partial_that_settles_is_the_prompt_at_once() {
        let mut stage = stage(SETTLES, true);
        assert!(stage.recognize(b"<10hp>", "<10hp>", End::Settled).is_none());
        let block = read(&stage, "<10hp> ", End::Settled);
        let mut out = Output::new(false);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        assert_eq!(out.bytes, with(&[&mark(1), b"DRAWN"]));
        // Shown as sent, a settled prompt keeps the cursor after it.
        let mut out = Output::new(false);
        let block = read(&stage, "<10hp> ", End::Settled);
        stage.show(&mut out, block, None, b"", Some(b"<10hp> "));
        assert_eq!(out.bytes, b"<10hp> ");
        // A capture that waits never reads a partial.
        let waits = self::stage(JAMES, false);
        assert!(waits
            .recognize(PROMPT.as_bytes(), PROMPT, End::Settled)
            .is_none());
    }

    #[test]
    fn a_prompt_whole_before_its_line_end_draws_the_line_end_after_it() {
        let mut stage = stage(SETTLES, true);
        let block = read(&stage, "<10hp> ", End::Line);
        assert_eq!(block.final_line().end, End::SettledLine);
        let mut out = Output::new(false);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        assert_eq!(out.bytes, with(&[&mark(1), b"DRAWN\r\n"]));
        // The row stays open with its line end, and a repaint keeps it.
        assert_eq!(
            stage.open_row(),
            Some(&OpenRow {
                gen: 1,
                body: b"DRAWN\r\n".to_vec()
            })
        );
        let mut new = Output::new(false);
        stage.repaint(&mut new, Some("NEW"));
        assert_eq!(
            new.replace,
            Some(Replace {
                gen: 1,
                bytes: with(&[&mark(2), b"NEW\r\n"]),
                fresh: false,
            })
        );
        let mut off = Output::new(false);
        stage.repaint(&mut off, None);
        assert_eq!(
            off.replace.map(|r| r.bytes),
            Some(with(&[&mark(3), b"<10hp> \r\n"]))
        );
        // A capture that waits for its line end reads the line end as
        // part of the prompt, so the drawn prompt keeps the cursor.
        let mut waits = self::stage(JAMES, false);
        let block = read(&waits, PROMPT, End::Line);
        assert_eq!(block.final_line().end, End::Line);
        let mut out = Output::new(false);
        waits.draw(&mut out, block, None, b"", "DRAWN");
        assert_eq!(out.bytes, with(&[&mark(1), b"DRAWN"]));
    }

    #[test]
    fn a_ga_after_a_prompt_that_settles_leaves_the_cursor_after_it() {
        let mut stage = stage(SETTLES, true);
        let block = read(&stage, "<10hp> ", End::Marker);
        assert_eq!(block.final_line().end, End::Settled);
        let mut out = Output::new(false);
        stage.show(&mut out, block, None, b"", Some(b"<10hp> "));
        assert_eq!(out.bytes, b"<10hp> ");
    }

    #[test]
    fn a_ga_ends_a_prompt_in_the_same_read_or_the_next() {
        let mut stage = stage(JAMES, false);
        // The same read: the partial never painted, so nothing flashes.
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Marker);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        assert_eq!(out.bytes, with(&[&mark(1), b"DRAWN"]));

        // The next read: the end of the first painted it, and the GA
        // replaces it.
        let mut first = Output::new(false);
        let painted = stage.paint_partial(&mut first, PROMPT.as_bytes(), None);
        assert_eq!(painted, Some((2, PROMPT.len())));
        let mut second = Output::new(false);
        let block = read(&stage, PROMPT, End::Marker);
        stage.draw(&mut second, block, Some(2), b"", "DRAWN");
        assert_eq!(
            second.replace,
            Some(Replace {
                gen: 2,
                bytes: with(&[&mark(3), b"DRAWN"]),
                fresh: true,
            })
        );

        // Drawing off, a GA ends the row after the prompt.
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Marker);
        stage.show(&mut out, block, None, b"", Some(PROMPT.as_bytes()));
        assert_eq!(out.bytes, with(&[PROMPT.as_bytes(), b"\r\n"]));
    }

    #[test]
    fn a_ga_on_a_partial_vosh_does_not_read_ends_the_row() {
        let mut stage = stage(JAMES, false);
        // Same read, never painted.
        let mut out = Output::new(false);
        stage.end_partial(&mut out, b"[Hit Return]", None, b"", Some(b"[Hit Return]"));
        assert_eq!(out.bytes, b"[Hit Return]\r\n");
        // Painted as it is, so only the row ends.
        let mut out = Output::new(false);
        stage.end_partial(&mut out, b"> ", Some((4, 2)), b"", Some(b"> "));
        assert_eq!(out.bytes, b"\r\n");
        assert_eq!(out.replace, None);
        // It grew in the read the GA came in, so the painted start is
        // replaced by the whole of it.
        let mut out = Output::new(false);
        stage.end_partial(
            &mut out,
            b"<100hp 50m 30mv> ",
            Some((4, 9)),
            b"",
            Some(b"<100hp 50m 30mv> "),
        );
        assert_eq!(
            out.replace,
            Some(Replace {
                gen: 4,
                bytes: b"<100hp 50m 30mv> \r\n".to_vec(),
                fresh: true,
            })
        );
        assert!(out.bytes.is_empty());
        // A Prompts trigger changed it, so it replaces the painted one.
        let mut out = Output::new(false);
        stage.end_partial(
            &mut out,
            b"> ",
            Some((4, 2)),
            b"",
            Some(b"\x1b[31m> \x1b[0m"),
        );
        assert_eq!(
            out.replace,
            Some(Replace {
                gen: 4,
                bytes: b"\x1b[31m> \x1b[0m\r\n".to_vec(),
                fresh: true,
            })
        );
        // A trigger hid it: the painted one is erased, and an unpainted
        // one writes nothing.
        let mut out = Output::new(false);
        stage.end_partial(&mut out, b"> ", Some((4, 2)), b"", None);
        assert_eq!(
            out.replace,
            Some(Replace {
                gen: 4,
                bytes: Vec::new(),
                fresh: false,
            })
        );
        let mut out = Output::new(false);
        stage.end_partial(&mut out, b"> ", None, b"", None);
        assert!(out.is_empty());
    }

    #[test]
    fn draw_off_repaints_the_open_row_as_the_game_sent_it() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");

        let mut off = Output::new(false);
        stage.repaint(&mut off, None);
        assert_eq!(
            off.replace,
            Some(Replace {
                gen: 1,
                bytes: with(&[&mark(2), PROMPT.as_bytes(), b"\r\n"]),
                fresh: false,
            })
        );
        assert!(off.bytes.is_empty());

        // Drawing back on paints the design again over the same row.
        let mut on = Output::new(false);
        stage.repaint(&mut on, Some("DRAWN"));
        assert_eq!(
            on.replace,
            Some(Replace {
                gen: 2,
                bytes: with(&[&mark(3), b"DRAWN"]),
                fresh: false,
            })
        );
        // A repaint that changes nothing writes nothing.
        let mut same = Output::new(false);
        stage.repaint(&mut same, Some("DRAWN"));
        assert!(same.is_empty());
        let mut new = Output::new(false);
        stage.repaint(&mut new, Some("NEW DESIGN"));
        assert_eq!(new.replace.map(|r| r.gen), Some(3));
    }

    #[test]
    fn the_open_row_closes_on_output_a_send_and_other_output() {
        let block_of = |stage: &Stage| read(stage, PROMPT, End::Line);

        // Output after it in the same read.
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
        out.text(b"You flee!\r\n");
        stage.finish(&out);
        let mut later = Output::new(false);
        stage.repaint(&mut later, None);
        assert!(later.is_empty());
        assert_eq!(stage.open_row(), None);

        // A send, a local write or a window size change.
        let mut out = Output::new(false);
        stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
        stage.close();
        let mut later = Output::new(false);
        stage.repaint(&mut later, None);
        assert!(later.is_empty());

        // Output from elsewhere, such as a slash command's echo.
        let mut out = Output::new(false);
        stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
        let mut later = Output::new(true);
        stage.repaint(&mut later, None);
        assert!(later.is_empty());

        // A partial painted after it.
        let mut out = Output::new(false);
        stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
        let _ = stage.paint_partial(&mut out, b"more", None);
        assert_eq!(stage.open_row(), None);

        // A hidden line writes nothing, so the row stays open.
        let mut out = Output::new(false);
        stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
        stage.line(&mut out, b"spam", "spam", None, b"");
        stage.finish(&out);
        assert!(stage.open_row().is_some());

        // A prompt drawn after other output in the same read stays open.
        let mut out = Output::new(true);
        out.text(b"text\r\n");
        stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
        stage.finish(&out);
        assert!(stage.open_row().is_some());
    }

    #[test]
    fn a_reset_forgets_the_session_and_keeps_counting() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        stage.record(None, 1, true, true);
        assert!(stage.gag_without_reader("capture"));
        stage.reset();
        assert!(stage.has_recognizer());
        assert_eq!(stage.open_row(), None);
        assert_eq!(stage.last_raw(), None);
        assert_eq!(stage.ring().count(), 0);
        assert_eq!(stage.line_trigger_notice(), None);
        assert!(stage.gag_without_reader("capture"));
        assert_eq!(stage.next_gen(), 2);
    }

    #[test]
    fn the_ring_records_one_entry_per_send_and_ga() {
        let mut stage = stage(JAMES, false);
        stage.record(None, 1, true, true);
        assert_eq!(stage.ring().count(), 0, "nothing came in yet");

        // A drawn prompt is recorded as the game sent it, before the gag.
        let mut out = Output::new(false);
        let colored = "\x1b[37m[1020/1020hp 800/800mn 930/930mv]";
        let block = stage
            .recognize(colored.as_bytes(), PROMPT, End::Line)
            .expect("the prompt");
        stage.draw(&mut out, block, None, b"", "DRAWN");
        stage.record(None, 10, true, true);
        // Nothing new came in, so a second send records nothing.
        stage.record(None, 11, true, true);

        // A line that is not a prompt, then a partial at a send.
        stage.line(
            &mut out,
            b"You are hungry.",
            "You are hungry.",
            None,
            b"You are hungry.\r\n",
        );
        stage.record(Some((b"<10hp> ", "<10hp> ")), 20, false, true);
        // A complete line, with no prompt and no partial.
        stage.line(&mut out, b"", "", None, b"\r\n");
        stage.line(&mut out, b"Healer> ", "Healer> ", None, b"Healer> \r\n");
        stage.line(&mut out, b"  ", "  ", None, b"  \r\n");
        stage.record(Some((b"", "")), 30, false, false);

        let ring: Vec<&Candidate> = stage.ring().collect();
        assert_eq!(
            ring[0],
            &Candidate {
                raw: colored.as_bytes().to_vec(),
                plain: PROMPT.to_string(),
                at_ms: 10,
                recognized: true,
                draw: true,
                capture: true,
            }
        );
        assert_eq!(
            ring[1],
            &Candidate {
                raw: b"<10hp> ".to_vec(),
                plain: "<10hp> ".to_string(),
                at_ms: 20,
                recognized: false,
                draw: false,
                capture: true,
            }
        );
        assert_eq!(
            ring[2],
            &Candidate {
                raw: b"Healer> ".to_vec(),
                plain: "Healer> ".to_string(),
                at_ms: 30,
                recognized: false,
                draw: false,
                capture: false,
            }
        );
        assert_eq!(ring.len(), 3);
    }

    #[test]
    fn the_ring_keeps_the_newest_thirty_two() {
        let mut stage = Stage::default();
        let mut out = Output::new(false);
        for i in 0..40_i64 {
            let text = format!("line {i}");
            stage.line(&mut out, text.as_bytes(), &text, None, b"");
            stage.record(None, i, false, false);
        }
        let ring: Vec<i64> = stage.ring().map(|c| c.at_ms).collect();
        assert_eq!(ring.len(), RING);
        assert_eq!(ring.first(), Some(&8));
        assert_eq!(ring.last(), Some(&39));
    }

    #[test]
    fn a_gag_with_no_reader_is_told_once_per_trigger() {
        let mut stage = Stage::default();
        assert!(!stage.has_recognizer());
        assert!(stage.gag_without_reader("prompt-capture"));
        assert!(!stage.gag_without_reader("prompt-capture"));
        assert!(stage.gag_without_reader("my-capture"));
    }

    #[test]
    fn line_triggers_that_matched_a_prompt_are_named_after_one_was_read() {
        let mut stage = stage(JAMES, false);
        stage.line_triggers_matched(["hp-watch"]);
        assert_eq!(stage.line_trigger_notice(), None, "no prompt read yet");
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        stage.line_triggers_matched(["bracket", "hp-watch"]);
        assert_eq!(
            stage.line_trigger_notice(),
            Some(vec!["bracket".to_string(), "hp-watch".to_string()])
        );
    }

    #[test]
    fn a_block_keeps_its_raw_text_for_the_raw_piece() {
        let stage = stage(JAMES, false);
        let colored = "\x1b[37m[1020/1020hp 800/800mn 930/930mv]\x1b[0m";
        let block = stage
            .recognize(colored.as_bytes(), PROMPT, End::Line)
            .expect("the prompt");
        assert_eq!(block.raw_text(), colored);
        assert_eq!(block.final_line().plain, PROMPT);
    }
}
