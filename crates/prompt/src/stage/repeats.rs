//! Collapse repeated lines. The stage writes a run of repeated lines
//! again in place with its count, and follows the colors the text
//! carries from line to line so the run keeps them.

use super::marks::mark;
use super::output::{escape_end, plain_text, shows_anything, Above, Output};
use super::Stage;
use crate::aabahran::damage::attack_line;
use crate::render::{Color, SgrState};

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

/// Which lines Collapse repeated lines takes, from the two rows under it
/// in Settings. A line it leaves whole shows as any other line and ends
/// the run before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollapseRules {
    /// In a fight: the lines of a fight collapse, from the round
    /// Char.Combat names a target in to the round that ends the fight. On
    /// at first. Off, every line of a fight shows, attack lines too,
    /// wherever they come.
    pub fights: bool,
    /// Attack lines: the hits and misses `dam_message` prints collapse,
    /// in a fight or not (see [`attack_line`]). Off at first, so a count
    /// never hides how many hits landed, and off while In a fight is.
    pub attacks: bool,
}

impl Default for CollapseRules {
    fn default() -> Self {
        Self {
            fights: true,
            attacks: false,
        }
    }
}

impl CollapseRules {
    /// True when attack lines collapse: their row says so, and so does
    /// In a fight, which leaves them whole while it is off.
    pub fn attacks_collapse(self) -> bool {
        self.fights && self.attacks
    }

    /// True when Collapse repeated lines takes a line that reads `plain`
    /// without its colors. `fighting` says the line belongs to a fight:
    /// Char.Combat named a target as it came, or the round it belongs to
    /// ended the fight. The game sends a pulse's packets before its text,
    /// so a line of a round reads the round's own Char.Combat. The `{}`
    /// that ends a fight comes in the middle of the round that ends it,
    /// before all of that round's text, so the session counts that round
    /// as the fight's until the prompt that ends it. A game that sends its
    /// prompt time packages after the text sends a fight's first round
    /// before the Char.Combat that names a target, so the session counts
    /// the round as the fight's from an attack line of yours.
    pub fn takes(self, fighting: bool, plain: &str) -> bool {
        if fighting && !self.fights {
            return false;
        }
        self.attacks_collapse() || !attack_line(plain)
    }
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
pub(super) struct RunAfter {
    pub(super) gen: u64,
    pub(super) empty: bool,
}

/// The run of repeated lines the text ends on, while Collapse repeated
/// lines is on (see [`Stage::repeat_line`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Run {
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
    pub(super) output: u64,
    /// What that output had written at the cursor right after it (see
    /// [`Output::written`]), so anything written later shows.
    pub(super) end: usize,
    place: RunPlace,
    /// How many bytes at the end of the run's region each renderer keeps
    /// back as held line ends while your prompt shows pinned. A rewrite
    /// leaves them out, so the line ends each renderer holds still follow
    /// it.
    held: usize,
    pub(super) after: Option<RunAfter>,
}

impl Stage {
    /// Take Collapse repeated lines from the profile. While it is on, the
    /// stage follows the colors the text carries from one line to the
    /// next, so a run it writes again keeps them (see [`counted`]).
    pub fn set_collapse(&mut self, on: bool) {
        self.collapse = on;
    }

    /// Follow the colors `bytes`, written to the text, leave the text in,
    /// while Collapse repeated lines is on.
    pub(super) fn carry_through(&mut self, bytes: &[u8]) {
        if self.collapse {
            apply_sgr(&mut self.carry, bytes);
        }
    }

    /// Catch the run of repeated lines up with `out` before it goes out.
    /// Anything written after the run ends it: later bytes in the output
    /// that wrote it, or any text in a later output. Output from elsewhere
    /// that came before a later output ends it too. While the run is the
    /// last thing its own output wrote, note the line ends at its end that
    /// the output holds back, which each renderer keeps after it.
    pub(super) fn settle_run(&mut self, out: &Output) {
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
    pub(super) fn end_run(&mut self, out: &mut Output) {
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
    pub(super) fn run_partial(&self, out: &Output, gen: u64) -> bool {
        self.run_last(out)
            && self
                .run
                .as_ref()
                .is_some_and(|run| run.after == Some(RunAfter { gen, empty: false }))
    }

    /// True when a region written at the cursor now lands right after the
    /// run of repeated lines: the run is still the last thing written, and
    /// nothing follows it but the empty region a pinned prompt left.
    pub(super) fn run_open_after(&self, out: &Output) -> bool {
        self.run_last(out)
            && self
                .run
                .as_ref()
                .is_some_and(|run| run.after.map_or(true, |after| after.empty))
    }

    /// Note that the region `gen`, a partial or held lines, now follows
    /// the run of repeated lines.
    pub(super) fn run_followed_by(&mut self, out: &Output, gen: u64) {
        if let Some(run) = self.run.as_mut() {
            run.after = Some(RunAfter { gen, empty: false });
            run.output = out.id.0;
            run.end = out.written();
        }
    }
}
