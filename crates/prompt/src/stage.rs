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
//! A prompt may span lines (D7). The stage holds a line that starts one
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

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::aabahran::Who;
use crate::capture::{Recognized, Recognizer};
use crate::config::{CaptureConfig, PromptShow};
use crate::render::{Color, SgrState, Span};
use crate::template::{FieldRef, Template, TokenKind};

/// The private OSC Vosh marks regions with.
pub const MARK_OSC: u32 = 7717;

/// How many candidates the ring keeps.
pub const RING: usize = 32;

/// How long a partial that can still become your prompt waits for the
/// next read before it paints raw, in milliseconds.
pub const HOLD_MS: u64 = 20;

/// The most rows the band above the command line keeps for your prompt.
pub const ZONE_MAX: usize = 6;

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

/// `line` as a run of `count` repeated lines shows it, when the text
/// carries the colors `carry` into the line: the line alone for one, and
/// from two on the count before it, `(N) `. The count draws in the gray of
/// 256 color 244, which the restored scrollback banner draws in too, since
/// dim draws in a shade of its own on each renderer.
///
/// The run is written again over its own region, so it starts from the
/// colors `carry` names whatever the renderer was left in. The count goes
/// after the colors the line opens with and keeps their background, so a
/// washed line still starts washed, and the colors go back to the line's
/// own after it, so the rest of the line and the lines after it show as
/// they would without the count.
pub fn counted(count: u32, line: &[u8], carry: &SgrState) -> Vec<u8> {
    if count < 2 {
        return line.to_vec();
    }
    let lead = leading_sgr(line);
    let mut look = *carry;
    apply_sgr(&mut look, &line[..lead]);
    let gray = SgrState {
        fg: Color::Index(244),
        bg: look.bg,
        ..SgrState::default()
    };
    let mut bytes = sgr(&format!("0;{}", SgrState::default().transition(carry)));
    bytes.extend_from_slice(&line[..lead]);
    bytes.extend(sgr(&look.transition(&gray)));
    bytes.extend(format!("({count}) ").into_bytes());
    bytes.extend(sgr(&gray.transition(&look)));
    bytes.extend_from_slice(&line[lead..]);
    bytes
}

/// The SGR code `ESC [ params m`, with the trailing `;` an empty
/// parameter list leaves gone. Nothing for no parameters.
fn sgr(params: &str) -> Vec<u8> {
    let params = params.trim_end_matches(';');
    if params.is_empty() {
        return Vec::new();
    }
    format!("\x1b[{params}m").into_bytes()
}

/// How many bytes the SGR codes `bytes` opens with take.
fn leading_sgr(bytes: &[u8]) -> usize {
    let mut at = 0;
    while bytes.get(at) == Some(&0x1b) && bytes.get(at + 1) == Some(&b'[') {
        let end = escape_end(bytes, at);
        if bytes.get(end - 1) != Some(&b'm') || !sgr_params(&bytes[at + 2..end - 1]) {
            break;
        }
        at = end;
    }
    at
}

/// True when `params` reads as the parameters of an SGR code.
fn sgr_params(params: &[u8]) -> bool {
    params
        .iter()
        .all(|b| b.is_ascii_digit() || *b == b';' || *b == b':')
}

/// Apply every SGR code in `bytes` to `state`, in order.
fn apply_sgr(state: &mut SgrState, bytes: &[u8]) {
    let mut i = 0;
    while let Some(at) = bytes[i..].iter().position(|&b| b == 0x1b) {
        let start = i + at;
        let end = escape_end(bytes, start);
        if bytes.get(start + 1) == Some(&b'[')
            && end > start + 2
            && bytes[end - 1] == b'm'
            && sgr_params(&bytes[start + 2..end - 1])
        {
            if let Ok(params) = std::str::from_utf8(&bytes[start + 2..end - 1]) {
                state.apply(params);
            }
        }
        i = end.max(start + 1);
    }
}

/// True when a line that shows as `line` can be part of a run of repeated
/// lines: something in it shows, and it takes one row of its own, with no
/// line end or carriage return inside it.
pub fn collapsible(line: &[u8]) -> bool {
    shows_anything(line) && !line.iter().any(|&b| b == b'\r' || b == b'\n')
}

/// What [`Stage::repeat_line`] made of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    /// The line starts a run of its own.
    Starts,
    /// The line joined the run the text ends on, which now holds this
    /// many lines and shows once with the count before it.
    Joins(u32),
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
    /// Lines the region's prompt shows right above the region, which a
    /// change of where your prompt shows moves with it.
    pub above: Option<Above>,
    /// The end of the region `bytes` leave out, since each renderer still
    /// holds it back as the line ends it waits to write (see
    /// [`Output::hold`]). A run of repeated lines rewritten while your
    /// prompt shows pinned carries it. A renderer that writes `bytes` on a
    /// new row, or finds the region open with nothing held back, holds it
    /// back in their place, so the line still ends before the next write.
    /// Empty for every other replace.
    pub tail: Vec<u8>,
}

/// The lines a drawn prompt shows as sent right above its region, such as
/// a tank line your design does not read (D7). A text prompt carries no
/// mark before them, so a renderer finds them by their text: when the
/// rows right above the open region show `plain`, it erases from their
/// first row and writes `bytes` there in place of the replace's own
/// bytes. Otherwise the replace goes as it would without them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Above {
    /// The lines' plain text, joined by `\n`.
    pub plain: String,
    /// What to write from their first row.
    pub bytes: Vec<u8>,
}

/// True when rows reading `rows`, top first and trailing blanks trimmed,
/// show the lines `plain` holds, however a renderer wrapped them. Blanks
/// do not count, since a word wrap drops the one it breaks at.
pub fn shows_lines(rows: &[String], plain: &str) -> bool {
    let squeeze = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let want = squeeze(plain);
    !want.is_empty() && squeeze(&rows.concat()) == want
}

/// What Vosh writes to the terminal for one socket read, or for one
/// repaint. A renderer applies `replace` first, then `bytes`, and keeps
/// `hold` back until the next write lands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub replace: Option<Replace>,
    pub bytes: Vec<u8>,
    /// The live render for the region this output leaves open, written
    /// back before anything else lands, when the region shows a preview.
    pub restore: Option<Vec<u8>>,
    /// What the band above the command line shows from now on, while your
    /// prompt shows pinned: the prompt's rows joined by `\r\n`, with no
    /// line end after the last. Empty clears the band.
    pub pin: Option<Vec<u8>>,
    /// Where each piece of your design landed on the band `pin` shows,
    /// its rows counted from the band's first, so the lines above the
    /// last one that show as sent come before them. None when the band
    /// shows no design: drawing off, the away prompt, the game's own line
    /// while the card reads your codes, or an empty band.
    pub pin_spans: Option<Vec<Span>>,
    /// Line ends the renderer keeps back until the next write lands, so
    /// while you wait the text ends on its last line and not on the empty
    /// rows a pinned prompt left. It is always the tail of `bytes`, never
    /// anything before it: whatever this output writes later takes it
    /// back first.
    pub hold: Vec<u8>,
    /// While your prompt shows pinned, whether the row the pinned prompt
    /// would have held is still where the next thing lands once this
    /// output is written, so each renderer drops the line end that would
    /// end it (see [`close_pin_row`]). None in the text and lifted, where
    /// renderers keep every byte, and on a repaint of the band alone,
    /// which leaves the row as each renderer has it.
    pub pin_row: Option<bool>,
    /// The row a prompt this output pinned left is still open, so the
    /// line end the next write would end it with writes nothing.
    row_open: bool,
    /// The open row is no longer the last thing on screen.
    closed: bool,
    /// The region this output leaves open shows a preview, with the live
    /// render to put in its place the moment anything is written after
    /// it, so a preview never reaches history.
    preview: Option<PreviewTail>,
    /// Output from elsewhere reached the terminal before this output.
    other: bool,
    /// How many writes of this output showed something, so the stage can
    /// tell text landed after a pinned prompt, whoever wrote it.
    visible: u64,
    /// Which output this is, so the stage can tell a new one from the one
    /// it pinned in.
    id: OutputId,
}

