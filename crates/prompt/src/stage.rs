//! The stage: what Vosh writes to the terminal around your prompt.
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
//! knows of it.
//!
//! The open row is the drawn prompt while it is the last thing on screen.
//! Only it is ever repainted. Any other output, a send, a local write, a
//! window size change and a disconnect close it.
//!
//! A prompt may span lines, as the tank line above your vitals does in
//! a fight. The stage holds a line that starts one
//! until the rest arrives, within the read, and paints held lines as a
//! region at the end of a read, so the prompt that finishes them replaces
//! it. A line that does not finish them releases them to the Line pass,
//! and so do a send, a local write and the end of the session, which
//! leave them as they show.
//! A line above the last one shows as sent unless the design reads a
//! value it carries. A partial that can still become your prompt waits up
//! to [`HOLD_MS`] for the next read before it paints raw.
//!
//! The stage also keeps the candidates ring, one entry per send and per GA
//! or EOR, which the prompt card reads to show and check your prompt.
//!
//! Where your prompt shows (`[prompt] show`) decides the rest. In the
//! text, the stage writes as described above. Pinned, a recognized prompt
//! leaves the text: the output carries it in [`Output::pin`] for the band
//! above the command line, the line ends before it wait in
//! [`Output::hold`] until the next write lands, and the empty line your
//! next unasked text starts with writes nothing, so the text keeps the
//! rows it would have kept minus the prompt's own. Lifted, each prompt
//! stays in the text between two more private marks, `ESC ] 7717 ; l ; L
//! BEL` before its first line and `ESC ] 7717 ; e ; L BEL` after its last
//! visible byte, so each renderer can draw a band under it.
//!
//! While Collapse repeated lines is on, [`Stage::repeat_line`] writes each
//! line the Line pass left as a region of its own. A line that shows the
//! same bytes as the run the text ends on joins it: the run's region is
//! written again in place with the count before the line, `(3) `, the way
//! a repaint rewrites the open row, so both renderers show the run once.
//! Anything else written after the run ends it, and so do your echo,
//! output from elsewhere and a new connection. A pinned prompt leaves the
//! text, so it ends nothing, and neither does a hidden line. The stage
//! follows the colors the text carries from line to line meanwhile, so
//! the run is written again from the colors it started in (see
//! [`counted`]). A rewrite leaves out the line end each renderer still
//! holds back after the run, and carries it as [`Replace::tail`].
//! [`CollapseRules`] says which lines the session offers it: the lines of
//! a fight unless you show every one, and attack lines only when you
//! collapse them too.

mod blocks;
mod drawing;
mod marks;
mod output;
mod pinning;
mod reading;
mod repeats;
mod ring;

pub use blocks::{Block, BlockLine, End, View};
pub use marks::{lift_end, lift_start, mark, MARK_OSC};
pub use output::{close_pin_row, shows_lines, Above, Output, Replace};
pub use reading::{Offer, Released};
pub use repeats::{collapsible, counted, CollapseRules, Repeat};
pub use ring::Candidate;

pub(crate) use blocks::OpenRow;
pub(crate) use output::plain_text;

use std::collections::{BTreeSet, VecDeque};

use crate::aabahran::Who;
use crate::capture::Recognizer;
use crate::config::{CaptureConfig, PromptShow};
use crate::design::{FieldRef, Template, TokenKind};
use crate::render::{SgrState, Span};

use blocks::OpenLift;
use pinning::{PinnedShown, Swallow};
use reading::{HeldRegion, Hides};
use repeats::Run;
use ring::Seen;

/// How many candidates the ring keeps.
pub(crate) const RING: usize = 32;

/// How long a partial that can still become your prompt waits for the
/// next read before it paints raw, in milliseconds.
pub const HOLD_MS: u64 = 20;

/// The most rows the band above the command line keeps for your prompt.
pub(crate) const ZONE_MAX: usize = 6;

