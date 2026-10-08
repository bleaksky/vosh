//! Selection drags across the scrollback split.
//!
//! Scrolled back, the surface draws history above a divider and the live
//! tail below it. A drag that begins in the history half stays in the
//! history: past the divider it reaches the end of the last history line
//! the half shows, never the divider or a live row, and the history
//! scrolls toward the tail, faster the further past the pointer goes. At
//! the tail the display offset is 0, so the split closes the way a scroll
//! back to the bottom closes it, and the same selection runs on over the
//! full screen. The selection lives in grid lines, so the copied text is
//! one run of lines with nothing doubled and nothing skipped.
//!
//! A drag that begins in the live half or with no split open maps the
//! pointer the way it always has.

use std::time::Duration;

use crate::native::grid::{TermGrid, SPLIT_MIN_ROWS};

/// How far past an edge, in points, the pointer reaches top speed. The
/// same as xterm's drag scroll, so both renderers move alike.
pub(crate) const AUTOSCROLL_EDGE_PT: f64 = 50.0;

/// The most lines one autoscroll tick moves, at or beyond the edge reach.
pub(crate) const AUTOSCROLL_MAX_LINES: u32 = 15;

/// Time between autoscroll ticks.
pub(crate) const AUTOSCROLL_TICK: Duration = Duration::from_millis(50);

/// The lines one autoscroll tick moves for a pointer `past_px` physical
/// pixels beyond the edge at backing scale `dpr`. None at or inside the
/// edge, one just past it, and `AUTOSCROLL_MAX_LINES` from
/// `AUTOSCROLL_EDGE_PT` points on, rising evenly between.
// The rate is a small whole number of lines.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn autoscroll_lines(past_px: f64, dpr: f64) -> u32 {
    if past_px.is_nan() || past_px <= 0.0 {
        return 0;
    }
    let dpr = if dpr > 0.0 { dpr } else { 1.0 };
    let reach = (past_px / dpr / AUTOSCROLL_EDGE_PT).min(1.0);
    1 + (reach * f64::from(AUTOSCROLL_MAX_LINES - 1)).round() as u32
}

/// The surface as the last frame left it, for mapping a pointer: the cell
/// size and surface height in physical pixels, the backing scale, and the
/// divider the renderer drew, as a fraction of the height.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    pub cell_w: f64,
    pub cell_h: f64,
    pub height: f64,
    pub dpr: f64,
    pub divider: Option<f32>,
}

/// The surface at one display offset: the split's divider in physical
/// pixels while the split is open.
#[derive(Clone, Copy, Debug)]
pub(crate) struct View {
    frame: Frame,
    offset: usize,
    divider_px: Option<f64>,
}

/// Where a drag reaches in the grid: a cell's left edge, or through the
/// end of a line, and the lines each autoscroll tick moves toward the
/// tail from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reach {
    pub line: i32,
    pub col: usize,
    pub through_end: bool,
    pub scroll: u32,
}

impl Frame {
    /// The surface at display offset `offset`, or None before the cell
    /// size is known. The split shows while the grid is scrolled back,
    /// holds six rows or more, and the renderer drew a divider, as the
    /// renderer decides it. At offset 0 a divider left from the last
    /// frame no longer counts.
    // Pixel rows on a surface are far inside i32 and f64 range.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn view(&self, offset: usize) -> Option<View> {
        if self.cell_w <= 0.0 || self.cell_h <= 0.0 {
            return None;
        }
        let rows = (self.height / self.cell_h).floor() as i32;
        let divider_px = self
            .divider
            .filter(|_| offset > 0 && usize::try_from(rows).is_ok_and(|rows| rows >= SPLIT_MIN_ROWS))
            // The renderer draws the divider on a whole pixel.
            .map(|frac| (f64::from(frac) * self.height).round());
        Some(View {
            frame: *self,
            offset,
            divider_px,
        })
    }
}