/// The region at the end of an output's bytes, or of its replace's bytes
/// when it wrote no bytes, while it shows a preview: where it starts, and
/// the region with the live render.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PreviewTail {
    in_replace: bool,
    at: usize,
    live: Vec<u8>,
}

/// A number each [`Output::new`] hands out. It never makes two outputs
/// unequal, so tests compare outputs by what they write.
#[derive(Debug, Clone, Copy, Default)]
struct OutputId(u64);

impl PartialEq for OutputId {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for OutputId {}

static NEXT_OUTPUT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Output {
    /// An empty output. `other` says output from elsewhere, such as a
    /// slash command's echo, reached the terminal since the session last
    /// wrote, which closes the open row.
    pub fn new(other: bool) -> Self {
        Self {
            closed: other,
            other,
            id: OutputId(NEXT_OUTPUT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
            ..Self::default()
        }
    }

    /// Which output this is, in the order the session made them. Each
    /// renderer keeps the newest it took, so text it writes itself can say
    /// which output it follows (see [`Stage::local_write`]).
    pub fn id(&self) -> u64 {
        self.id.0
    }

    /// Nothing to write and nothing for the band.
    pub fn is_empty(&self) -> bool {
        self.untouched() && self.pin.is_none()
    }

    /// It writes anything to the text: bytes, a replace, held line ends
    /// or a restore. A pin alone writes nothing there.
    pub fn writes_text(&self) -> bool {
        !self.untouched()
    }

    /// Nothing written to the text yet. Held line ends count as written,
    /// and the band does not count.
    fn untouched(&self) -> bool {
        self.replace.is_none()
            && self.bytes.is_empty()
            && self.hold.is_empty()
            && self.restore.is_none()
    }

    /// Put the live render back in place of the preview this output's
    /// open region shows, since something is about to be written after
    /// it. The restore goes with it, since no region is left open.
    fn unpreview(&mut self) {
        let Some(tail) = self.preview.take() else {
            return;
        };
        let bytes = match (tail.in_replace, self.replace.as_mut()) {
            (true, Some(replace)) => &mut replace.bytes,
            _ => &mut self.bytes,
        };
        bytes.truncate(tail.at);
        bytes.extend(tail.live);
        self.restore = None;
    }

    /// The region `region`, which this output just wrote at the very end
    /// of what it wrote, shows a preview. `live` is the same region with
    /// the live render, which renderers write back before anything else
    /// lands, and which this output writes in its place when anything
    /// follows it here.
    fn preview_tail(&mut self, region: usize, live: Vec<u8>) {
        let in_replace = self.bytes.is_empty();
        let len = match (in_replace, &self.replace) {
            (true, Some(replace)) => replace.bytes.len(),
            _ => self.bytes.len(),
        };
        self.restore = Some(live.clone());
        self.preview = Some(PreviewTail {
            in_replace,
            at: len - region,
            live,
        });
    }

    /// Put the held line ends back at the end of the bytes, since
    /// something is about to be written after them.
    fn unhold(&mut self) {
        if !self.hold.is_empty() {
            let hold = std::mem::take(&mut self.hold);
            self.bytes.extend(hold);
        }
    }

    /// Append `bytes` after everything written so far, held line ends
    /// included. After a pinned prompt, the line end that would end its
    /// row goes, since the row is not in the text.
    fn push(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.unpreview();
        let bytes = if self.row_open {
            let (rest, closed) = close_pin_row(bytes);
            self.row_open = !closed;
            rest
        } else {
            std::borrow::Cow::Borrowed(bytes)
        };
        if bytes.is_empty() {
            return;
        }
        self.unhold();
        self.bytes.extend_from_slice(&bytes);
        if shows_anything(&bytes) {
            self.visible += 1;
        }
    }

    /// Keep back the line ends this output ends on, the ones a pinned
    /// prompt's row would have followed.
    fn hold_tail(&mut self) {
        self.unpreview();
        self.unhold();
        let at = trailing_line_ends(&self.bytes);
        if at < self.bytes.len() {
            self.hold = self.bytes.split_off(at);
        }
    }

    /// Write bytes that are not a region, such as a line as the Line
    /// pass left it. They close the open row.
    pub fn text(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.push(bytes);
        self.closed = true;
    }

    /// Write `bytes` as region `gen`, such as the partial a read ended on.
    /// It closes the open row.
    pub fn region(&mut self, gen: u64, bytes: &[u8]) {
        self.push(&mark(gen));
        self.push(bytes);
        self.closed = true;
    }

    /// Replace region `gen`, which an earlier output wrote. Before
    /// anything else in this output it rides as [`Output::replace`], and
    /// the renderer decides whether the region is still open. After other
    /// bytes the region is closed, so `fresh` bytes follow on a new row
    /// and anything else is dropped.
    pub fn replace(&mut self, gen: u64, bytes: Vec<u8>, fresh: bool) {
        if self.untouched() {
            if shows_anything(&bytes) {
                self.visible += 1;
            }
            self.replace = Some(Replace {
                gen,
                bytes,
                fresh,
                above: None,
                tail: Vec::new(),
            });
        } else if fresh && !bytes.is_empty() {
            self.new_row();
            self.push(&bytes);
        } else {
            return;
        }
        self.closed = true;
    }

    /// Replace the open region `gen` with `bytes`, as a repaint does, and
    /// with it the lines `above` it when a renderer finds them there.
    fn replace_above(&mut self, gen: u64, bytes: Vec<u8>, above: Option<Above>) {
        self.replace(gen, bytes, false);
        if let Some(replace) = self.replace.as_mut().filter(|r| r.gen == gen) {
            replace.above = above;
        }
    }

    /// How much this output wrote to the text at the cursor, its bytes and
    /// the line ends it holds back together. Holding line ends back moves
    /// them and leaves this the same, so only a new write or a rewrite
    /// changes it.
    fn written(&self) -> usize {
        self.bytes.len() + self.hold.len()
    }

    /// Write `bytes` in place of everything this output wrote from `at`
    /// on, the line ends it holds back included, as a run of repeated
    /// lines this output wrote last does when the next line joins it.
    fn rewrite_from(&mut self, at: usize, bytes: &[u8]) {
        self.hold.clear();
        self.bytes.truncate(at);
        self.push(bytes);
        self.closed = true;
    }

    /// Write `bytes` in place of what this output's replace writes, and in
    /// place of what it writes from the lines above its region when it
    /// carries them, as a run of repeated lines this output rewrote does
    /// when the next line joins it. The new text lands on the row a pinned
    /// prompt left, as a write at the cursor would. `tail` is what `bytes`
    /// leave out of the region (see [`Replace::tail`]).
    fn rewrite_replace(&mut self, bytes: Vec<u8>, tail: Vec<u8>) {
        let Some(replace) = self.replace.as_mut() else {
            return;
        };
        if let Some(above) = replace.above.as_mut() {
            above.bytes.clone_from(&bytes);
        }
        replace.bytes = bytes;
        replace.tail = tail;
        replace.fresh = true;
        self.visible += 1;
        self.row_open = false;
        self.closed = true;
    }

    /// End the row the cursor sits on, unless this output already left it
    /// at the start of one.
    fn new_row(&mut self) {
        self.unpreview();
        if self.untouched() || !self.at_row_start() {
            self.push(b"\r\n");
        }
    }

    /// True when what this output wrote last ends a row. Marks write
    /// nothing, so they do not count.
    fn at_row_start(&self) -> bool {
        let last = if !self.hold.is_empty() {
            &self.hold[..]
        } else if self.bytes.is_empty() {
            self.replace.as_ref().map_or(&[][..], |r| &r.bytes[..])
        } else {
            &self.bytes[..]
        };
        without_trailing_marks(last).ends_with(b"\n")
    }
}

/// True when `bytes` put a visible character on screen: anything but line
/// ends, spaces and escape sequences.
fn shows_anything(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x1b => i = escape_end(bytes, i),
            b'\r' | b'\n' | b' ' | b'\t' | 0x07 => i += 1,
            _ => return true,
        }
    }
    false
}