/// What the stage keeps between reads.
#[derive(Debug, Clone, Default)]
pub struct Stage {
    recognizer: Option<Recognizer>,
    /// The last generation handed out. It never starts over, so a region
    /// from an earlier connection never shares a number with a new one.
    gen: u64,
    open: Option<OpenRow>,
    /// The output that first wrote the open row's region. A repaint
    /// keeps it, since a renderer drops a repaint of a region that text
    /// landed after, so text that landed after this output closes the
    /// row.
    open_since: u64,
    /// The newest output the stage finished with something to write,
    /// which the session sent.
    sent: u64,
    /// The last recognized prompt, as sent.
    last_raw: Option<Block>,
    ring: VecDeque<Candidate>,
    /// The last candidate id handed out. It never starts over, as the
    /// generation does not.
    candidates: u64,
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
    /// Where your prompt shows, from the `[prompt]` table.
    show: PromptShow,
    /// Where the latest prompt went, so a change to `show` knows what
    /// to move.
    shown_as: PromptShow,
    /// What the band shows while your prompt shows pinned.
    pinned: Option<Vec<u8>>,
    /// Where each piece of the design landed on the band, as the band
    /// last went out.
    pinned_spans: Option<Vec<Span>>,
    /// A prompt pinned with drawing off, as Prompts triggers left it: what
    /// the band shows for it, and what the text would have shown, so a
    /// repaint of the band and a move back to the text keep what the
    /// triggers did.
    pinned_shown: Option<PinnedShown>,
    /// The next empty line writes nothing, since the pinned prompt's row
    /// it would have ended is not in the text.
    swallow: Option<Swallow>,
    /// The lift the open row carries, set with every open row.
    open_lift: Option<OpenLift>,
    /// The lines the open row's prompt shows as sent right above its
    /// region, and their plain text, set with every open row.
    open_heads: Option<(Vec<u8>, String)>,
    /// The prompt card is open, so in the text the row that draws your
    /// design borrows the band of Lifted (the 2026-09-30 addendum, item
    /// 2).
    card: bool,
    /// The run of repeated lines the text ends on, while Collapse repeated
    /// lines is on.
    run: Option<Run>,
    /// Collapse repeated lines is on, so the stage follows the colors the
    /// text carries from line to line.
    collapse: bool,
    /// The colors the text carries into the next line, as the lines the
    /// stage wrote left them, while Collapse repeated lines is on.
    carry: SgrState,
}

