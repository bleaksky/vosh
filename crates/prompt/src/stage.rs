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

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::aabahran::Who;
use crate::capture::{Recognized, Recognizer};
use crate::config::{CaptureConfig, PromptShow};
use crate::render::Span;
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
    /// renderers keep every byte.
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

/// The lift the open row carries while your prompt shows lifted, and
/// whether its start mark sits inside the row's region, as it does when
/// you chose Lifted with the row open, so a repaint writes it again.
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
            ..Self::default()
        };
    }

    /// Take where your prompt shows from the `[prompt]` table. The prompt
    /// on screen moves at the next repaint.
    pub fn set_show(&mut self, show: PromptShow) {
        self.show = show;
    }

    /// Where your prompt shows.
    pub fn shows(&self) -> PromptShow {
        self.show
    }

    /// What the band shows, while your prompt shows pinned.
    pub fn pinned(&self) -> Option<&[u8]> {
        self.pinned.as_deref()
    }

    /// The next empty line writes nothing.
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

    /// The webview wrote to the terminal itself, such as your typed echo.
    /// It closes the open row and lands where a pinned prompt's row would
    /// have been, so the next empty line writes again.
    pub fn local_write(&mut self) {
        self.open = None;
        self.swallow = None;
    }

    /// Catch up with `out` before it goes out, so bytes written after
    /// the open row close it, and say whether a pinned prompt's row is
    /// still open after it. The session calls it at the end of every
    /// read.
    pub fn finish(&mut self, out: &mut Output) {
        self.sync(out);
        self.seal(out);
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
        let lift = self.lifts().then(|| OpenLift {
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
        put(out, painted, bytes, true);
        if let Some(live) = &live {
            out.preview_tail(len, region_bytes(gen, lift, live));
        }
        out.closed = false;
        self.open = Some(view.open_row(gen, body, live));
        self.open_lift = lift;
        self.open_heads = block.heads_shown_with_text();
        self.shown_as = self.show;
        self.note_recognized(block);
    }

    /// Your prompt shows lifted, so each one carries lift marks.
    fn lifts(&self) -> bool {
        self.show == PromptShow::Lifted
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
        write(out, &mut self.open, painted, bytes);
        self.shown_as = self.show;
        self.note_recognized(block);
    }

    /// Pin `block` drawn as `rendered`: it leaves the text, and the band
    /// shows the lines above the last one that show as sent, then the
    /// design. See [`Stage::pin`].
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

    /// [`Stage::pin_drawn`] with what the open card shows. The band shows
    /// it, and needs no restore, since nothing on the band reaches
    /// history. It shows the live render again at the next repaint
    /// without a preview.
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
        match painted {
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
        // The line end a pinned prompt's row would have taken, and any
        // empty line with it, writes nothing. It is still logged and kept.
        let swallowed = self.swallow.is_some()
            && painted.is_none()
            && plain.trim().is_empty()
            && !shows_anything(bytes);
        if !swallowed {
            write(out, &mut self.open, painted, bytes.to_vec());
        }
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
        self.repaint_view(out, View::live(rendered));
    }

    /// [`Stage::repaint`] with what the open card shows, as a preview the
    /// card sets or clears asks. In the text and lifted, the row carries
    /// the live render as its restore while it shows anything else, so a
    /// renderer writes the live render back before anything lands after
    /// it. The band shows the preview with no restore.
    pub fn repaint_view(&mut self, out: &mut Output, view: View) {
        self.repaint_row(out, view);
        self.seal(out);
    }

    /// True when a repaint with `view` would change what your prompt
    /// shows, the open row or the band, such as after a GMCP packet that
    /// arrived with no prompt after it. False while there is nothing to
    /// repaint, and while a change of where your prompt shows waits for
    /// its own repaint.
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
                body != open.body || live != open.live
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
        // Choosing Lifted lifts the open row, from its region's start.
        let lifting = self.open_lift.is_none() && self.lifts();
        let Some(open) = &mut self.open else {
            return;
        };
        let Some(block) = &self.last_raw else {
            return;
        };
        let (body, live) = row_bodies(block, view);
        if body == open.body && live == open.live && !lifting {
            // The same bytes can come from pieces numbered anew, such as
            // after an edit, so the row keeps the latest render's.
            let same = view.open_row(open.gen, Vec::new(), None);
            open.spans = same.spans;
            open.plain = same.plain;
            return;
        }
        let old = open.gen;
        let lift = match self.open_lift {
            Some(lift) => Some(lift),
            None if lifting => Some(OpenLift {
                id: self.next_gen(),
                start_inside: true,
            }),
            None => None,
        };
        let gen = self.next_gen();
        let bytes = region_bytes(gen, lift, &body);
        // Choosing Lifted lifts the lines above the region with it, from
        // the first of them, where a renderer finds them.
        let above = match (lifting, lift, &self.open_heads) {
            (true, Some(lift), Some((heads, plain))) => {
                let mut whole = lift_start(lift.id);
                whole.extend_from_slice(heads);
                whole.extend(mark(gen));
                whole.extend(with_lift_end(&body, lift.id));
                Some(Above {
                    plain: plain.clone(),
                    bytes: whole,
                })
            }
            _ => None,
        };
        out.replace_above(old, bytes, above);
        out.restore = live.as_ref().map(|live| region_bytes(gen, lift, live));
        out.closed = false;
        self.open = Some(view.open_row(gen, body, live));
        self.open_lift = lift;
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
                let lift = self.lifts().then(|| OpenLift {
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
            bytes.extend(with_lift_end(body, lift.id));
        }
        None => bytes.extend_from_slice(body),
    }
    bytes
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
                body: b"DRAWN".to_vec(),
                live: None,
                spans: Vec::new(),
                plain: String::new(),
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
                above: None,
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
                above: None,
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
                above: None,
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
                above: None,
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
                body: b"DRAWN\r\n".to_vec(),
                live: None,
                spans: Vec::new(),
                plain: String::new(),
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
                above: None,
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
                above: None,
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
                above: None,
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
                above: None,
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
                above: None,
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
                above: None,
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
                above: None,
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
        stage.finish(&mut out);
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
        stage.finish(&mut out);
        assert!(stage.open_row().is_some());

        // A prompt drawn after other output in the same read stays open.
        let mut out = Output::new(true);
        out.text(b"text\r\n");
        stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
        stage.finish(&mut out);
        assert!(stage.open_row().is_some());
    }

    /// A span of piece `piece` on the first row, `width` cells from
    /// `col`.
    fn span_at(piece: usize, col: usize, width: usize) -> Span {
        Span {
            piece,
            row: 0,
            col,
            width,
            fg: crate::render::SpanColor::Default,
            bg: crate::render::SpanColor::Default,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    /// `shown` with its pieces, as the session hands a render over.
    fn drawn_view<'a>(shown: &'a str, spans: &'a [Span]) -> View<'a> {
        View {
            shown: Some(shown),
            spans,
            plain: shown,
            ..View::default()
        }
    }

    #[test]
    fn the_open_row_keeps_the_pieces_of_the_render_that_drew_it() {
        let one = [span_at(0, 0, 5)];
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw_view(&mut out, block, None, b"", drawn_view("DRAWN", &one));
        let open = stage.open_row().expect("the row");
        assert_eq!(
            (open.spans.clone(), open.plain.as_str()),
            (one.to_vec(), "DRAWN")
        );

        // A repaint that draws the same bytes keeps them, and takes the
        // latest render's pieces, since an edit can number them anew.
        let renumbered = [span_at(0, 0, 2), span_at(1, 2, 3)];
        let mut same = Output::new(false);
        stage.repaint_view(&mut same, drawn_view("DRAWN", &renumbered));
        assert!(same.is_empty());
        let open = stage.open_row().expect("the row");
        assert_eq!(open.spans, renumbered);

        // Another design brings its own.
        let new = [span_at(0, 0, 3)];
        let mut out = Output::new(false);
        stage.repaint_view(&mut out, drawn_view("NEW", &new));
        let open = stage.open_row().expect("the row");
        assert_eq!(
            (open.spans.clone(), open.plain.as_str()),
            (new.to_vec(), "NEW")
        );

        // The game's own line has none, drawing off or under the card.
        let mut off = Output::new(false);
        stage.repaint_view(&mut off, View::live(None));
        let open = stage.open_row().expect("the row");
        assert!(open.spans.is_empty() && open.plain.is_empty());
        let mut raw = Output::new(false);
        stage.repaint_view(
            &mut raw,
            View {
                live: Some("NEW"),
                spans: &new,
                plain: "NEW",
                ..View::default()
            },
        );
        let open = stage.open_row().expect("the row");
        assert!(open.spans.is_empty() && open.plain.is_empty());

        // A prompt that comes back from the band brings them too.
        let mut stage = self::stage(JAMES, false);
        stage.set_show(PromptShow::Pinned);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.pin_view(&mut out, block, None, b"", drawn_view("DRAWN", &one));
        stage.finish(&mut out);
        stage.set_show(PromptShow::Text);
        let mut back = Output::new(false);
        stage.repaint_view(&mut back, drawn_view("DRAWN", &one));
        let open = stage.open_row().expect("the row");
        assert_eq!(open.spans, one);
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
                id: 1,
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
                id: 2,
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
                id: 3,
                raw: b"Healer> ".to_vec(),
                plain: "Healer> ".to_string(),
                at_ms: 30,
                recognized: false,
                draw: false,
                capture: false,
            }
        );
        assert_eq!(ring.len(), 3);
        assert_eq!(stage.candidate(2).map(|c| c.at_ms), Some(20));
        assert_eq!(stage.candidate(4), None);

        // A new connection starts the ring over and keeps counting, so
        // an id from before never names a new entry.
        stage.reset();
        stage.line(&mut out, b"> ", "> ", None, b"> \r\n");
        stage.record(None, 40, false, true);
        assert_eq!(stage.ring().map(|c| c.id).collect::<Vec<_>>(), [4]);
        assert_eq!(stage.candidate(1), None);
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

    /// A stage that reads JAMES and pins your prompt.
    fn pinned_stage() -> Stage {
        let mut stage = stage(JAMES, false);
        stage.set_show(PromptShow::Pinned);
        stage
    }

    /// Pin the prompt as the session does with drawing on.
    fn pin_prompt(stage: &mut Stage, out: &mut Output) {
        let block = read(stage, PROMPT, End::Line);
        stage.pin_drawn(out, block, None, b"", "DRAWN");
    }

    #[test]
    fn the_line_ends_a_text_ends_on_are_found_after_its_last_visible_character() {
        assert_eq!(trailing_line_ends(b"room\r\n\r\n"), 4);
        assert_eq!(trailing_line_ends(b"room\x1b[0m\r\n\x1b[0m\r\n"), 4);
        assert_eq!(trailing_line_ends(b"room"), 4);
        assert_eq!(
            trailing_line_ends(b"room\x1b[0m"),
            8,
            "no line end, no hold"
        );
        assert_eq!(trailing_line_ends(b"\r\n"), 0);
        assert_eq!(trailing_line_ends(b""), 0);
        // A mark stops the run, so a region keeps its start.
        let mut marked = b"a\r\n".to_vec();
        marked.extend(mark(3));
        marked.extend_from_slice(b"\r\n");
        assert_eq!(trailing_line_ends(&marked), marked.len() - 2);
        assert!(shows_anything(b"a"));
        assert!(!shows_anything(b"\r\n \x1b[0m\x1b]7717;o;4\x07"));
    }

    #[test]
    fn a_pinned_prompt_leaves_the_text_and_holds_the_line_ends_before_it() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"The Bank of Aabahran\r\n[Exits: south]\r\n");
        stage.line(&mut out, b"", "", None, b"\r\n");
        pin_prompt(&mut stage, &mut out);
        assert_eq!(out.bytes, b"The Bank of Aabahran\r\n[Exits: south]");
        assert_eq!(out.hold, b"\r\n\r\n");
        assert_eq!(out.pin.as_deref(), Some(&b"DRAWN"[..]));
        assert_eq!(out.replace, None);
        assert_eq!(stage.open_row(), None);
        assert_eq!(stage.pinned(), Some(&b"DRAWN"[..]));
        assert!(stage.swallows());
        stage.finish(&mut out);

        // The next unasked text starts with an empty line, which writes
        // nothing, then lands where the prompt's row was.
        let mut next = Output::new(false);
        stage.line(&mut next, b"", "", None, b"\r\n");
        assert!(next.bytes.is_empty() && next.hold.is_empty());
        stage.line(
            &mut next,
            b"Tarvik tells you 'hi'",
            "Tarvik tells you 'hi'",
            None,
            b"Tarvik tells you 'hi'\r\n",
        );
        stage.line(&mut next, b"", "", None, b"\r\n");
        pin_prompt(&mut stage, &mut next);
        assert_eq!(next.bytes, b"Tarvik tells you 'hi'");
        assert_eq!(next.hold, b"\r\n\r\n");
        // Once text lands, the next empty line writes again.
        let mut more = Output::new(false);
        stage.line(&mut more, b"", "", None, b"\r\n");
        assert!(more.bytes.is_empty(), "the second pin armed it again");
    }

    #[test]
    fn two_pins_in_one_read_keep_the_hold_at_the_end_of_the_output() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        assert_eq!(out.hold, b"\r\n\r\n");
        // The next pulse in the same read.
        stage.line(&mut out, b"", "", None, b"\r\n");
        stage.line(&mut out, b"tell", "tell", None, b"tell\r\n");
        assert_eq!(
            out.bytes, b"room\r\n\r\ntell\r\n",
            "the hold went back first"
        );
        assert!(out.hold.is_empty());
        stage.line(&mut out, b"", "", None, b"\r\n");
        pin_prompt(&mut stage, &mut out);
        assert_eq!(out.bytes, b"room\r\n\r\ntell");
        assert_eq!(out.hold, b"\r\n\r\n");
        // A region painted after a pin takes the hold back too.
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        let _ = stage.paint_partial(&mut out, b"<10", None);
        assert!(out.hold.is_empty());
        assert!(out.bytes.starts_with(b"room\r\n\r\n\x1b]7717;o;"));
    }

    #[test]
    fn the_end_of_a_read_keeps_the_swallow_however_often_it_is_told() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        stage.end_read(&mut out);
        stage.finish(&mut out);
        stage.finish(&mut out);
        assert!(stage.swallows());
        let mut next = Output::new(false);
        stage.line(&mut next, b"", "", None, b"\r\n");
        assert!(next.bytes.is_empty());
        // Text in a later output ends it, whoever wrote it.
        next.text(b"echo\r\n");
        stage.finish(&mut next);
        assert!(!stage.swallows());
    }

    #[test]
    fn a_pulse_of_hidden_lines_moves_nothing() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        let mut next = Output::new(false);
        stage.line(&mut next, b"", "", None, b"\r\n");
        stage.line(&mut next, b"spam", "spam", None, b"");
        stage.line(&mut next, b"", "", None, b"\r\n");
        pin_prompt(&mut stage, &mut next);
        assert!(next.bytes.is_empty());
        assert!(next.hold.is_empty());
        assert_eq!(next.pin.as_deref(), Some(&b"DRAWN"[..]));
        assert!(!next.is_empty(), "the band still changes");
    }

    #[test]
    fn enter_on_an_empty_line_moves_nothing_and_updates_the_band() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        // A send leaves it armed.
        stage.close();
        let mut next = Output::new(false);
        stage.line(&mut next, b"", "", None, b"\r\n");
        let block = read(&stage, PROMPT, End::Line);
        stage.pin_drawn(&mut next, block, None, b"", "NEW");
        assert!(next.bytes.is_empty() && next.hold.is_empty());
        assert_eq!(next.pin.as_deref(), Some(&b"NEW"[..]));
        assert!(!next.is_empty());
    }

    #[test]
    fn a_local_write_and_other_output_end_the_swallow() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        stage.local_write();
        let mut next = Output::new(false);
        stage.line(&mut next, b"", "", None, b"\r\n");
        assert_eq!(next.bytes, b"\r\n");

        let mut out = Output::new(false);
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        let mut next = Output::new(true);
        stage.line(&mut next, b"", "", None, b"\r\n");
        assert_eq!(next.bytes, b"\r\n");

        // Text a script echoed in the same read ends it too.
        let mut out = Output::new(false);
        pin_prompt(&mut stage, &mut out);
        out.text(b"echo\r\n");
        stage.line(&mut out, b"", "", None, b"\r\n");
        assert!(out.bytes.ends_with(b"echo\r\n\r\n"));

        // Output from elsewhere that came before the read the prompt
        // pinned in came before the prompt, so it ends nothing.
        let mut out = Output::new(true);
        out.text(b"tell\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        assert!(stage.swallows());
        assert_eq!(out.pin_row, Some(true));
    }

    #[test]
    fn a_prompt_that_took_its_line_end_arms_nothing() {
        let mut stage = stage(SETTLES, true);
        stage.set_show(PromptShow::Pinned);
        let block = read(&stage, "<10hp> ", End::Line);
        assert_eq!(block.final_line().end, End::SettledLine);
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        stage.pin_drawn(&mut out, block, None, b"", "DRAWN");
        assert!(!stage.swallows());
        assert_eq!(
            out.pin.as_deref(),
            Some(&b"DRAWN"[..]),
            "no line end on the band"
        );
        stage.line(&mut out, b"arrives", "arrives", None, b"arrives\r\n");
        assert_eq!(out.bytes, b"room\r\n\r\narrives\r\n");
    }

    #[test]
    fn a_painted_start_of_a_pinned_prompt_is_erased_unless_closed() {
        let mut stage = pinned_stage();
        let mut first = Output::new(false);
        let painted = stage.paint_partial(&mut first, b"[1020/1020hp 800", None);
        let mut second = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.pin_drawn(&mut second, block, painted.map(|(g, _)| g), b"", "DRAWN");
        assert_eq!(
            second.replace,
            Some(Replace {
                gen: 1,
                bytes: Vec::new(),
                fresh: false,
                above: None,
            })
        );
        assert!(second.bytes.is_empty());
        // Echoes a Prompts trigger wrote take the painted region's place,
        // line end and all, since a replace is written whole.
        let mut third = Output::new(false);
        let painted = stage.paint_partial(&mut third, b"[1020", None);
        let mut fourth = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.pin_drawn(
            &mut fourth,
            block,
            painted.map(|(g, _)| g),
            b"low on mana\r\n",
            "DRAWN",
        );
        assert_eq!(
            fourth.replace,
            Some(Replace {
                gen: 2,
                bytes: b"low on mana\r\n".to_vec(),
                fresh: true,
                above: None,
            })
        );
        // After other output the painted start is closed, so it stays.
        let mut fifth = Output::new(false);
        let painted = stage.paint_partial(&mut fifth, b"[1020", None);
        let mut sixth = Output::new(false);
        sixth.text(b"The moon rises.\r\n");
        let block = read(&stage, PROMPT, End::Line);
        stage.pin_drawn(&mut sixth, block, painted.map(|(g, _)| g), b"", "DRAWN");
        assert_eq!(sixth.replace, None);
        assert_eq!(sixth.bytes, b"The moon rises.");
        assert_eq!(sixth.hold, b"\r\n");
    }

    #[test]
    fn a_pinned_prompt_that_spans_lines_puts_every_line_on_the_band() {
        let stage = pinned_stage();
        let block = Block {
            lines: vec![
                BlockLine {
                    raw: b"Tester: [===|---]".to_vec(),
                    plain: "Tester: [===|---]".into(),
                    end: End::Line,
                },
                BlockLine {
                    raw: PROMPT.as_bytes().to_vec(),
                    plain: PROMPT.into(),
                    end: End::Line,
                },
            ],
            replaced: vec![1],
            values: BTreeMap::new(),
            afk: false,
        };
        let mut stage = stage;
        let mut out = Output::new(false);
        out.text(b"A guard has quite a few wounds.\r\n\r\n");
        stage.pin_drawn(&mut out, block.clone(), None, b"", "DRAWN");
        assert_eq!(out.pin.as_deref(), Some(&b"Tester: [===|---]\r\nDRAWN"[..]));
        assert_eq!(out.bytes, b"A guard has quite a few wounds.");
        // A design that reads the tank line takes it over.
        let over = Block {
            replaced: vec![0, 1],
            ..block.clone()
        };
        let mut out = Output::new(false);
        stage.pin_drawn(&mut out, over, None, b"", "Tank 75%\r\nDRAWN");
        assert_eq!(out.pin.as_deref(), Some(&b"Tank 75%\r\nDRAWN"[..]));
        // Drawing off, the band shows the lines as sent, and a trigger
        // that hid the last one leaves the lines above it.
        let mut out = Output::new(false);
        stage.pin_shown(
            &mut out,
            block.clone(),
            None,
            b"",
            Some(b"\x1b[31m[shown]\x1b[0m"),
        );
        assert_eq!(
            out.pin.as_deref(),
            Some(&b"Tester: [===|---]\r\n\x1b[31m[shown]\x1b[0m"[..])
        );
        let mut out = Output::new(false);
        stage.pin_shown(&mut out, block, None, b"", None);
        assert_eq!(out.pin.as_deref(), Some(&b"Tester: [===|---]"[..]));
    }

    #[test]
    fn a_repaint_while_pinned_changes_only_the_band() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        let mut repaint = Output::new(false);
        stage.repaint(&mut repaint, Some("NEW"));
        assert_eq!(repaint.pin.as_deref(), Some(&b"NEW"[..]));
        assert!(repaint.bytes.is_empty() && repaint.replace.is_none() && repaint.hold.is_empty());
        let mut same = Output::new(false);
        stage.repaint(&mut same, Some("NEW"));
        assert!(same.is_empty());
        let mut off = Output::new(false);
        stage.repaint(&mut off, None);
        assert_eq!(off.pin.as_deref(), Some(PROMPT.as_bytes()));
        // Your echo after it changes nothing about that.
        stage.local_write();
        let mut later = Output::new(false);
        stage.repaint(&mut later, Some("LATER"));
        assert_eq!(later.pin.as_deref(), Some(&b"LATER"[..]));
    }

    #[test]
    fn choosing_pinned_moves_the_open_row_to_the_band() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        stage.finish(&mut out);
        stage.set_show(PromptShow::Pinned);
        let mut moved = Output::new(false);
        stage.repaint(&mut moved, Some("DRAWN"));
        assert_eq!(
            moved.replace,
            Some(Replace {
                gen: 1,
                bytes: Vec::new(),
                fresh: false,
                above: None,
            })
        );
        assert_eq!(moved.pin.as_deref(), Some(&b"DRAWN"[..]));
        assert!(stage.swallows());
        assert_eq!(stage.open_row(), None);
        // With the row closed, the next prompt goes to the band.
        let mut stage = self::stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        stage.close();
        stage.set_show(PromptShow::Pinned);
        let mut moved = Output::new(false);
        stage.repaint(&mut moved, Some("DRAWN"));
        assert!(moved.is_empty());
    }

    #[test]
    fn leaving_pinned_brings_the_prompt_back_only_while_its_row_would_still_be_last() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        stage.set_show(PromptShow::Text);
        let mut back = Output::new(false);
        stage.repaint(&mut back, Some("DRAWN"));
        assert_eq!(back.pin.as_deref(), Some(&b""[..]), "the band empties");
        assert_eq!(back.bytes, with(&[&mark(1), b"DRAWN"]));
        assert_eq!(stage.open_row().map(|r| r.gen), Some(1));
        assert!(!stage.swallows());

        // After your echo the prompt stays off the text.
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        stage.local_write();
        stage.set_show(PromptShow::Text);
        let mut back = Output::new(false);
        stage.repaint(&mut back, Some("DRAWN"));
        assert_eq!(back.pin.as_deref(), Some(&b""[..]));
        assert!(back.bytes.is_empty());
        assert_eq!(stage.open_row(), None);
    }

    #[test]
    fn the_line_end_that_would_end_a_pinned_row_is_found_past_escapes() {
        let cut = |bytes: &[u8]| {
            let (rest, closed) = close_pin_row(bytes);
            (rest.into_owned(), closed)
        };
        assert_eq!(cut(b"\r\nTICK\r\n"), (b"TICK\r\n".to_vec(), true));
        assert_eq!(cut(b"\n"), (Vec::new(), true));
        // Colors before it stay, since they write nothing.
        assert_eq!(cut(b"\x1b[33m\r\nTICK"), (b"\x1b[33mTICK".to_vec(), true));
        // Text that shows first fills the row, so nothing goes.
        assert_eq!(cut(b"look\r\n"), (b"look\r\n".to_vec(), true));
        assert_eq!(cut(b" look"), (b" look".to_vec(), true));
        // Escapes alone leave the row open.
        assert_eq!(cut(b"\x1b[0m"), (b"\x1b[0m".to_vec(), false));
        assert_eq!(cut(b""), (Vec::new(), false));
    }

    #[test]
    fn a_framed_echo_after_a_pinned_prompt_takes_the_prompt_row() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        // A script echo in the same read, framed with a line end first to
        // end the prompt's row. That row is not in the text, so the line
        // end writes nothing and the echo takes the row.
        out.text(b"\r\nThe moon rises.\r\n");
        assert_eq!(out.bytes, b"room\r\n\r\nThe moon rises.\r\n");
        assert!(out.hold.is_empty());
        // Only the first one goes.
        out.text(b"\r\nThe sun sets.\r\n");
        assert_eq!(
            out.bytes,
            b"room\r\n\r\nThe moon rises.\r\n\r\nThe sun sets.\r\n"
        );
        // Swallowed empty lines leave the row open for the echo after them.
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        stage.line(&mut out, b"", "", None, b"\r\n");
        out.text(b"\r\nThe moon rises.\r\n");
        assert_eq!(out.bytes, b"room\r\n\r\nThe moon rises.\r\n");
    }

    #[test]
    fn each_pinned_output_says_whether_the_prompt_row_is_open() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        assert_eq!(out.pin_row, Some(true));
        // Text that lands closes it.
        let mut next = Output::new(false);
        stage.line(&mut next, b"", "", None, b"\r\n");
        stage.line(&mut next, b"tell", "tell", None, b"tell\r\n");
        stage.finish(&mut next);
        assert_eq!(next.pin_row, Some(false));
        // A prompt that took its line end leaves no row open.
        let mut settles = stage_settling_pinned();
        let block = read(&settles, "<10hp> ", End::Line);
        let mut out = Output::new(false);
        settles.pin_drawn(&mut out, block, None, b"", "DRAWN");
        settles.finish(&mut out);
        assert_eq!(out.pin_row, Some(false));
        // Leaving Pinned closes it with the band.
        let mut out = Output::new(false);
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        stage.set_show(PromptShow::Text);
        let mut back = Output::new(false);
        stage.repaint(&mut back, Some("DRAWN"));
        assert_eq!(back.pin_row, Some(false));
        // In the text no output says anything about it.
        let mut text = self::stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&text, PROMPT, End::Line);
        text.draw(&mut out, block, None, b"", "DRAWN");
        text.finish(&mut out);
        assert_eq!(out.pin_row, None);
        let mut again = Output::new(false);
        text.repaint(&mut again, Some("NEW"));
        assert_eq!(again.pin_row, None);
    }

    /// A stage whose capture settles, pinning your prompt.
    fn stage_settling_pinned() -> Stage {
        let mut stage = stage(SETTLES, true);
        stage.set_show(PromptShow::Pinned);
        stage
    }

    #[test]
    fn a_prompt_pinned_with_drawing_off_keeps_what_prompts_triggers_did() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        let block = read(&stage, PROMPT, End::Line);
        stage.pin_shown(&mut out, block, None, b"", Some(b"[1020/1020HITPOINTS]"));
        assert_eq!(out.pin.as_deref(), Some(&b"[1020/1020HITPOINTS]"[..]));
        stage.finish(&mut out);
        // A repaint of the band keeps the trigger's text.
        let mut again = Output::new(false);
        stage.repaint(&mut again, None);
        assert_eq!(again.pin, None);
        // Back in the text it shows as the trigger left it.
        stage.set_show(PromptShow::Text);
        let mut back = Output::new(false);
        stage.repaint(&mut back, None);
        assert_eq!(back.bytes, b"[1020/1020HITPOINTS]\r\n");
        // A trigger that hid it leaves nothing to bring back.
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.pin_shown(&mut out, block, None, b"", None);
        stage.finish(&mut out);
        let mut again = Output::new(false);
        stage.repaint(&mut again, None);
        assert_eq!(again.pin, None, "the band stays empty");
        stage.set_show(PromptShow::Text);
        let mut back = Output::new(false);
        stage.repaint(&mut back, None);
        assert!(back.bytes.is_empty());
    }

    #[test]
    fn in_the_text_nothing_is_held_or_pinned() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        assert!(out.hold.is_empty());
        assert_eq!(out.pin, None);
        assert!(!stage.swallows());
        stage.line(&mut out, b"", "", None, b"\r\n");
        assert!(out.bytes.ends_with(b"DRAWN\r\n"));
    }

    /// A stage that reads `pattern` and lifts your prompt.
    fn lifted_stage(pattern: &str, settle: bool) -> Stage {
        let mut stage = stage(pattern, settle);
        stage.set_show(PromptShow::Lifted);
        stage
    }

    #[test]
    fn a_lift_ends_after_the_last_visible_byte_and_keeps_your_echo_a_cell_away() {
        assert_eq!(lift_start(4), b"\x1b]7717;l;4\x07");
        assert_eq!(lift_end(4), b"\x1b]7717;e;4\x07");
        // A body that ends on a glyph gains one plain space.
        assert_eq!(
            with_lift_end(b"DRAWN", 4),
            with(&[b"DRAWN", &lift_end(4), b" "])
        );
        assert_eq!(
            with_lift_end(b"DRAWN\x1b[0m", 4),
            with(&[b"DRAWN\x1b[0m", &lift_end(4), b" "])
        );
        // One that ends on a space, or keeps a line end, gains nothing.
        assert_eq!(
            with_lift_end(b"<10hp> ", 4),
            with(&[b"<10hp> ", &lift_end(4)])
        );
        assert_eq!(
            with_lift_end(b"DRAWN\r\n", 4),
            with(&[b"DRAWN", &lift_end(4), b"\r\n"])
        );
        // The marks take no room when the text wraps.
        let marked = with(&[&lift_start(1), b"ab cd", &lift_end(1)]);
        let text = String::from_utf8(marked.clone()).unwrap();
        assert_eq!(crate::wrap::wrap_stream(&text, 5).as_bytes(), &marked[..]);
    }

    #[test]
    fn a_lifted_prompt_carries_its_marks_around_every_line_it_shows() {
        let mut stage = lifted_stage(JAMES, false);
        let mut out = Output::new(false);
        let block = Block {
            lines: vec![
                BlockLine {
                    raw: b"Tester: [===|---]".to_vec(),
                    plain: "Tester: [===|---]".into(),
                    end: End::Line,
                },
                BlockLine {
                    raw: PROMPT.as_bytes().to_vec(),
                    plain: PROMPT.into(),
                    end: End::Line,
                },
            ],
            replaced: vec![1],
            values: BTreeMap::new(),
            afk: false,
        };
        stage.draw(&mut out, block.clone(), None, b"echo\r\n", "DRAWN");
        // Echoes stay outside, the tank line shown as sent inside.
        assert_eq!(
            out.bytes,
            with(&[
                b"echo\r\n",
                &lift_start(1),
                b"Tester: [===|---]\r\n",
                &mark(2),
                b"DRAWN",
                &lift_end(1),
                b" "
            ])
        );
        assert_eq!(stage.open_row().map(|r| &r.body[..]), Some(&b"DRAWN"[..]));
        // A repaint rewrites the region with the same lift's end mark.
        let mut repaint = Output::new(false);
        stage.repaint(&mut repaint, Some("NEW> "));
        assert_eq!(
            repaint.replace.map(|r| r.bytes),
            Some(with(&[&mark(3), b"NEW> ", &lift_end(1)]))
        );
        // Drawing off, the repaint lifts the line as the game sent it.
        let mut off = Output::new(false);
        stage.repaint(&mut off, None);
        assert_eq!(
            off.replace.map(|r| r.bytes),
            Some(with(&[&mark(4), PROMPT.as_bytes(), &lift_end(1), b"\r\n"]))
        );
        // Shown as sent, the whole block sits between the marks, before
        // its line end.
        let mut shown = Output::new(false);
        stage.show(&mut shown, block, None, b"", Some(PROMPT.as_bytes()));
        assert_eq!(
            shown.bytes,
            with(&[
                &lift_start(5),
                b"Tester: [===|---]\r\n",
                PROMPT.as_bytes(),
                &lift_end(5),
                b"\r\n"
            ])
        );
    }

    #[test]
    fn a_prompt_whole_before_its_line_end_ends_its_lift_before_it() {
        let mut stage = lifted_stage(SETTLES, true);
        let block = read(&stage, "<10hp> ", End::Line);
        let mut out = Output::new(false);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        assert_eq!(
            out.bytes,
            with(&[&lift_start(1), &mark(2), b"DRAWN", &lift_end(1), b"\r\n"])
        );
        // A prompt shown as sent that keeps the cursor after it ends on
        // its own space.
        let block = read(&stage, "<10hp> ", End::Settled);
        let mut out = Output::new(false);
        stage.show(&mut out, block, None, b"", Some(b"<10hp> "));
        assert_eq!(out.bytes, with(&[&lift_start(3), b"<10hp> ", &lift_end(3)]));
    }

    #[test]
    fn choosing_lifted_lifts_the_open_row_at_once() {
        let mut stage = stage(JAMES, false);
        let block = read(&stage, PROMPT, End::Line);
        let mut out = Output::new(false);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        assert!(!out.bytes.windows(8).any(|w| w == b"7717;l;1"));
        stage.set_show(PromptShow::Lifted);
        let mut lift = Output::new(false);
        stage.repaint(&mut lift, Some("DRAWN"));
        assert_eq!(
            lift.replace.map(|r| r.bytes),
            Some(with(&[
                &mark(3),
                &lift_start(2),
                b"DRAWN",
                &lift_end(2),
                b" "
            ]))
        );
        // The start sits inside the region, so a repaint writes it again.
        let mut again = Output::new(false);
        stage.repaint(&mut again, Some("NEW"));
        assert_eq!(
            again.replace.map(|r| r.bytes),
            Some(with(&[
                &mark(4),
                &lift_start(2),
                b"NEW",
                &lift_end(2),
                b" "
            ]))
        );
        // Back to the text, the row keeps its marks, and nothing moves.
        stage.set_show(PromptShow::Text);
        let mut same = Output::new(false);
        stage.repaint(&mut same, Some("NEW"));
        assert!(same.is_empty());
    }

    /// A block with a tank line above the prompt, which the design does
    /// not read, so it shows as sent.
    fn tank_block() -> Block {
        Block {
            lines: vec![
                BlockLine {
                    raw: b"Tester: [===|---]".to_vec(),
                    plain: "Tester: [===|---]".into(),
                    end: End::Line,
                },
                BlockLine {
                    raw: PROMPT.as_bytes().to_vec(),
                    plain: PROMPT.into(),
                    end: End::Line,
                },
            ],
            replaced: vec![1],
            values: BTreeMap::new(),
            afk: false,
        }
    }

    #[test]
    fn the_band_carries_where_each_piece_of_the_design_landed_on_it() {
        // Two pieces on the design's first row and one on its second.
        let spans = [
            span_at(0, 0, 3),
            span_at(1, 3, 2),
            Span {
                row: 1,
                ..span_at(2, 0, 3)
            },
        ];
        let rows = |out: &Output| -> Option<Vec<(usize, usize, usize)>> {
            out.pin_spans
                .as_ref()
                .map(|spans| spans.iter().map(|s| (s.piece, s.row, s.col)).collect())
        };
        // The tank line shows as sent above the design, so each piece
        // sits a row lower on the band.
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        stage.pin_view(
            &mut out,
            tank_block(),
            None,
            b"",
            drawn_view("HP 1>\r\nMN9", &spans),
        );
        assert_eq!(
            out.pin.as_deref(),
            Some(&b"Tester: [===|---]\r\nHP 1>\r\nMN9"[..])
        );
        assert_eq!(rows(&out), Some(vec![(0, 1, 0), (1, 1, 3), (2, 2, 0)]));
        stage.finish(&mut out);

        // A repaint that shows the same band with the same pieces sends
        // nothing. Pieces numbered anew go out again, bytes and all.
        let mut same = Output::new(false);
        stage.repaint_view(&mut same, drawn_view("HP 1>\r\nMN9", &spans));
        assert_eq!(same.pin, None);
        let renumbered = [
            span_at(0, 0, 5),
            Span {
                row: 1,
                ..span_at(1, 0, 3)
            },
        ];
        let mut again = Output::new(false);
        stage.repaint_view(&mut again, drawn_view("HP 1>\r\nMN9", &renumbered));
        assert!(again.pin.is_some());
        assert_eq!(rows(&again), Some(vec![(0, 1, 0), (1, 2, 0)]));

        // Drawing off and the game's own line under the card show no
        // design, so the band carries no pieces.
        let mut off = Output::new(false);
        stage.repaint_view(&mut off, View::live(None));
        assert_eq!(
            off.pin.as_deref(),
            Some(&b"Tester: [===|---]\r\n[1020/1020hp 800/800mn 930/930mv]"[..])
        );
        assert_eq!(off.pin_spans, None);
        let mut raw = Output::new(false);
        stage.repaint_view(
            &mut raw,
            View {
                live: Some("HP 1>"),
                spans: &spans,
                plain: "HP 1>",
                ..View::default()
            },
        );
        assert_eq!(raw.pin, None, "the band already shows the game's lines");
        let mut on = Output::new(false);
        stage.repaint_view(&mut on, drawn_view("HP 2>", &spans[..2]));
        assert_eq!(rows(&on), Some(vec![(0, 1, 0), (1, 1, 3)]));

        // A prompt pinned with drawing off carries none.
        let mut shown = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.pin_shown(&mut shown, block, None, b"", Some(PROMPT.as_bytes()));
        assert!(shown.pin.is_some());
        assert_eq!(shown.pin_spans, None);

        // Choosing Pinned with the row open in the text sends the band
        // with its pieces, and leaving Pinned clears both.
        let mut stage = self::stage(JAMES, false);
        let mut out = Output::new(false);
        stage.draw_view(
            &mut out,
            tank_block(),
            None,
            b"",
            drawn_view("HP 1>", &spans[..2]),
        );
        stage.finish(&mut out);
        stage.set_show(PromptShow::Pinned);
        let mut pin = Output::new(false);
        stage.repaint_view(&mut pin, drawn_view("HP 1>", &spans[..2]));
        assert_eq!(rows(&pin), Some(vec![(0, 1, 0), (1, 1, 3)]));
        stage.finish(&mut pin);
        stage.set_show(PromptShow::Text);
        let mut back = Output::new(false);
        stage.repaint_view(&mut back, drawn_view("HP 1>", &spans[..2]));
        assert_eq!(back.pin.as_deref(), Some(&b""[..]));
        assert_eq!(back.pin_spans, None);
    }

    #[test]
    fn a_change_of_place_takes_the_tank_line_above_the_region_along() {
        assert!(shows_lines(
            &["Tester: [===|".into(), "---]".into()],
            "Tester: [===|---]"
        ));
        assert!(!shows_lines(&["Tester: [===|".into()], "Tester: [===|---]"));
        assert!(!shows_lines(&[], ""));

        // In the text, then Pinned: the tank line goes with the region.
        let mut stage = self::stage(JAMES, false);
        let mut out = Output::new(false);
        stage.draw(&mut out, tank_block(), None, b"", "DRAWN");
        assert_eq!(
            out.bytes,
            with(&[b"Tester: [===|---]\r\n", &mark(1), b"DRAWN"])
        );
        stage.set_show(PromptShow::Pinned);
        let mut pin = Output::new(false);
        stage.repaint(&mut pin, Some("DRAWN"));
        assert_eq!(
            pin.replace,
            Some(Replace {
                gen: 1,
                bytes: Vec::new(),
                fresh: false,
                above: Some(Above {
                    plain: "Tester: [===|---]".into(),
                    bytes: Vec::new(),
                }),
            })
        );
        assert_eq!(pin.pin.as_deref(), Some(&b"Tester: [===|---]\r\nDRAWN"[..]));

        // In the text, then Lifted: the lift starts at the tank line when
        // a renderer finds it, and at the region otherwise.
        let mut stage = self::stage(JAMES, false);
        let mut out = Output::new(false);
        stage.draw(&mut out, tank_block(), None, b"", "DRAWN");
        stage.set_show(PromptShow::Lifted);
        let mut lift = Output::new(false);
        stage.repaint(&mut lift, Some("DRAWN"));
        let replace = lift.replace.expect("the repaint");
        assert_eq!(
            replace.bytes,
            with(&[&mark(3), &lift_start(2), b"DRAWN", &lift_end(2), b" "])
        );
        assert_eq!(
            replace.above,
            Some(Above {
                plain: "Tester: [===|---]".into(),
                bytes: with(&[
                    &lift_start(2),
                    b"Tester: [===|---]\r\n",
                    &mark(3),
                    b"DRAWN",
                    &lift_end(2),
                    b" "
                ]),
            })
        );
        // A later repaint rewrites only the region.
        let mut again = Output::new(false);
        stage.repaint(&mut again, Some("NEW"));
        assert_eq!(again.replace.and_then(|r| r.above), None);
        // With no lines above, nothing rides along.
        let mut stage = stage_settling_pinned();
        stage.set_show(PromptShow::Text);
        let block = read(&stage, "<10hp> ", End::Line);
        let mut out = Output::new(false);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        stage.set_show(PromptShow::Pinned);
        let mut pin = Output::new(false);
        stage.repaint(&mut pin, Some("DRAWN"));
        assert_eq!(pin.replace.and_then(|r| r.above), None);
    }

    #[test]
    fn leaving_pinned_for_lifted_brings_the_prompt_back_lifted() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        stage.set_show(PromptShow::Lifted);
        let mut back = Output::new(false);
        stage.repaint(&mut back, Some("DRAWN"));
        assert_eq!(
            back.bytes,
            with(&[&lift_start(1), &mark(2), b"DRAWN", &lift_end(1), b" "])
        );
    }

    /// What the open row shows with a preview on, and the live render
    /// behind it.
    fn preview<'a>(shown: &'a str, live: &'a str) -> View<'a> {
        View {
            shown: Some(shown),
            live: Some(live),
            ..View::default()
        }
    }

    #[test]
    fn a_prompt_drawn_with_a_preview_carries_the_live_render_as_its_restore() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE"));
        assert_eq!(out.bytes, with(&[&mark(1), b"LOW"]));
        assert_eq!(out.restore, Some(with(&[&mark(1), b"LIVE"])));
        let open = stage.open_row().expect("the open row");
        assert_eq!(open.body, b"LOW");
        assert_eq!(open.live.as_deref(), Some(&b"LIVE"[..]));
        // A preview that draws what the live render draws needs none.
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw_view(&mut out, block, None, b"", preview("SAME", "SAME"));
        assert_eq!(out.restore, None);
        assert_eq!(stage.open_row().and_then(|o| o.live.clone()), None);
    }

    #[test]
    fn anything_written_after_a_preview_in_the_same_output_puts_the_live_render_back_first() {
        // A line after the prompt in the same read.
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE"));
        stage.line(&mut out, b"You flee!", "You flee!", None, b"You flee!\r\n");
        assert_eq!(out.bytes, with(&[&mark(1), b"LIVE", b"You flee!\r\n"]));
        assert_eq!(out.restore, None);
        stage.finish(&mut out);
        assert_eq!(stage.open_row(), None);

        // Two prompts in one read: only the last one shows the preview.
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE"));
        out.text(b"\r\n");
        let block = read(&stage, PROMPT, End::Line);
        stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE2"));
        assert_eq!(
            out.bytes,
            with(&[&mark(2), b"LIVE", b"\r\n", &mark(3), b"LOW"])
        );
        assert_eq!(out.restore, Some(with(&[&mark(3), b"LIVE2"])));

        // A prompt that replaced its painted start, then a line.
        let mut stage = self::stage(JAMES, false);
        let mut first = Output::new(false);
        let _ = stage.paint_partial(&mut first, b"[1020/10", None);
        let mut second = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw_view(&mut second, block, Some(1), b"", preview("LOW", "LIVE"));
        assert_eq!(
            second.replace.as_ref().map(|r| r.bytes.clone()),
            Some(with(&[&mark(2), b"LOW"]))
        );
        assert_eq!(second.restore, Some(with(&[&mark(2), b"LIVE"])));
        second.text(b"\r\nThe guard arrives.\r\n");
        assert_eq!(
            second.replace.map(|r| r.bytes),
            Some(with(&[&mark(2), b"LIVE"]))
        );
        assert_eq!(second.restore, None);

        // The game's own line in the region ends its row, the live render
        // does not, so a fresh write after it starts a new row.
        let mut stage = self::stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        let raw = View {
            shown: None,
            live: Some("LIVE"),
            ..View::default()
        };
        stage.draw_view(&mut out, block, None, b"", raw);
        assert_eq!(out.bytes, with(&[&mark(1), PROMPT.as_bytes(), b"\r\n"]));
        out.replace(9, b"later".to_vec(), true);
        assert_eq!(out.bytes, with(&[&mark(1), b"LIVE\r\nlater"]));
        assert_eq!(out.restore, None);
    }

    #[test]
    fn a_preview_repaints_the_open_row_with_the_live_render_behind_it() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "LIVE");
        assert_eq!(out.restore, None);

        let mut low = Output::new(false);
        stage.repaint_view(&mut low, preview("LOW", "LIVE"));
        assert_eq!(
            low.replace,
            Some(Replace {
                gen: 1,
                bytes: with(&[&mark(2), b"LOW"]),
                fresh: false,
                above: None,
            })
        );
        assert!(low.bytes.is_empty());
        assert_eq!(low.restore, Some(with(&[&mark(2), b"LIVE"])));
        // The same view again writes nothing.
        let mut same = Output::new(false);
        stage.repaint_view(&mut same, preview("LOW", "LIVE"));
        assert!(same.is_empty());
        // The live render behind the preview moved, so the row carries
        // the new one.
        let mut moved = Output::new(false);
        stage.repaint_view(&mut moved, preview("LOW", "LIVE2"));
        assert_eq!(
            moved.replace.map(|r| r.bytes),
            Some(with(&[&mark(3), b"LOW"]))
        );
        assert_eq!(moved.restore, Some(with(&[&mark(3), b"LIVE2"])));
        // The card closes: the live render, with nothing to restore.
        let mut live = Output::new(false);
        stage.repaint_view(&mut live, View::live(Some("LIVE2")));
        assert_eq!(
            live.replace.map(|r| r.bytes),
            Some(with(&[&mark(4), b"LIVE2"]))
        );
        assert_eq!(live.restore, None);
        assert_eq!(stage.open_row().and_then(|o| o.live.clone()), None);
        // A preview that matches the live render after all writes it with
        // nothing to restore, so a restore a renderer still holds goes.
        let mut stage = self::stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE"));
        let mut back = Output::new(false);
        stage.repaint_view(&mut back, View::live(Some("LOW")));
        assert_eq!(
            back.replace.map(|r| r.bytes),
            Some(with(&[&mark(2), b"LOW"]))
        );
        assert_eq!(back.restore, None);
    }

    #[test]
    fn while_the_card_reads_your_codes_the_row_shows_the_game_line_over_your_design() {
        let mut stage = stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        let mut raw = Output::new(false);
        stage.repaint_view(
            &mut raw,
            View {
                shown: None,
                live: Some("DRAWN"),
                ..View::default()
            },
        );
        assert_eq!(
            raw.replace.map(|r| r.bytes),
            Some(with(&[&mark(2), PROMPT.as_bytes(), b"\r\n"]))
        );
        assert_eq!(raw.restore, Some(with(&[&mark(2), b"DRAWN"])));
    }

    #[test]
    fn a_lifted_preview_and_its_restore_both_end_with_the_lift() {
        let mut stage = lifted_stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        let mut low = Output::new(false);
        stage.repaint_view(&mut low, preview("LOW", "DRAWN"));
        assert_eq!(
            low.replace.map(|r| r.bytes),
            Some(with(&[&mark(3), b"LOW", &lift_end(1), b" "]))
        );
        assert_eq!(
            low.restore,
            Some(with(&[&mark(3), b"DRAWN", &lift_end(1), b" "]))
        );
        // A prompt drawn while the preview lasts shows it, and puts the
        // live render back when text follows.
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw_view(&mut out, block, None, b"", preview("LOW", "DRAWN"));
        assert_eq!(
            out.bytes,
            with(&[&lift_start(4), &mark(5), b"LOW", &lift_end(4), b" "])
        );
        assert_eq!(
            out.restore,
            Some(with(&[&mark(5), b"DRAWN", &lift_end(4), b" "]))
        );
        out.text(b"\r\nmore\r\n");
        assert_eq!(
            out.bytes,
            with(&[
                &lift_start(4),
                &mark(5),
                b"DRAWN",
                &lift_end(4),
                b" ",
                b"\r\nmore\r\n"
            ])
        );
        // A lift that starts inside the region starts again in both.
        let mut stage = self::stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        stage.set_show(PromptShow::Lifted);
        let mut lift = Output::new(false);
        stage.repaint(&mut lift, Some("DRAWN"));
        let mut low = Output::new(false);
        stage.repaint_view(&mut low, preview("LOW", "DRAWN"));
        assert_eq!(
            low.replace.map(|r| r.bytes),
            Some(with(&[
                &mark(4),
                &lift_start(2),
                b"LOW",
                &lift_end(2),
                b" "
            ]))
        );
        assert_eq!(
            low.restore,
            Some(with(&[
                &mark(4),
                &lift_start(2),
                b"DRAWN",
                &lift_end(2),
                b" "
            ]))
        );
    }

    #[test]
    fn a_pinned_preview_changes_only_the_band() {
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        out.text(b"room\r\n\r\n");
        let block = read(&stage, PROMPT, End::Line);
        stage.pin_view(&mut out, block, None, b"", preview("LOW", "DRAWN"));
        assert_eq!(out.pin.as_deref(), Some(&b"LOW"[..]));
        assert_eq!(out.restore, None);
        assert_eq!(out.bytes, b"room");
        stage.finish(&mut out);
        // The card closes, and the band shows the live render.
        let mut live = Output::new(false);
        stage.repaint_view(&mut live, View::live(Some("DRAWN")));
        assert_eq!(live.pin.as_deref(), Some(&b"DRAWN"[..]));
        assert!(live.replace.is_none() && live.bytes.is_empty() && live.restore.is_none());
        // A preview on the band needs no restore either.
        let mut low = Output::new(false);
        stage.repaint_view(&mut low, preview("LOW", "DRAWN"));
        assert_eq!(low.pin.as_deref(), Some(&b"LOW"[..]));
        assert!(low.replace.is_none() && low.bytes.is_empty() && low.restore.is_none());
        // The game's own line while the card reads your codes.
        let mut raw = Output::new(false);
        stage.repaint_view(
            &mut raw,
            View {
                shown: None,
                live: Some("DRAWN"),
                ..View::default()
            },
        );
        assert_eq!(raw.pin.as_deref(), Some(PROMPT.as_bytes()));
        assert!(raw.restore.is_none());
    }

    #[test]
    fn a_repaint_is_due_only_when_it_would_change_what_your_prompt_shows() {
        let mut stage = stage(JAMES, false);
        assert!(!stage.stale(View::live(Some("NEW"))), "nothing drawn yet");
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        stage.finish(&mut out);
        assert!(!stage.stale(View::live(Some("DRAWN"))));
        assert!(stage.stale(View::live(Some("NEW"))));
        assert!(stage.stale(View::live(None)), "drawing off");
        // The live render behind a preview counts too.
        assert!(stage.stale(preview("DRAWN", "LIVE")));
        let mut low = Output::new(false);
        stage.repaint_view(&mut low, preview("LOW", "DRAWN"));
        assert!(!stage.stale(preview("LOW", "DRAWN")));
        assert!(stage.stale(preview("LOW", "DRAWN2")));
        // A closed row needs nothing.
        stage.close();
        assert!(!stage.stale(View::live(Some("NEW"))));

        // The band, while pinned, whether or not text came after it.
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        pin_prompt(&mut stage, &mut out);
        stage.finish(&mut out);
        assert!(!stage.stale(View::live(Some("DRAWN"))));
        assert!(stage.stale(View::live(Some("NEW"))));
        let mut later = Output::new(false);
        later.text(b"\r\nA guard arrives.\r\n");
        stage.finish(&mut later);
        assert!(stage.stale(View::live(Some("NEW"))));
        // A change of where your prompt shows waits for its own repaint.
        stage.set_show(PromptShow::Text);
        assert!(!stage.stale(View::live(Some("NEW"))));
        let mut stage = self::stage(JAMES, false);
        let mut out = Output::new(false);
        let block = read(&stage, PROMPT, End::Line);
        stage.draw(&mut out, block, None, b"", "DRAWN");
        stage.set_show(PromptShow::Lifted);
        assert!(!stage.stale(View::live(Some("NEW"))));
    }

    #[test]
    fn an_output_writes_text_when_anything_lands_in_the_text() {
        assert!(!Output::new(false).writes_text());
        assert!(!Output::new(true).writes_text());
        let mut out = Output::new(false);
        out.text(b"x");
        assert!(out.writes_text());
        // A pin alone writes nothing to the text.
        let mut stage = pinned_stage();
        let mut out = Output::new(false);
        pin_prompt(&mut stage, &mut out);
        assert!(!out.writes_text());
        let mut out = Output::new(false);
        out.replace(3, Vec::new(), false);
        assert!(out.writes_text());
    }
}