/// What `bytes` does to the row a pinned prompt left open. The prompt is
/// not in the text, so the line end that would end its row writes
/// nothing: the first one, when only escape sequences and carriage
/// returns come before it. Returns the bytes left to write, and whether
/// the row is closed, by that line end or by anything else that lands on
/// it first. Escape sequences alone leave it open. Both renderers apply
/// it to what reaches them from outside the session's reads, such as a
/// framed echo or an error notice, and the stage to its own writes after
/// a pin in the same output.
pub fn close_pin_row(bytes: &[u8]) -> (std::borrow::Cow<'_, [u8]>, bool) {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x1b => i = escape_end(bytes, i),
            b'\r' => i += 1,
            b'\n' => {
                let start = if i > 0 && bytes[i - 1] == b'\r' {
                    i - 1
                } else {
                    i
                };
                let mut rest = bytes[..start].to_vec();
                rest.extend_from_slice(&bytes[i + 1..]);
                return (std::borrow::Cow::Owned(rest), true);
            }
            _ => return (std::borrow::Cow::Borrowed(bytes), true),
        }
    }
    (std::borrow::Cow::Borrowed(bytes), false)
}

/// `text` with every escape sequence taken out, as it shows.
pub(crate) fn plain_text(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i = escape_end(bytes, i);
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Where the escape sequence that starts at `at` ends: after the final
/// byte of a CSI, after the BEL or ST of an OSC, after the next byte
/// otherwise.
fn escape_end(bytes: &[u8], at: usize) -> usize {
    match bytes.get(at + 1) {
        Some(b'[') => {
            let mut i = at + 2;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            (i + 1).min(bytes.len())
        }
        Some(b']') => {
            let mut i = at + 2;
            while i < bytes.len() {
                if bytes[i] == 0x07 {
                    return i + 1;
                }
                if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'\\') {
                    return i + 2;
                }
                i += 1;
            }
            bytes.len()
        }
        Some(_) => at + 2,
        None => bytes.len(),
    }
}

/// Where the line ends `bytes` finishes on start: a run of `\r`, `\n` and
/// SGR codes after the last thing that shows, holding at least one line
/// end. `bytes.len()` when there is none. A mark or any other escape stops
/// the run, so a region never loses its start to the hold.
pub fn trailing_line_ends(bytes: &[u8]) -> usize {
    let mut at = bytes.len();
    let mut line_end = false;
    loop {
        match bytes[..at].last() {
            Some(b'\n') => {
                line_end = true;
                at -= 1;
            }
            Some(b'\r') => at -= 1,
            Some(b'm') => match sgr_start(&bytes[..at]) {
                Some(start) => at = start,
                None => break,
            },
            _ => break,
        }
    }
    if line_end {
        at
    } else {
        bytes.len()
    }
}

/// The start of the SGR code `ESC [ params m` that `bytes` ends with.
fn sgr_start(bytes: &[u8]) -> Option<usize> {
    let end = bytes.len().checked_sub(1)?;
    let esc = bytes[..end].iter().rposition(|&b| b == 0x1b)?;
    let params = &bytes[esc + 1..end];
    (params.first() == Some(&b'[')
        && params[1..]
            .iter()
            .all(|b| b.is_ascii_digit() || *b == b';' || *b == b':'))
    .then_some(esc)
}

/// `bytes` without the marks at its end, region and lift marks alike.
fn without_trailing_marks(mut bytes: &[u8]) -> &[u8] {
    let prefix = format!("\x1b]{MARK_OSC};");
    while bytes.ends_with(b"\x07") {
        let Some(start) = find_last(bytes, prefix.as_bytes()) else {
            break;
        };
        let tail = &bytes[start + prefix.len()..bytes.len() - 1];
        let Some(digits) = tail
            .strip_prefix(b"o;")
            .or_else(|| tail.strip_prefix(b"l;"))
            .or_else(|| tail.strip_prefix(b"e;"))
        else {
            break;
        };
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
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
    /// The groups each line reads, top line first, so the lines the drawn
    /// prompt replaces follow an edit that changes what your design
    /// reads. Empty when not known, and then `replaced` stays as it is.
    pub groups: Vec<Vec<String>>,
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

    /// How many lines [`Block::heads_shown`] shows.
    fn heads_shown_rows(&self) -> usize {
        let last = self.lines.len().saturating_sub(1);
        (0..last)
            .filter(|index| !self.replaced.contains(index))
            .count()
    }

    /// The lines [`Block::heads_shown`] shows, with their plain text, or
    /// None when it shows none.
    fn heads_shown_with_text(&self) -> Option<(Vec<u8>, String)> {
        let bytes = self.heads_shown();
        if bytes.is_empty() {
            return None;
        }
        let last = self.lines.len().saturating_sub(1);
        let plain: Vec<&str> = self
            .lines
            .iter()
            .enumerate()
            .take(last)
            .filter(|(index, _)| !self.replaced.contains(index))
            .map(|(_, line)| line.plain.as_str())
            .collect();
        Some((bytes, plain.join("\n")))
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

/// The lift the open row carries while your prompt shows lifted, or the
/// open card borrows Lifted's band in the text, and whether its start
/// mark sits inside the row's region, as it does when you chose Lifted
/// with the row open, so a repaint writes it again. Its end keeps your
/// echo a cell away, the card's borrowed lift too (the 2026-09-30
/// addendum, item 2), so a row the card lifted keeps that space when you
/// later choose Lifted, in the open row and in your scrollback alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenLift {
    id: u64,
    start_inside: bool,
}

/// The drawn prompt while it is the last thing on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRow {
    pub gen: u64,
    /// What the row shows: the drawn prompt, or with drawing off the lines
    /// it replaced.
    pub body: Vec<u8>,
    /// The drawn prompt with the live values, while the row shows a
    /// preview in its place. Renderers hold it as the row's restore.
    pub live: Option<Vec<u8>>,
    /// Where each piece of the design landed in the row, from the render
    /// that drew it. Empty while the row shows the game's own lines.
    pub spans: Vec<Span>,
    /// The rows of the design the row shows, as plain text joined by
    /// `\n`, which the webview wraps at its own width to put each span
    /// on screen. Empty while the row shows the game's own lines.
    pub plain: String,
}

/// What your prompt shows, handed to [`Stage::draw_view`],
/// [`Stage::pin_view`] and [`Stage::repaint_view`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct View<'a> {
    /// Your design drawn as your prompt shows it, or None for the lines
    /// the game sent: drawing off, or the card reading your codes.
    pub shown: Option<&'a str>,
    /// Your design drawn with the live values, while `shown` is a preview
    /// of the open card's in its place: other values, the labels of the
    /// values with nothing to show, or the game's own line. The region
    /// carries it as its restore, so only live renders reach history.
    /// None when your prompt shows the live render. The band above the
    /// command line never needs it, since nothing on it reaches history.
    pub live: Option<&'a str>,
    /// Where each piece of the design landed in `shown`, from the render
    /// that drew it. Empty while `shown` is None.
    pub spans: &'a [Span],
    /// The rows `shown` draws, as plain text joined by `\n`. Empty while
    /// `shown` is None.
    pub plain: &'a str,
}

