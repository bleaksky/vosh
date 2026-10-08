//! Rows a later read can color again: the people of a room look whose
//! Room.Chars has not come yet, so the session cannot tell which of them
//! is your target when they show. The rows go out as one region, and the
//! packet's read writes the region again with your target's row in its
//! color, as a repaint rewrites the open row. A renderer that finds
//! anything written after the region drops the rewrite, so the rows stay
//! as they show.

use super::marks::mark;
use super::output::{write, Output};
use super::Stage;

/// One row of the region, as the Line pass left it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    /// The line as the game sent it, and without its codes.
    raw: Vec<u8>,
    plain: String,
    /// What shows before the line, such as what a script echoed in place
    /// of it.
    before: Vec<u8>,
    /// The line as its triggers left it, or None when one hid it.
    shown: Option<Vec<u8>>,
}

impl Row {
    fn bytes(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.before);
        if let Some(shown) = &self.shown {
            out.extend_from_slice(shown);
            out.extend_from_slice(b"\r\n");
        }
    }
}

/// The rows a later read can color again, while they are the last thing
/// written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Recolor {
    /// The region the rows show in.
    gen: u64,
    /// The place of the first row, as the caller numbers them.
    first: usize,
    rows: Vec<Row>,
    /// The output that wrote the rows.
    pub(super) output: u64,
    /// What that output had written at the cursor right after them (see
    /// [`Output::written`]), so anything written later shows.
    end: usize,
}

/// A row [`Stage::recolor`] wrote again: how the line showed and how it
/// shows now, each None when a trigger hid it, and how many rows after
/// it show a line, so the scrollback ring can find it from its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recolored {
    pub was: Option<Vec<u8>>,
    pub now: Option<Vec<u8>>,
    pub after: usize,
}

impl Stage {
    /// Write a complete line that is not your prompt, as [`Stage::line`]
    /// does, as a row a later read can color again with
    /// [`Stage::recolor`]. `place` numbers the row: the row right after
    /// the last one this output wrote, with nothing between them, joins
    /// its region, and any other row starts a region of its own. `before`
    /// shows before the line, and `shown` is the line as its triggers left
    /// it, None when one hid it.
    pub fn recolorable_line(
        &mut self,
        out: &mut Output,
        raw: &[u8],
        plain: &str,
        place: usize,
        before: Vec<u8>,
        shown: Option<Vec<u8>>,
    ) {
        self.sync(out);
        self.end_run(out);
        let row = Row {
            raw: raw.to_vec(),
            plain: plain.to_string(),
            before,
            shown,
        };
        let mut bytes = Vec::new();
        row.bytes(&mut bytes);
        self.carry_through(&bytes);
        let joins = self.recolor.as_ref().is_some_and(|r| {
            r.output == out.id.0 && r.end == out.written() && r.first + r.rows.len() == place
        });
        if joins {
            write(out, &mut self.open, None, bytes);
        } else {
            let gen = self.next_gen();
            let mut region = mark(gen);
            region.extend(bytes);
            write(out, &mut self.open, None, region);
            self.recolor = Some(Recolor {
                gen,
                first: place,
                rows: Vec::new(),
                output: out.id.0,
                end: 0,
            });
        }
        if let Some(r) = self.recolor.as_mut() {
            r.rows.push(row);
            r.end = out.written();
        }
        self.note_line(raw, plain);
    }

    /// The line at `place` among the rows a later read can color again,
    /// as the game sent it and without its codes, while they are still
    /// the last thing written as of `out`, a later output that has
    /// written nothing yet.
    pub fn recolorable(&self, out: &Output, place: usize) -> Option<(&[u8], &str)> {
        let r = self.recolor.as_ref()?;
        if r.output == out.id.0 || out.other || !out.untouched() {
            return None;
        }
        let row = r.rows.get(place.checked_sub(r.first)?)?;
        Some((&row.raw, &row.plain))
    }

    /// Write the rows a later read can color again with the line at
    /// `place` shown as `shown` now, None when a trigger hides it, as the
    /// replace of their region. Returns the row when it shows another way
    /// than before. The rows are never written again after, so the region
    /// closes. See [`Stage::recolorable`] for when it can.
    pub fn recolor(
        &mut self,
        out: &mut Output,
        place: usize,
        shown: Option<Vec<u8>>,
    ) -> Option<Recolored> {
        self.recolorable(out, place)?;
        let mut r = self.recolor.take()?;
        let at = place - r.first;
        if r.rows[at].shown == shown {
            return None;
        }
        let was = std::mem::replace(&mut r.rows[at].shown, shown.clone());
        let mut bytes = Vec::new();
        for row in &r.rows {
            row.bytes(&mut bytes);
        }
        out.replace(r.gen, bytes, false);
        let after = r.rows[at + 1..]
            .iter()
            .filter(|row| row.shown.is_some())
            .count();
        Some(Recolored {
            was,
            now: shown,
            after,
        })
    }

