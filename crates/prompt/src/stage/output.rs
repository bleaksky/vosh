//! The [`Output`] Vosh writes to the terminal for one read or one
//! repaint, the [`Replace`] of an earlier region it opens with, and the
//! helpers that find escape sequences, marks and line ends in its bytes.

use super::blocks::OpenRow;
use super::marks::{mark, MARK_OSC};
use crate::render::Span;

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
/// a tank line your design does not read. A text prompt carries no
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
    pub(super) row_open: bool,
    /// The open row is no longer the last thing on screen.
    pub(super) closed: bool,
    /// The region this output leaves open shows a preview, with the live
    /// render to put in its place the moment anything is written after
    /// it, so a preview never reaches history.
    preview: Option<PreviewTail>,
    /// Output from elsewhere reached the terminal before this output.
    pub(super) other: bool,
    /// How many writes of this output showed something, so the stage can
    /// tell text landed after a pinned prompt, whoever wrote it.
    pub(super) visible: u64,
    /// Which output this is, so the stage can tell a new one from the one
    /// it pinned in.
    pub(super) id: OutputId,
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
pub(super) struct OutputId(pub(super) u64);

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
    ///
    /// [`Stage::local_write`]: super::Stage::local_write
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
    pub(super) fn untouched(&self) -> bool {
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
    pub(super) fn preview_tail(&mut self, region: usize, live: Vec<u8>) {
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
    pub(super) fn push(&mut self, bytes: &[u8]) {
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
    pub(super) fn hold_tail(&mut self) {
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
    pub(crate) fn region(&mut self, gen: u64, bytes: &[u8]) {
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
    pub(super) fn replace_above(&mut self, gen: u64, bytes: Vec<u8>, above: Option<Above>) {
        self.replace(gen, bytes, false);
        if let Some(replace) = self.replace.as_mut().filter(|r| r.gen == gen) {
            replace.above = above;
        }
    }

    /// How much this output wrote to the text at the cursor, its bytes and
    /// the line ends it holds back together. Holding line ends back moves
    /// them and leaves this the same, so only a new write or a rewrite
    /// changes it.
    pub(super) fn written(&self) -> usize {
        self.bytes.len() + self.hold.len()
    }

    /// Write `bytes` in place of everything this output wrote from `at`
    /// on, the line ends it holds back included, as a run of repeated
    /// lines this output wrote last does when the next line joins it.
    pub(super) fn rewrite_from(&mut self, at: usize, bytes: &[u8]) {
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
    pub(super) fn rewrite_replace(&mut self, bytes: Vec<u8>, tail: Vec<u8>) {
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
    pub(super) fn new_row(&mut self) {
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
pub(super) fn shows_anything(bytes: &[u8]) -> bool {
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
pub(super) fn escape_end(bytes: &[u8], at: usize) -> usize {
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
pub(crate) fn trailing_line_ends(bytes: &[u8]) -> usize {
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

/// Write `bytes` over the region `painted`, or as they are.
pub(super) fn put(out: &mut Output, painted: Option<u64>, bytes: Vec<u8>, fresh: bool) {
    match painted {
        Some(gen) => out.replace(gen, bytes, fresh),
        None => out.text(&bytes),
    }
}

/// Write what is not a region: over the region `painted`, or as it is.
/// It closes the open row, unless it writes nothing at all, as a hidden
/// line that was never painted does.
pub(super) fn write(
    out: &mut Output,
    open: &mut Option<OpenRow>,
    painted: Option<u64>,
    bytes: Vec<u8>,
) {
    if bytes.is_empty() && painted.is_none() {
        return;
    }
    let fresh = !bytes.is_empty();
    put(out, painted, bytes, fresh);
    *open = None;
}