impl<'a> View<'a> {
    /// The live render, or the game's lines with drawing off.
    pub fn live(rendered: Option<&'a str>) -> Self {
        Self {
            shown: rendered,
            ..Self::default()
        }
    }

    /// The open row `gen` showing `body`, with the live render behind it,
    /// and where the pieces of what it shows landed.
    fn open_row(self, gen: u64, body: Vec<u8>, live: Option<Vec<u8>>) -> OpenRow {
        let drawn = self.shown.is_some();
        OpenRow {
            gen,
            body,
            live,
            spans: if drawn {
                self.spans.to_vec()
            } else {
                Vec::new()
            },
            plain: if drawn {
                self.plain.to_string()
            } else {
                String::new()
            },
        }
    }
}

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

    /// The lines of a prompt of `count` lines that the drawn prompt
    /// replaces, `groups` holding what each line reads: the last line,
    /// and each line above it that carries a value your design reads.
    fn replaced(&self, count: usize, groups: &[Vec<String>]) -> Vec<usize> {
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

/// The empty line a pinned prompt's row would have ended with, which
/// writes nothing when it comes. `seen` is how many visible writes the
/// output `output` had when the prompt pinned, so a later one disarms it,
/// and so does any visible write in a later output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Swallow {
    output: u64,
    seen: u64,
}

impl Swallow {
    /// Armed as of now in `out`.
    fn at(out: &Output) -> Self {
        Self {
            output: out.id.0,
            seen: out.visible,
        }
    }

    /// Something showed in `out` since it armed, or `out` is a later
    /// output that came after output from elsewhere. Output from
    /// elsewhere that came before the output it armed in came before the
    /// prompt, so it ends nothing.
    fn ended_by(self, out: &Output) -> bool {
        if out.id.0 == self.output {
            out.visible > self.seen
        } else {
            out.other || out.visible > 0
        }
    }
}

/// A prompt pinned with drawing off, after Prompts triggers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PinnedShown {
    /// What the band shows.
    band: Vec<u8>,
    /// What the text would have shown, line ends included.
    text: Vec<u8>,
}

/// Where the line of a run of repeated lines sits in the output that last
/// wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunPlace {
    /// In the output's bytes, from this index.
    Bytes(usize),
    /// In the output's replace, which rewrites an earlier region whole.
    Replace,
}

/// The region right after the line of a run: a partial a read ended on,
/// which the next line completes, or the empty region a pinned prompt
/// left where it erased one, which the next line writes after. Either way
/// the line of the run sits right above it, so a renderer finds it there
/// by its text (see [`Above`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RunAfter {
    gen: u64,
    empty: bool,
}

