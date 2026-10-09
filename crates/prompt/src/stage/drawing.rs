//! How the stage draws your prompt in the text, as the open row a later
//! read or a repaint replaces, and writes the lines around it that are
//! not your prompt.

use super::blocks::{Block, End, OpenLift, View};
use super::marks::{end_lift, lift_start, mark, region_bytes, with_lift_end};
use super::output::{put, shows_anything, write, Above, Output};
use super::Stage;
use crate::config::PromptShow;

impl Stage {
    /// Draw `rendered` in place of `block`, as the open row with no line
    /// end, so the cursor sits after it like a game's prompt. A prompt
    /// whole before the line end that came after it keeps that line end
    /// in the row. `painted` is the region an earlier read painted the
    /// block's partial as, which the drawn prompt replaces. `before` goes
    /// first, such as lines a Prompts trigger's script echoed. Test only.
    /// The app's tests reach it through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
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

    /// `Stage::draw` with what the open card shows: a preview, the labels
    /// of values with nothing to show, or the game's own line. The
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
    pub(super) fn lifts_view(&self, view: &View) -> bool {
        self.lifts() || (self.card && self.show == PromptShow::Text && view.shown.is_some())
    }

    /// `bytes`, the lines of a prompt shown as sent, between the marks of
    /// a new lift while your prompt shows lifted and something in them
    /// shows.
    pub(super) fn lift_shown(&mut self, bytes: Vec<u8>) -> Vec<u8> {
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
    pub fn show_as_sent(
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
    pub(crate) fn stale(&self, view: View) -> bool {
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
        // reads, and so which of them show as sent.
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
pub(super) fn row_bodies(block: &Block, view: View) -> (Vec<u8>, Option<Vec<u8>>) {
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