    /// Catch the rows a later read can color again up with `out` before
    /// it goes out. Anything written after them ends them: later bytes in
    /// the output that wrote them, or any text in a later output, and so
    /// does output from elsewhere before a later output. A replace counts
    /// as text, so the rows are colored again at most once.
    pub(super) fn settle_recolor(&mut self, out: &Output) {
        let Some(r) = self.recolor.as_ref() else {
            return;
        };
        let ended = if r.output == out.id.0 {
            out.written() != r.end
        } else {
            out.other || out.writes_text()
        };
        if ended {
            self.recolor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VILLAGER: &str = "A Blackwatch villager scurries about, taking care of business.";
    const RESTING: &str = "Tolliver is resting here.";

    fn row(stage: &mut Stage, out: &mut Output, place: usize, line: &str) {
        stage.recolorable_line(
            out,
            line.as_bytes(),
            line,
            place,
            Vec::new(),
            Some(line.as_bytes().to_vec()),
        );
    }

    #[test]
    fn the_rows_show_as_one_region_a_later_read_writes_again_with_the_new_color() {
        let mut stage = Stage::default();
        let mut first = Output::new(false);
        first.text(b"[Exits: south]\r\n");
        row(&mut stage, &mut first, 1, VILLAGER);
        row(&mut stage, &mut first, 2, RESTING);
        stage.finish(&mut first);
        let gen = stage.gen;
        let mut want = b"[Exits: south]\r\n".to_vec();
        want.extend(mark(gen));
        want.extend(format!("{VILLAGER}\r\n{RESTING}\r\n").as_bytes());
        assert_eq!(first.bytes, want);

        let mut next = Output::new(false);
        assert_eq!(
            stage.recolorable(&next, 2),
            Some((RESTING.as_bytes(), RESTING))
        );
        let red = format!("\x1b[91m{RESTING}\x1b[0m");
        assert_eq!(
            stage.recolor(&mut next, 2, Some(red.as_bytes().to_vec())),
            Some(Recolored {
                was: Some(RESTING.as_bytes().to_vec()),
                now: Some(red.as_bytes().to_vec()),
                after: 0,
            })
        );
        let replace = next.replace.as_ref().expect("the rows go again");
        assert_eq!(replace.gen, gen);
        assert!(!replace.fresh);
        assert_eq!(
            String::from_utf8_lossy(&replace.bytes),
            format!("{VILLAGER}\r\n{red}\r\n")
        );
        // The rows go once.
        assert_eq!(stage.recolorable(&next, 1), None);
    }

    #[test]
    fn anything_written_after_the_rows_leaves_them_as_they_show() {
        let mut stage = Stage::default();
        let mut first = Output::new(false);
        row(&mut stage, &mut first, 1, VILLAGER);
        first.text(b"A werebeast looks into the sky.\r\n");
        stage.finish(&mut first);
        assert_eq!(stage.recolorable(&Output::new(false), 1), None);

        // Output from elsewhere, such as your echo, came first.
        let mut first = Output::new(false);
        row(&mut stage, &mut first, 1, VILLAGER);
        stage.finish(&mut first);
        assert_eq!(stage.recolorable(&Output::new(true), 1), None);

        // A later output wrote text, then the packet came.
        let mut next = Output::new(false);
        next.text(b"Tolliver says 'hello'\r\n");
        assert_eq!(stage.recolor(&mut next, 1, None), None);
        assert!(next.replace.is_none());
    }

    #[test]
    fn a_row_with_a_gap_before_it_starts_a_region_of_its_own() {
        let mut stage = Stage::default();
        let mut first = Output::new(false);
        row(&mut stage, &mut first, 1, VILLAGER);
        first.text(b"\r\n");
        row(&mut stage, &mut first, 1, RESTING);
        stage.finish(&mut first);
        let next = Output::new(false);
        assert_eq!(
            stage.recolorable(&next, 1),
            Some((RESTING.as_bytes(), RESTING))
        );
        assert_eq!(stage.recolorable(&next, 2), None);
    }
}