/// The run of repeated lines the text ends on, while Collapse repeated
/// lines is on (see [`Stage::repeat_line`]).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
    /// The line as it shows, colors included, which the next line has to
    /// show byte for byte to join the run.
    line: Vec<u8>,
    /// The colors the text carried into the run's first line, which the
    /// run is written again from.
    carry: SgrState,
    /// The colors after the ones the line opens with, which the next line
    /// has to start in too, so it looks the same.
    look: SgrState,
    /// How many lines the run holds.
    count: u32,
    /// The region the run shows in.
    gen: u64,
    /// How many bytes the region's mark takes, so a line that ends the
    /// run in the output that wrote it can take the mark out.
    mark: usize,
    /// The output that last wrote the run, or the region after it.
    output: u64,
    /// What that output had written at the cursor right after it (see
    /// [`Output::written`]), so anything written later shows.
    end: usize,
    place: RunPlace,
    /// How many bytes at the end of the run's region each renderer keeps
    /// back as held line ends while your prompt shows pinned. A rewrite
    /// leaves them out, so the line ends each renderer holds still follow
    /// it.
    held: usize,
    after: Option<RunAfter>,
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
    /// mortal. See [`Stage::set_capture_for`]. Test only. The app's tests
    /// reach it through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn set_capture(&mut self, capture: &CaptureConfig) {
        self.set_capture_for(capture, Who::default());
    }

    /// Compile the capture a profile's `[prompt]` table holds. `who`
    /// decides what Aabahran's `%u` and `%s` print.
    pub fn set_capture_for(&mut self, capture: &CaptureConfig, who: Who) {
        self.recognizer = Recognizer::compile_for(capture, who);
        // Once something reads your prompt, no trigger hides it with
        // nothing drawn in its place.
        if self.recognizer.is_some() {
            self.forget_gags_without_reader();
        }
    }

    /// The compiled capture.
    pub fn recognizer(&self) -> Option<&Recognizer> {
        self.recognizer.as_ref()
    }

    /// Take the fields your design reads, which decide the lines above
    /// the last one it hides (D7). The last prompt read follows at once,
    /// so a repaint after an edit shows a line above the last one as sent
    /// exactly when the new design leaves it alone.
    pub fn set_reads(&mut self, reads: &BTreeSet<FieldRef>) {
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
    pub fn reset(&mut self) {
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

    /// Take Collapse repeated lines from the profile. While it is on, the
    /// stage follows the colors the text carries from one line to the
    /// next, so a run it writes again keeps them (see [`counted`]).
    pub fn set_collapse(&mut self, on: bool) {
        self.collapse = on;
    }

    /// Follow the colors `bytes`, written to the text, leave the text in,
    /// while Collapse repeated lines is on.
    fn carry_through(&mut self, bytes: &[u8]) {
        if self.collapse {
            apply_sgr(&mut self.carry, bytes);
        }
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
    pub fn card_open(&self) -> bool {
        self.card
    }

    /// Take where your prompt shows from the `[prompt]` table. The prompt
    /// on screen moves at the next repaint.
    pub fn set_show(&mut self, show: PromptShow) {
        self.show = show;
    }

    /// Where your prompt shows. Test only.
    #[cfg(test)]
    pub fn shows(&self) -> PromptShow {
        self.show
    }

    /// What the band shows, while your prompt shows pinned. Test only.
    #[cfg(test)]
    pub fn pinned(&self) -> Option<&[u8]> {
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
    pub fn zone(&self, draw: bool, template: &Template) -> usize {
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

    /// Catch the run of repeated lines up with `out` before it goes out.
    /// Anything written after the run ends it: later bytes in the output
    /// that wrote it, or any text in a later output. Output from elsewhere
    /// that came before a later output ends it too. While the run is the
    /// last thing its own output wrote, note the line ends at its end that
    /// the output holds back, which each renderer keeps after it.
    fn settle_run(&mut self, out: &Output) {
        let Some(run) = self.run.as_mut() else {
            return;
        };
        if run.output != out.id.0 {
            if out.other || out.writes_text() {
                self.run = None;
            }
            return;
        }
        if out.written() != run.end {
            self.run = None;
        } else if run.after.is_none() && matches!(run.place, RunPlace::Bytes(_)) {
            run.held = out.hold.len();
        }
    }

    /// True when the run of repeated lines, and the region after it if
    /// any, is still the last thing written as of `out`: nothing came after
    /// it in the output that wrote it, or `out` is a later output that has
    /// written nothing yet with no output from elsewhere before it. Every
    /// output between the two went through [`Stage::settle_run`], which
    /// ends the run when one wrote text.
    fn run_last(&self, out: &Output) -> bool {
        match &self.run {
            None => false,
            Some(run) if run.output == out.id.0 => out.written() == run.end,
            Some(_) => !out.other && out.untouched(),
        }
    }

    /// Note a complete line that is not your prompt for the candidates
    /// ring.
    fn note_line(&mut self, raw: &[u8], plain: &str) {
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
    fn paint_held(&mut self, out: &mut Output, partial: &[u8]) -> (u64, usize) {
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
        self.draw_view(out, block, painted, before, View::live(Some(rendered)));
    }

    /// [`Stage::draw`] with what the open card shows: a preview, the
    /// labels of values with nothing to show, or the game's own line. The
    /// region carries the live render as its restore, and anything this
    /// output writes after it puts the live render back first, so only
    /// live renders reach history.
    pub fn draw_view(
        &mut self,
        out: &mut Output,
        block: Block,
        painted: Option<u64>,
        before: &[u8],
        view: View,
    ) {
        self.sync(out);
        let lift = self.lifts_view(&view).then(|| OpenLift {
            id: self.next_gen(),
            start_inside: false,
        });
        let gen = self.next_gen();
        let (body, live) = row_bodies(&block, view);
        let region = region_bytes(gen, lift, &body);
        let mut bytes = before.to_vec();
        if let Some(lift) = lift {
            bytes.extend(lift_start(lift.id));
        }
        bytes.extend(block.heads_shown());
        let len = region.len();
        bytes.extend(region);
        self.carry_through(&bytes);
        put(out, painted, bytes, true);
        if let Some(live) = &live {
            out.preview_tail(len, region_bytes(gen, lift, live));
        }
        out.closed = false;
        self.open = Some(view.open_row(gen, body, live));
        self.open_since = out.id.0;
        self.open_lift = lift;
        self.open_heads = block.heads_shown_with_text();
        self.shown_as = self.show;
        self.note_recognized(block);
    }

    /// Your prompt shows lifted, so each one carries lift marks.
    fn lifts(&self) -> bool {
        self.show == PromptShow::Lifted
    }

    /// The row that shows `view` carries lift marks: your prompt shows
    /// lifted, or the open card borrows the band for your design in the
    /// text.
    fn lifts_view(&self, view: &View) -> bool {
        self.lifts() || (self.card && self.show == PromptShow::Text && view.shown.is_some())
    }

    /// `bytes`, the lines of a prompt shown as sent, between the marks of
    /// a new lift while your prompt shows lifted and something in them
    /// shows.
    fn lift_shown(&mut self, bytes: Vec<u8>) -> Vec<u8> {
        if !self.lifts() || !shows_anything(&bytes) {
            return bytes;
        }
        let id = self.next_gen();
        let mut out = lift_start(id);
        out.extend(with_lift_end(&bytes, id));
        out
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
        let mut shown = block.heads();
        if let Some(display) = display {
            shown.extend_from_slice(display);
            shown.extend_from_slice(block.final_line().end.terminator());
        }
        let mut bytes = before.to_vec();
        bytes.extend(self.lift_shown(shown));
        self.carry_through(&bytes);
        write(out, &mut self.open, painted, bytes);
        self.shown_as = self.show;
        self.note_recognized(block);
    }

    /// [`Stage::pin_view`] with `rendered` as the live render. Test only.
    #[cfg(test)]
    pub fn pin_drawn(
        &mut self,
        out: &mut Output,
        block: Block,
        painted: Option<u64>,
        before: &[u8],
        rendered: &str,
    ) {
        self.pin_view(out, block, painted, before, View::live(Some(rendered)));
    }

    /// Pin `block` with what the open card shows: it leaves the text, and
    /// the band shows the lines above the last one that show as sent, then
    /// the design `view` holds. The band needs no restore, since nothing
    /// on it reaches history. It shows the live render again at the next
    /// repaint without a preview.
    pub fn pin_view(
        &mut self,
        out: &mut Output,
        block: Block,
        painted: Option<u64>,
        before: &[u8],
        view: View,
    ) {
        let body = pin_body(&block, view.shown);
        let spans = band_spans(&block, view);
        self.pin(out, block, painted, before, body, spans);
        self.pinned_shown = None;
    }

    /// Pin `block` shown as sent, drawing off: the band shows every line
    /// above the last one, then what Prompts triggers left of the last.
    /// `display` is None when one hid it. See [`Stage::pin`].
    pub fn pin_shown(
        &mut self,
        out: &mut Output,
        block: Block,
        painted: Option<u64>,
        before: &[u8],
        display: Option<&[u8]>,
    ) {
        let mut body = block.heads();
        let mut text = block.heads();
        if let Some(display) = display {
            body.extend_from_slice(display);
            text.extend_from_slice(display);
            text.extend_from_slice(block.final_line().end.terminator());
        }
        let body = trim_line_end(body);
        self.pinned_shown = Some(PinnedShown {
            band: body.clone(),
            text,
        });
        self.pin(out, block, painted, before, body, None);
    }

    /// Take `block` out of the text and put `body` on the band, with the
    /// pieces of the design it shows as `spans`. `before`,
    /// such as lines a Prompts trigger's script echoed, still goes to the
    /// text, over the region `painted` when an earlier read painted part
    /// of the prompt there. With nothing before, that region is erased,
    /// which a renderer that wrote after it drops. The line ends the text
    /// ends on are held back, and the empty line your next unasked text
    /// starts with writes nothing, unless the prompt already took its line
    /// end.
    fn pin(
        &mut self,
        out: &mut Output,
        block: Block,
        painted: Option<u64>,
        before: &[u8],
        body: Vec<u8>,
        spans: Option<Vec<Span>>,
    ) {
        self.sync(out);
        self.carry_through(before);
        match painted {
            // The prompt completes a partial painted right after a run of
            // repeated lines. Its region empties and stays open, so the
            // next line can still find the run right above it.
            Some(gen) if before.is_empty() && out.untouched() && self.run_partial(out, gen) => {
                let empty = self.next_gen();
                out.replace(gen, mark(empty), false);
                if let Some(run) = self.run.as_mut() {
                    run.after = Some(RunAfter {
                        gen: empty,
                        empty: true,
                    });
                    run.output = out.id.0;
                    run.end = out.written();
                }
            }
            Some(gen) => out.replace(gen, before.to_vec(), !before.is_empty()),
            None => out.text(before),
        }
        out.hold_tail();
        self.open = None;
        self.swallow = (block.final_line().end != End::SettledLine).then(|| Swallow::at(out));
        out.row_open = self.swallow.is_some();
        self.set_band(out, body, spans);
        self.shown_as = PromptShow::Pinned;
        self.note_recognized(block);
    }

    /// Put `body` on the band, with the pieces of the design it shows.
    fn set_band(&mut self, out: &mut Output, body: Vec<u8>, spans: Option<Vec<Span>>) {
        out.pin = Some(body.clone());
        out.pin_spans.clone_from(&spans);
        self.pinned = Some(body);
        self.pinned_spans = spans;
    }

    /// Write a complete line that is not your prompt, as the Line pass
    /// left it. `painted` is the region an earlier read painted its start
    /// as. `bytes` is empty when a trigger hid the line. Returns true when
    /// the line was the line end a pinned prompt's row would have taken,
    /// which writes nothing.
    pub fn line(
        &mut self,
        out: &mut Output,
        raw: &[u8],
        plain: &str,
        painted: Option<u64>,
        bytes: &[u8],
    ) -> bool {
        self.sync(out);
        // The line end a pinned prompt's row would have taken, and any
        // empty line with it, writes nothing. It is still logged.
        let swallowed = self.swallow.is_some()
            && painted.is_none()
            && plain.trim().is_empty()
            && !shows_anything(bytes);
        if !swallowed {
            // Anything written ends a run of repeated lines. A hidden line
            // that was never painted writes nothing, so the run goes on.
            if !bytes.is_empty() || painted.is_some() {
                self.end_run(out);
            }
            self.carry_through(bytes);
            write(out, &mut self.open, painted, bytes.to_vec());
        }
        self.note_line(raw, plain);
        swallowed
    }

    /// Write a complete line that is not your prompt while Collapse
    /// repeated lines is on. `line` is what the Line pass left of it, which
    /// shows on one row of its own (see [`collapsible`]), and `painted` is
    /// the region an earlier read painted its start as.
    ///
    /// A line that shows the same bytes as the run of repeated lines the
    /// text ends on joins the run. The run's region is written again in
    /// place, the line once with the count before it: in this output's
    /// own bytes or replace when it wrote the run, as the replace of the
    /// run's region in a later output, or, past a region right after the
    /// run, as the replace of that region with the run found right above
    /// it. A renderer that finds the region closed writes the counted line
    /// on a new row. Any other line starts a run of its own, written as a
    /// region over `painted` or after everything else.
    pub fn repeat_line(
        &mut self,
        out: &mut Output,
        raw: &[u8],
        plain: &str,
        painted: Option<u64>,
        line: &[u8],
    ) -> Repeat {
        self.sync(out);
        self.note_line(raw, plain);
        self.open = None;
        // The colors the line starts in, and the ones after those it opens
        // with. A line that starts in other colors looks different even
        // with the same bytes, so it starts a run of its own.
        let carry = self.carry;
        let mut look = carry;
        apply_sgr(&mut look, &line[..leading_sgr(line)]);
        let joins = self.run_last(out)
            && self.run.as_ref().is_some_and(|run| {
                run.line == line
                    && run.look == look
                    && match run.after {
                        None => painted.is_none(),
                        Some(after) if after.empty => painted.is_none(),
                        // The line completes the partial an earlier read
                        // painted right after the run.
                        Some(after) => painted == Some(after.gen) && run.output != out.id.0,
                    }
            });
        if joins {
            return self.join_run(out);
        }
        self.end_run(out);
        self.carry_through(line);
        let gen = self.next_gen();
        let mut bytes = mark(gen);
        let mark_len = bytes.len();
        bytes.extend_from_slice(line);
        bytes.extend_from_slice(b"\r\n");
        let place = match painted {
            Some(old) if out.untouched() => {
                out.replace(old, bytes, true);
                RunPlace::Replace
            }
            Some(old) => {
                let len = bytes.len();
                out.replace(old, bytes, true);
                RunPlace::Bytes(out.bytes.len() - len)
            }
            None => {
                out.text(&bytes);
                RunPlace::Bytes(out.bytes.len() - bytes.len())
            }
        };
        self.run = Some(Run {
            line: line.to_vec(),
            carry,
            look,
            count: 1,
            gen,
            mark: mark_len,
            output: out.id.0,
            end: out.written(),
            place,
            held: 0,
            after: None,
        });
        Repeat::Starts
    }

    /// The run of repeated lines ends, as a line written after it does.
    /// When `out` wrote it in its bytes and nothing came after it yet, its
    /// region is never written again, so its mark goes. An output then
    /// carries no mark of a run but the one of the run it ends on.
    fn end_run(&mut self, out: &mut Output) {
        let Some(run) = self.run.take() else {
            return;
        };
        if let (RunPlace::Bytes(at), None) = (run.place, run.after) {
            if run.output == out.id.0 && out.written() == run.end {
                out.bytes.drain(at..at + run.mark);
            }
        }
    }

    /// The run of repeated lines the text ends on as it shows, the count
    /// before its line from the second on, and the region it shows in.
    /// None when there is none.
    pub fn run_shown(&self) -> Option<(Vec<u8>, u64)> {
        let run = self.run.as_ref()?;
        Some((counted(run.count, &run.line, &run.carry), run.gen))
    }

    /// The next line joins the run of repeated lines, which [`run_last`]
    /// found is still the last thing written. See [`Stage::repeat_line`].
    ///
    /// [`run_last`]: Stage::run_last
    fn join_run(&mut self, out: &mut Output) -> Repeat {
        let gen = self.next_gen();
        let Some(run) = self.run.as_mut() else {
            return Repeat::Starts;
        };
        // What the run shows now, which a renderer finds above the region
        // after it.
        let shows = plain_text(&String::from_utf8_lossy(&counted(
            run.count, &run.line, &run.carry,
        )));
        run.count += 1;
        let mut whole = mark(gen);
        run.mark = whole.len();
        whole.extend(counted(run.count, &run.line, &run.carry));
        whole.extend_from_slice(b"\r\n");
        // Without the line ends each renderer keeps back after the run, so
        // they still follow it. The replace carries them as its tail.
        let mut kept = whole.clone();
        let mut tail = Vec::new();
        if run.held <= run.line.len() + 2 {
            tail = kept.split_off(kept.len() - run.held);
        }
        // The run leaves the text in the colors its line leaves it in.
        self.carry = run.carry;
        apply_sgr(&mut self.carry, &run.line);
        let same = run.output == out.id.0;
        match (run.after, same) {
            (None, true) => match run.place {
                RunPlace::Bytes(at) => out.rewrite_from(at, &whole),
                RunPlace::Replace => out.rewrite_replace(kept, tail),
            },
            (None, false) => {
                out.replace(run.gen, kept, true);
                if let Some(replace) = out.replace.as_mut().filter(|r| r.gen == run.gen) {
                    replace.tail = tail;
                }
                run.place = RunPlace::Replace;
            }
            (Some(_), true) => {
                // A pinned prompt in this output emptied the region after
                // the run, and its replace now writes the run in its place.
                if let Some(replace) = out.replace.as_mut() {
                    replace.above = Some(Above {
                        plain: shows,
                        bytes: Vec::new(),
                    });
                }
                out.rewrite_replace(whole, Vec::new());
                run.place = RunPlace::Replace;
                run.held = 0;
            }
            (Some(after), false) => {
                out.replace(after.gen, whole.clone(), true);
                if let Some(replace) = out.replace.as_mut() {
                    replace.above = Some(Above {
                        plain: shows,
                        bytes: whole,
                    });
                }
                run.place = RunPlace::Replace;
                run.held = 0;
            }
        }
        run.gen = gen;
        run.after = None;
        run.output = out.id.0;
        run.end = out.written();
        Repeat::Joins(run.count)
    }

    /// True when the partial region `gen` is the one painted right after
    /// the run of repeated lines, which is still the last thing written.
    fn run_partial(&self, out: &Output, gen: u64) -> bool {
        self.run_last(out)
            && self
                .run
                .as_ref()
                .is_some_and(|run| run.after == Some(RunAfter { gen, empty: false }))
    }

    /// True when a region written at the cursor now lands right after the
    /// run of repeated lines: the run is still the last thing written, and
    /// nothing follows it but the empty region a pinned prompt left.
    fn run_open_after(&self, out: &Output) -> bool {
        self.run_last(out)
            && self
                .run
                .as_ref()
                .is_some_and(|run| run.after.map_or(true, |after| after.empty))
    }

    /// Note that the region `gen`, a partial or held lines, now follows
    /// the run of repeated lines.
    fn run_followed_by(&mut self, out: &Output, gen: u64) {
        if let Some(run) = self.run.as_mut() {
            run.after = Some(RunAfter { gen, empty: false });
            run.output = out.id.0;
            run.end = out.written();
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
                // A partial right after a run of repeated lines that grew
                // still follows the run.
                let follows = self.run_partial(out, old);
                let gen = self.next_gen();
                let mut bytes = mark(gen);
                bytes.extend_from_slice(raw);
                out.replace(old, bytes, true);
                self.open = None;
                if follows {
                    self.run_followed_by(out, gen);
                }
                Some((gen, raw.len()))
            }
            None if raw.is_empty() => None,
            None => {
                // A partial painted right after a run of repeated lines
                // follows the run, so the line it becomes can join it.
                let follows = self.run_open_after(out);
                let gen = self.next_gen();
                out.region(gen, raw);
                self.open = None;
                if follows {
                    self.run_followed_by(out, gen);
                }
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
            self.carry_through(raw);
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
        self.carry_through(&bytes);
        write(out, &mut self.open, painted, bytes);
    }

    /// [`Stage::repaint_view`] with `rendered` as the live render. Test
    /// only. The app's tests reach it through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn repaint(&mut self, out: &mut Output, rendered: Option<&str>) {
        self.repaint_view(out, View::live(rendered));
    }

    /// Repaint the open row as the `[prompt]` table now says, with what
    /// the open card shows, as a preview the card sets or clears asks: the
    /// design `view` holds while drawing is on, else the lines the drawn
    /// prompt replaced, as the game sent them. Nothing when no row is open
    /// or it already shows that. A repaint is dropped by a renderer that
    /// wrote anything after the row. In the text and lifted, the row carries
    /// the live render as its restore while it shows anything else, so a
    /// renderer writes the live render back before anything lands after
    /// it. The band shows the preview with no restore.
    ///
    /// A repaint of the band alone writes nothing to the text, so it says
    /// nothing about the pinned prompt's row. Your echo can close that row
    /// in a renderer before the session hears of it, and a repaint that
    /// went out in between would open it again there.
    pub fn repaint_view(&mut self, out: &mut Output, view: View) {
        let band_only = self.shown_as == PromptShow::Pinned && self.show == PromptShow::Pinned;
        self.repaint_row(out, view);
        if !band_only {
            self.seal(out);
        }
        self.note_sent(out);
    }

    /// True when there is something a repaint of what your prompt shows
    /// could change: the open row, or the band while your prompt shows
    /// pinned. False while a change of where your prompt shows waits for
    /// its own repaint. It draws nothing, so the session asks it after
    /// every read that brought GMCP packets.
    pub fn repaintable(&self) -> bool {
        if self.last_raw.is_none() {
            return false;
        }
        match (self.shown_as, self.show) {
            (PromptShow::Pinned, PromptShow::Pinned) => self.pinned.is_some(),
            (PromptShow::Pinned, _) | (_, PromptShow::Pinned) => false,
            _ => self.open.is_some() && !(self.open_lift.is_none() && self.lifts()),
        }
    }

    /// True when a repaint with `view` would change what your prompt
    /// shows, the open row or the band, such as after a GMCP packet that
    /// arrived with no prompt after it. False while there is nothing to
    /// repaint, and while a change of where your prompt shows waits for
    /// its own repaint. Test only.
    #[cfg(test)]
    pub fn stale(&self, view: View) -> bool {
        let Some(block) = &self.last_raw else {
            return false;
        };
        match (self.shown_as, self.show) {
            (PromptShow::Pinned, PromptShow::Pinned) => self
                .pinned
                .as_ref()
                .is_some_and(|pinned| self.band_body(block, view.shown) != *pinned),
            (PromptShow::Pinned, _) | (_, PromptShow::Pinned) => false,
            _ => {
                let Some(open) = &self.open else {
                    return false;
                };
                if self.open_lift.is_none() && self.lifts() {
                    return false;
                }
                let (body, live) = row_bodies(block, view);
                body != open.body
                    || live != open.live
                    || block.heads_shown_with_text() != self.open_heads
            }
        }
    }

    /// [`Stage::repaint_view`], before the renderers hear about the pinned
    /// prompt's row.
    fn repaint_row(&mut self, out: &mut Output, view: View) {
        self.sync(out);
        match (self.shown_as, self.show) {
            (PromptShow::Pinned, PromptShow::Pinned) => return self.repaint_pinned(out, view),
            (_, PromptShow::Pinned) => return self.move_to_pinned(out, view),
            (PromptShow::Pinned, _) => return self.move_from_pinned(out, view),
            _ => {}
        }
        // Choosing Lifted lifts the open row, from its region's start, and
        // so does the open card in the text.
        let lifting = self.open_lift.is_none() && self.lifts_view(&view);
        let Some(open) = &mut self.open else {
            return;
        };
        let Some(block) = &self.last_raw else {
            return;
        };
        // An edit can change which lines above the last one your design
        // reads, and so which of them show as sent (D7).
        let heads = block.heads_shown_with_text();
        let heads_moved = heads != self.open_heads;
        let (body, live) = row_bodies(block, view);
        if body == open.body && live == open.live && !lifting && !heads_moved {
            // The same bytes can come from pieces numbered anew, such as
            // after an edit, so the row keeps the latest render's.
            let same = view.open_row(open.gen, Vec::new(), None);
            open.spans = same.spans;
            open.plain = same.plain;
            return;
        }
        let old = open.gen;
        let lift = match self.open_lift {
            // The lift starts again from the lines that show above the
            // region now, or from the region, so both renderers band them.
            Some(lift) if heads_moved => Some(OpenLift {
                start_inside: true,
                ..lift
            }),
            Some(lift) => Some(lift),
            None if lifting => Some(OpenLift {
                id: self.next_gen(),
                start_inside: true,
            }),
            None => None,
        };
        let gen = self.next_gen();
        let region = region_bytes(gen, lift, &body);
        // The lines above the region as they show now, from the first of
        // them: the lift's start, the lines, then the region.
        let whole = {
            let mut whole = Vec::new();
            if let Some(lift) = lift {
                whole.extend(lift_start(lift.id));
            }
            if let Some((bytes, _)) = &heads {
                whole.extend_from_slice(bytes);
            }
            whole.extend(mark(gen));
            match lift {
                Some(lift) => whole.extend(end_lift(&body, lift)),
                None => whole.extend_from_slice(&body),
            }
            whole
        };
        let (bytes, above) = match &self.open_heads {
            // Choosing Lifted lifts the lines above the region with it,
            // and an edit that reads other lines rewrites them. A renderer
            // finds the lines that showed by their text, erases them with
            // the region and writes them as they show now.
            Some((_, plain)) if lifting || heads_moved => (
                region,
                Some(Above {
                    plain: plain.clone(),
                    bytes: whole,
                }),
            ),
            // No line showed above the region, so the lines that show now
            // go at its start.
            None if heads_moved => (whole, None),
            _ => (region, None),
        };
        out.replace_above(old, bytes, above);
        out.restore = live.as_ref().map(|live| region_bytes(gen, lift, live));
        out.closed = false;
        self.open = Some(view.open_row(gen, body, live));
        self.open_lift = lift;
        self.open_heads = heads;
    }

    /// What the band shows for `block` with `shown`: drawing off, what
    /// Prompts triggers left of the prompt it pinned.
    fn band_body(&self, block: &Block, shown: Option<&str>) -> Vec<u8> {
        let shown = shown.filter(|_| !block.afk);
        match (shown, &self.pinned_shown) {
            (None, Some(pinned)) => pinned.band.clone(),
            _ => pin_body(block, shown),
        }
    }

    /// Show the band as the `[prompt]` table now says. It never writes to
    /// the text, so it never races your echo. The same bytes go out again
    /// when the pieces in them are numbered anew, such as after an edit.
    fn repaint_pinned(&mut self, out: &mut Output, view: View) {
        let (Some(block), Some(pinned)) = (&self.last_raw, &self.pinned) else {
            return;
        };
        let body = self.band_body(block, view.shown);
        let spans = band_spans(block, view);
        if body == *pinned && spans == self.pinned_spans {
            return;
        }
        self.set_band(out, body, spans);
    }

    /// You chose Pinned. The open row, if any, is erased and its prompt
    /// goes to the band, and the next empty line writes nothing. Without
    /// one, the next prompt goes to the band.
    fn move_to_pinned(&mut self, out: &mut Output, view: View) {
        let Some(open) = self.open.take() else {
            self.shown_as = PromptShow::Pinned;
            return;
        };
        let Some(block) = &self.last_raw else {
            return;
        };
        let body = pin_body(block, view.shown.filter(|_| !block.afk));
        let spans = band_spans(block, view);
        let settled_line = block.final_line().end == End::SettledLine;
        // The lines above the region leave the text with it.
        let above = self.open_heads.take().map(|(_, plain)| Above {
            plain,
            bytes: Vec::new(),
        });
        out.replace_above(open.gen, Vec::new(), above);
        self.open_lift = None;
        self.swallow = (!settled_line).then(|| Swallow::at(out));
        out.row_open = self.swallow.is_some();
        self.set_band(out, body, spans);
        self.shown_as = PromptShow::Pinned;
    }

    /// You chose to show your prompt in the text again. The band empties.
    /// While the pinned prompt's row would still be the last thing on
    /// screen, the prompt comes back there, as a fresh open row after the
    /// held line ends. Otherwise the next prompt shows in the text.
    fn move_from_pinned(&mut self, out: &mut Output, view: View) {
        out.pin = Some(Vec::new());
        out.pin_spans = None;
        self.pinned = None;
        self.pinned_spans = None;
        let pinned_shown = self.pinned_shown.take();
        self.shown_as = self.show;
        if self.swallow.take().is_none() {
            return;
        }
        let Some(block) = self.last_raw.clone() else {
            return;
        };
        let drawing = view.shown.is_some() || view.live.is_some();
        match (drawing && !block.afk).then_some(view) {
            Some(view) => {
                let lift = self.lifts_view(&view).then(|| OpenLift {
                    id: self.next_gen(),
                    start_inside: false,
                });
                let gen = self.next_gen();
                let (body, live) = row_bodies(&block, view);
                let region = region_bytes(gen, lift, &body);
                let mut bytes = Vec::new();
                if let Some(lift) = lift {
                    bytes.extend(lift_start(lift.id));
                }
                bytes.extend(block.heads_shown());
                let len = region.len();
                bytes.extend(region);
                out.text(&bytes);
                if let Some(live) = &live {
                    out.preview_tail(len, region_bytes(gen, lift, live));
                }
                out.closed = false;
                self.open = Some(view.open_row(gen, body, live));
                self.open_since = out.id.0;
                self.open_lift = lift;
                self.open_heads = block.heads_shown_with_text();
            }
            None => {
                // As Prompts triggers left it, when it pinned drawing off.
                let shown = match pinned_shown {
                    Some(shown) => shown.text,
                    None => {
                        let mut shown = block.heads();
                        let last = block.final_line();
                        shown.extend_from_slice(&last.raw);
                        shown.extend_from_slice(last.end.terminator());
                        shown
                    }
                };
                let bytes = self.lift_shown(shown);
                out.text(&bytes);
            }
        }
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

    /// Note that `trigger` hid a line and set prompt values while nothing
    /// reads your prompt, so Vosh drew nothing in its place. True the
    /// first time this session, when the session tells the webview.
    pub fn gag_without_reader(&mut self, trigger: &str) -> bool {
        self.gag_reported.insert(trigger.to_string())
    }

    /// Start the list of triggers that hid a prompt with nothing reading
    /// it over, as a profile that reads one or another profile does.
    pub fn forget_gags_without_reader(&mut self) {
        self.gag_reported.clear();
    }

    /// The triggers that hid a prompt this session while nothing read
    /// it, in name order.
    pub fn gags_without_reader(&self) -> impl Iterator<Item = &str> {
        self.gag_reported.iter().map(String::as_str)
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

/// What the open row holds for `block` with `view`: your design as drawn,
/// or the lines it replaced as the game sent them, and the live render
/// when the row shows something else. A live render that draws the same
/// bytes needs no restore.
fn row_bodies(block: &Block, view: View) -> (Vec<u8>, Option<Vec<u8>>) {
    let body = match view.shown {
        Some(shown) => drawn(block, shown),
        None => block.shown(),
    };
    let live = view
        .live
        .map(|live| drawn(block, live))
        .filter(|live| *live != body);
    (body, live)
}

/// Region `gen` holding `body`: its mark, then, while it carries a lift,
/// the lift's start when it sits inside the region, the body and the
/// lift's end.
fn region_bytes(gen: u64, lift: Option<OpenLift>, body: &[u8]) -> Vec<u8> {
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
fn end_lift(body: &[u8], lift: OpenLift) -> Vec<u8> {
    with_lift_end(body, lift.id)
}

/// What the band shows for `block`: the lines above the last one that
/// show as sent, then `rendered`, or with drawing off every line as the
/// game sent it. No line end after the last row.
fn pin_body(block: &Block, rendered: Option<&str>) -> Vec<u8> {
    match rendered {
        Some(rendered) => {
            let mut body = block.heads_shown();
            body.extend_from_slice(rendered.as_bytes());
            trim_line_end(body)
        }
        None => {
            let mut body = block.heads();
            body.extend_from_slice(&block.final_line().raw);
            trim_line_end(body)
        }
    }
}

/// Where each piece of the design landed on the band for `block` with
/// `view`, its rows moved down past the lines above the last one that show
/// as sent, which the band shows first. None when the band shows no
/// design: drawing off, the game's own line, or the away prompt.
fn band_spans(block: &Block, view: View) -> Option<Vec<Span>> {
    if view.shown.is_none() || block.afk {
        return None;
    }
    let heads = block.heads_shown_rows();
    Some(
        view.spans
            .iter()
            .map(|span| Span {
                row: span.row + heads,
                ..span.clone()
            })
            .collect(),
    )
}

/// `bytes` without the line ends at its very end.
fn trim_line_end(mut bytes: Vec<u8>) -> Vec<u8> {
    while bytes.last().is_some_and(|&b| b == b'\n' || b == b'\r') {
        bytes.pop();
    }
    bytes
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
mod tests;