impl Stage {
    /// Compile the capture a profile's `[prompt]` table holds, for a
    /// mortal. See `Stage::set_capture_for`. Test only. The app's tests
    /// reach it through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn set_capture(&mut self, capture: &CaptureConfig) {
        self.set_capture_for(capture, Who::default());
    }

    /// Compile the capture a profile's `[prompt]` table holds. `who`
    /// decides what Aabahran's `%u` and `%s` print.
    pub(crate) fn set_capture_for(&mut self, capture: &CaptureConfig, who: Who) {
        self.recognizer = Recognizer::compile_for(capture, who);
        // Once something reads your prompt, no trigger hides it with
        // nothing drawn in its place.
        if self.recognizer.is_some() {
            self.forget_gags_without_reader();
        }
    }

    /// The compiled capture.
    pub(crate) fn recognizer(&self) -> Option<&Recognizer> {
        self.recognizer.as_ref()
    }

    /// Take the fields your design reads, which decide the lines above
    /// the last one it hides. The last prompt read follows at once,
    /// so a repaint after an edit shows a line above the last one as sent
    /// exactly when the new design leaves it alone.
    pub(crate) fn set_reads(&mut self, reads: &BTreeSet<FieldRef>) {
        self.hides = Hides::of(reads);
        if let Some(block) = self.last_raw.as_mut() {
            if block.groups.len() == block.lines.len() {
                block.replaced = self.hides.replaced(block.lines.len(), &block.groups);
            }
        }
    }

    /// Something reads your prompt. Without it Vosh recognizes nothing,
    /// hides nothing and draws nothing.
    pub fn has_recognizer(&self) -> bool {
        self.recognizer.is_some()
    }

    /// A connection opened or closed. The regions, the ring and what the
    /// session noted go. The capture, what the design reads, the
    /// generation count and the candidate count stay.
    pub(crate) fn reset(&mut self) {
        *self = Self {
            recognizer: self.recognizer.take(),
            gen: self.gen,
            candidates: self.candidates,
            hides: std::mem::take(&mut self.hides),
            show: self.show,
            shown_as: self.show,
            card: self.card,
            collapse: self.collapse,
            ..Self::default()
        };
    }

    /// The prompt card opened or closed. While it is open, the row that
    /// draws your design in the text carries the lift marks of Lifted, so
    /// the page can draw the card's edit band under it with the band pass.
    /// The game's own lines, while the card reads your codes or drawing is
    /// off, carry none. A row keeps its marks when the card closes, as it
    /// does when you leave Lifted, and the page turns the band pass off.
    pub fn set_card(&mut self, open: bool) {
        self.card = open;
    }

    /// The prompt card is open. Test only.
    #[cfg(test)]
    pub(crate) fn card_open(&self) -> bool {
        self.card
    }

    /// Take where your prompt shows from the `[prompt]` table. The prompt
    /// on screen moves at the next repaint.
    pub(crate) fn set_show(&mut self, show: PromptShow) {
        self.show = show;
    }

    /// What the band shows, while your prompt shows pinned. Test only.
    #[cfg(test)]
    pub(crate) fn pinned(&self) -> Option<&[u8]> {
        self.pinned.as_deref()
    }

    /// The next empty line writes nothing. Test only. The app's tests
    /// reach it through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn swallows(&self) -> bool {
        self.swallow.is_some()
    }

    /// The most rows the band above the command line can show for any
    /// prompt this capture reads, so the band keeps one height while the
    /// capture, the design and the switch stay the same. Drawing, a way
    /// the game prints your prompt takes the lines above its last that
    /// show as sent, plus a row for the design and one for every line
    /// break in it, conditions or not. A design that reads the whole
    /// prompt as sent takes that prompt's lines in its place. Not
    /// drawing, and for the away prompt, it takes its own lines. At least
    /// 1 and at most [`ZONE_MAX`].
    pub(crate) fn zone(&self, draw: bool, template: &Template) -> usize {
        let Some(recognizer) = &self.recognizer else {
            return 1;
        };
        let breaks = template
            .tokens()
            .iter()
            .filter(|t| t.kind == TokenKind::Nl)
            .count();
        let most = recognizer
            .shapes()
            .iter()
            .map(|(lines, afk)| {
                if !draw || *afk {
                    return lines.len();
                }
                if self.hides.all {
                    return breaks + lines.len();
                }
                let last = lines.len().saturating_sub(1);
                let heads = lines[..last]
                    .iter()
                    .filter(|groups| !self.hides.line(groups))
                    .count();
                heads + 1 + breaks
            })
            .max()
            .unwrap_or(1);
        most.clamp(1, ZONE_MAX)
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
    /// or other output does. A pinned prompt left no row, so the line end
    /// it would have taken still writes nothing.
    pub fn close(&mut self) {
        self.open = None;
    }

    /// The webview wrote to the terminal itself, such as your typed echo,
    /// on a renderer whose newest output was `after` (see [`Output::id`]).
    /// It closes the open row and lands where a pinned prompt's row would
    /// have been, so the next empty line writes again. Your echo reaches
    /// the session by a call apart from your line, so the session can
    /// hear of it after it sent the reply. A row or a pinned prompt that
    /// later output brought came after the text, so it stays.
    pub fn local_write(&mut self, after: u64) {
        if self.open_since <= after {
            self.open = None;
        }
        if self.swallow.is_some_and(|swallow| swallow.output <= after) {
            self.swallow = None;
        }
        // The text landed after the run of repeated lines, so the next
        // line starts a run of its own. A run a later output wrote came
        // after the text, so it stays.
        if self.run.as_ref().is_some_and(|run| run.output <= after) {
            self.run = None;
        }
    }

    /// True when the stage finished output newer than `after` with
    /// something to write, so text a renderer wrote after it took `after`
    /// landed before that output.
    pub fn wrote_after(&self, after: u64) -> bool {
        self.sent > after
    }

    /// Catch up with `out` before it goes out, so bytes written after
    /// the open row close it, and say whether a pinned prompt's row is
    /// still open after it. The session calls it at the end of every
    /// read.
    pub fn finish(&mut self, out: &mut Output) {
        self.sync(out);
        self.seal(out);
        self.note_sent(out);
    }

    /// `out` goes out next when it has anything to write.
    fn note_sent(&mut self, out: &Output) {
        self.settle_run(out);
        if !out.is_empty() {
            self.sent = self.sent.max(out.id.0);
        }
    }

    /// While your prompt shows pinned, or leaves Pinned, tell the
    /// renderers whether the pinned prompt's row is open after `out`.
    fn seal(&self, out: &mut Output) {
        if self.show == PromptShow::Pinned
            || self.shown_as == PromptShow::Pinned
            || out.pin.is_some()
        {
            out.pin_row = Some(self.swallow.is_some());
        }
    }

    /// Bytes written after the open row close it. Output from elsewhere,
    /// or anything that showed after a pinned prompt, means the next
    /// empty line writes again.
    fn sync(&mut self, out: &Output) {
        if out.closed {
            self.open = None;
        }
        if self.swallow.is_some_and(|swallow| swallow.ended_by(out)) {
            self.swallow = None;
        }
    }

    /// Note that `trigger` hid a line and set prompt values while nothing
    /// reads your prompt, so Vosh drew nothing in its place. True the
    /// first time this session, when the session tells the webview.
    pub fn gag_without_reader(&mut self, trigger: &str) -> bool {
        self.gag_reported.insert(trigger.to_string())
    }

    /// Start the list of triggers that hid a prompt with nothing reading
    /// it over, as a profile that reads one or another profile does.
    pub(crate) fn forget_gags_without_reader(&mut self) {
        self.gag_reported.clear();
    }

    /// The triggers that hid a prompt this session while nothing read
    /// it, in name order.
    pub fn gags_without_reader(&self) -> impl Iterator<Item = &str> {
        self.gag_reported.iter().map(String::as_str)
    }

    /// Note Line triggers that matched a line Vosh read as your prompt,
    /// which they no longer see.
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

#[cfg(test)]
mod tests;