impl View {
    /// The grid cell `(line, col)` under a physical-pixel point, as the
    /// renderer lays the grid out: above the divider is history at the
    /// display offset, below it live rows at their places in the full
    /// view, and with no split the whole surface reads at the offset.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(crate) fn cell_at(&self, x: f64, y: f64) -> (i32, usize) {
        let col = (x / self.frame.cell_w).floor().max(0.0) as usize;
        let row = (y / self.frame.cell_h).floor().max(0.0) as i32;
        match self.divider_px {
            Some(divider) if y >= divider => (row, col),
            _ => (row - self.offset_lines(), col),
        }
    }

    /// True when a press at `y` lands in the history half of an open
    /// split.
    pub(crate) fn in_history(&self, y: f64) -> bool {
        self.divider_px.is_some_and(|divider| y < divider)
    }

    /// The newest line the history half shows while the split is open.
    /// Its row can hang past the divider, cut off, as the renderer draws
    /// it.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn history_last_line(&self) -> Option<i32> {
        let divider = self.divider_px?;
        let rows = (divider / self.frame.cell_h).ceil() as i32;
        Some(rows - 1 - self.offset_lines())
    }

    /// Where a drag that began in the history half reaches with the
    /// pointer at `(x, y)`. Above the divider it is the cell under the
    /// pointer. At or past it, the end of the last history line, and the
    /// history scrolls toward the tail. With the split closed it is the
    /// cell under the pointer in the full view.
    pub(crate) fn history_reach(&self, x: f64, y: f64) -> Reach {
        if let (Some(divider), Some(last)) = (self.divider_px, self.history_last_line()) {
            if y >= divider {
                return Reach {
                    line: last,
                    col: 0,
                    through_end: true,
                    scroll: autoscroll_lines(y - divider, self.frame.dpr),
                };
            }
        }
        let (line, col) = self.cell_at(x, y);
        Reach {
            line,
            col,
            through_end: false,
            scroll: 0,
        }
    }

    fn offset_lines(&self) -> i32 {
        i32::try_from(self.offset).unwrap_or(i32::MAX)
    }
}

/// Extend a drag that began in the history half to where the pointer at
/// `(x, y)` reaches, and return the lines each autoscroll tick should
/// move, 0 when none should run.
pub(crate) fn drag(grid: &mut TermGrid, frame: &Frame, x: f64, y: f64) -> u32 {
    let Some(view) = frame.view(grid.display_offset()) else {
        return 0;
    };
    let reach = view.history_reach(x, y);
    grid.extend_selection(reach.line, reach.col, reach.through_end);
    reach.scroll
}

