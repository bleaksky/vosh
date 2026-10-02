//! How the stage pins your prompt to the band above the command line,
//! and moves it between the band and the text when you change where it
//! shows.

use super::blocks::{Block, End, OpenLift, View};
use super::drawing::row_bodies;
use super::marks::{lift_start, mark, region_bytes};
use super::output::{Above, Output};
use super::repeats::RunAfter;
use super::Stage;
use crate::config::PromptShow;
use crate::render::Span;

/// The empty line a pinned prompt's row would have ended with, which
/// writes nothing when it comes. `seen` is how many visible writes the
/// output `output` had when the prompt pinned, so a later one disarms it,
/// and so does any visible write in a later output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Swallow {
    pub(super) output: u64,
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
    pub(super) fn ended_by(self, out: &Output) -> bool {
        if out.id.0 == self.output {
            out.visible > self.seen
        } else {
            out.other || out.visible > 0
        }
    }
}

/// A prompt pinned with drawing off, after Prompts triggers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PinnedShown {
    /// What the band shows.
    band: Vec<u8>,
    /// What the text would have shown, line ends included.
    text: Vec<u8>,
}

impl Stage {
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

    /// What the band shows for `block` with `shown`: drawing off, what
    /// Prompts triggers left of the prompt it pinned.
    pub(super) fn band_body(&self, block: &Block, shown: Option<&str>) -> Vec<u8> {
        let shown = shown.filter(|_| !block.afk);
        match (shown, &self.pinned_shown) {
            (None, Some(pinned)) => pinned.band.clone(),
            _ => pin_body(block, shown),
        }
    }

    /// Show the band as the `[prompt]` table now says. It never writes to
    /// the text, so it never races your echo. The same bytes go out again
    /// when the pieces in them are numbered anew, such as after an edit.
    pub(super) fn repaint_pinned(&mut self, out: &mut Output, view: View) {
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
    pub(super) fn move_to_pinned(&mut self, out: &mut Output, view: View) {
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
    pub(super) fn move_from_pinned(&mut self, out: &mut Output, view: View) {
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