/// One autoscroll tick of a drag from the history half: scroll toward the
/// tail by the pointer's rate, never past it, then extend the selection
/// to where the pointer now reaches. Reaching the tail closes the split,
/// and the selection carries on to the pointer's cell in the full view.
/// True when another tick should follow.
pub(crate) fn tick(grid: &mut TermGrid, frame: &Frame, x: f64, y: f64) -> bool {
    let Some(view) = frame.view(grid.display_offset()) else {
        return false;
    };
    let lines = view.history_reach(x, y).scroll;
    if lines == 0 {
        return false;
    }
    grid.scroll(-i32::try_from(lines).unwrap_or(i32::MAX));
    drag(grid, frame, x, y) > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10 px cell on a 120 px surface (12 rows) at scale 1, with the
    /// divider where the renderer draws it at the default ratio: 79 px,
    /// so the history half shows 8 rows, the last cut off.
    fn frame() -> Frame {
        Frame {
            cell_w: 8.0,
            cell_h: 10.0,
            height: 120.0,
            dpr: 1.0,
            divider: Some(79.0 / 120.0),
        }
    }

    /// A grid of `lines` numbered lines, `rows` tall, scrolled back by
    /// `offset`.
    fn grid(lines: usize, rows: usize, offset: i32) -> TermGrid {
        let mut grid = TermGrid::new(20, rows);
        let text: Vec<String> = (0..lines).map(|n| format!("L{n:02}")).collect();
        grid.feed(text.join("\r\n").as_bytes());
        grid.scroll(offset);
        grid
    }

    /// The numbered lines from `first` to `last`, as a copy reads them.
    fn run(first: usize, last: usize) -> String {
        (first..=last)
            .map(|n| format!("L{n:02}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_rate_grows_with_the_distance_past_the_edge() {
        assert_eq!(autoscroll_lines(-5.0, 1.0), 0);
        assert_eq!(autoscroll_lines(0.0, 1.0), 0);
        assert_eq!(autoscroll_lines(f64::NAN, 1.0), 0);
        assert_eq!(autoscroll_lines(0.5, 1.0), 1);
        assert_eq!(autoscroll_lines(25.0, 1.0), 8);
        assert_eq!(autoscroll_lines(50.0, 1.0), AUTOSCROLL_MAX_LINES);
        assert_eq!(autoscroll_lines(500.0, 1.0), AUTOSCROLL_MAX_LINES);
        // Points, not pixels: the same reach on a Retina surface.
        assert_eq!(autoscroll_lines(50.0, 2.0), 8);
        assert_eq!(autoscroll_lines(100.0, 2.0), AUTOSCROLL_MAX_LINES);
        let mut last = 0;
        for px in 1..=60 {
            let lines = autoscroll_lines(f64::from(px), 1.0);
            assert!(lines >= last, "slower at {px} px");
            last = lines;
        }
    }

    #[test]
    fn a_press_above_the_divider_lands_in_the_history() {
        let view = frame().view(8).expect("a view");
        assert!(view.in_history(0.0));
        assert!(view.in_history(78.9));
        assert!(!view.in_history(79.0));
        assert!(!view.in_history(110.0));
        // No split at the tail, even with last frame's divider.
        assert!(!frame().view(0).expect("a view").in_history(10.0));
        // None while the renderer drew no divider, as during a find.
        let finding = Frame {
            divider: None,
            ..frame()
        };
        assert!(!finding.view(8).expect("a view").in_history(10.0));
    }

    #[test]
    fn the_history_half_ends_on_the_row_the_divider_cuts() {
        // 79 px over 10 px rows: rows 0 to 7, the eighth cut off.
        assert_eq!(
            frame().view(8).and_then(|v| v.history_last_line()),
            Some(-1)
        );
        assert_eq!(frame().view(3).and_then(|v| v.history_last_line()), Some(4));
        // A divider on a row boundary shows whole rows only.
        let even = Frame {
            divider: Some(80.0 / 120.0),
            ..frame()
        };
        assert_eq!(even.view(8).and_then(|v| v.history_last_line()), Some(-1));
        assert_eq!(frame().view(0).and_then(|v| v.history_last_line()), None);
    }

    #[test]
    fn the_cell_map_matches_the_renderer_on_both_sides() {
        let view = frame().view(8).expect("a view");
        assert_eq!(view.cell_at(17.0, 15.0), (-7, 2));
        // Below the divider the live rows keep their full view places.
        assert_eq!(view.cell_at(0.0, 100.0), (10, 0));
        let full = frame().view(0).expect("a view");
        assert_eq!(full.cell_at(0.0, 15.0), (1, 0));
        assert!(Frame {
            cell_h: 0.0,
            ..frame()
        }
        .view(8)
        .is_none());
    }

    #[test]
    fn past_the_divider_the_reach_stops_at_the_history() {
        let view = frame().view(8).expect("a view");
        let last = view.history_last_line().expect("a split");
        // Above it, the cell under the pointer.
        assert_eq!(
            view.history_reach(16.0, 45.0),
            Reach {
                line: -4,
                col: 2,
                through_end: false,
                scroll: 0
            }
        );
        // On the divider, the end of the last history line, no scroll.
        assert_eq!(view.history_reach(0.0, 79.0).scroll, 0);
        // Past it, never a live row or the divider, and faster further on.
        let mut rate = 0;
        for y in 80..400 {
            let reach = view.history_reach(40.0, f64::from(y));
            assert_eq!((reach.line, reach.through_end), (last, true), "at {y}");
            assert!(reach.scroll >= rate.max(1), "at {y}");
            rate = reach.scroll;
        }
    }

    #[test]
    fn a_drag_past_the_divider_selects_through_the_last_history_line() {
        // 48 lines in history and L48 to L59 on the screen, so grid
        // line n holds L(n + 48), and offset 8 shows L40 to L47 above
        // the divider.
        let mut g = grid(60, 12, 8);
        assert_eq!(g.scrollback_len(), 48);
        let view = frame().view(8).expect("a view");
        let (line, col) = view.cell_at(0.0, 15.0);
        assert_eq!((line, col), (-7, 0));
        g.start_selection(line, col);
        let scroll = drag(&mut g, &frame(), 40.0, 100.0);
        assert!(scroll > 0);
        assert_eq!(g.selection_text().as_deref(), Some(run(41, 47).as_str()));
        assert_eq!(g.display_offset(), 8);
    }

    #[test]
    fn the_autoscroll_hands_the_selection_to_the_full_view_at_the_tail() {
        let mut g = grid(60, 12, 8);
        let view = frame().view(8).expect("a view");
        let (line, col) = view.cell_at(0.0, 15.0);
        g.start_selection(line, col);
        // 21 px past the divider: 7 lines a tick.
        let (x, y) = (40.0, 100.0);
        assert_eq!(drag(&mut g, &frame(), x, y), 7);
        let mut seen = vec![g.selection_text().expect("a selection")];
        let mut ticks = 0;
        while tick(&mut g, &frame(), x, y) {
            ticks += 1;
            assert!(g.display_offset() > 0, "a tick past the tail");
            seen.push(g.selection_text().expect("a selection"));
        }
        seen.push(g.selection_text().expect("a selection"));
        assert_eq!(ticks, 1, "8 lines at 7 a tick");
        assert_eq!(g.display_offset(), 0, "the split closed");
        // While the split was open, each copy ended on the last history
        // line: offset 8 shows L41 to L47, offset 1 shows to L54.
        assert_eq!(seen[0], run(41, 47));
        assert_eq!(seen[1], run(41, 54));
        // Closed, the pointer's row 10 is L58, up to its column 5 (before
        // the pointer's cell), so L41 to L57 whole and nothing of L58's
        // three characters past the start.
        let full = frame().view(0).expect("a view");
        assert_eq!(full.cell_at(x, y), (10, 5));
        assert_eq!(seen[2], format!("{}\nL58", run(41, 57)));
    }

    #[test]
    fn every_copy_on_the_way_down_is_one_unbroken_run() {
        for (y, offset) in [(80.0, 30), (95.0, 30), (130.0, 30), (300.0, 47)] {
            let mut g = grid(60, 12, offset);
            let view = frame().view(g.display_offset()).expect("a view");
            let (line, col) = view.cell_at(0.0, 5.0);
            g.start_selection(line, col);
            let anchor = usize::try_from(line + 48).expect("a line");
            drag(&mut g, &frame(), 0.0, y);
            let mut guard = 0;
            loop {
                let text = g.selection_text().expect("a selection");
                let numbers: Vec<usize> = text
                    .lines()
                    .map(|l| l.trim_start_matches('L').parse().expect("a number"))
                    .collect();
                let want: Vec<usize> = (anchor..anchor + numbers.len()).collect();
                assert_eq!(numbers, want, "y {y} offset {offset}");
                if g.display_offset() > 0 {
                    // Open: nothing past the history half.
                    let last = frame()
                        .view(g.display_offset())
                        .and_then(|v| v.history_last_line())
                        .expect("a split");
                    assert_eq!(
                        numbers.last().copied(),
                        Some(usize::try_from(last + 48).expect("a line"))
                    );
                }
                if !tick(&mut g, &frame(), 0.0, y) {
                    break;
                }
                guard += 1;
                assert!(guard < 100, "the autoscroll never ended");
            }
            assert_eq!(g.display_offset(), 0, "y {y} offset {offset}");
        }
    }

    #[test]
    fn a_pointer_back_above_the_divider_stops_the_autoscroll() {
        let mut g = grid(60, 12, 8);
        g.start_selection(-7, 0);
        assert!(drag(&mut g, &frame(), 0.0, 100.0) > 0);
        assert_eq!(drag(&mut g, &frame(), 16.0, 45.0), 0);
        assert!(!tick(&mut g, &frame(), 16.0, 45.0));
        assert_eq!(g.display_offset(), 8);
        assert_eq!(g.selection_text().as_deref(), Some("L41\nL42\nL43\nL4"));
    }
}
