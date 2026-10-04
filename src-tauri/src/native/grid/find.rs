//! Find in the terminal. A search over every line the shared grid holds,
//! with the matches the renderer marks and the one you stepped to.

use std::sync::Mutex;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use regex::RegexBuilder;

use super::{grid_slot, TermGrid};

/// A grid's find. The matches run in reading order, top of scrollback to
/// bottom, and `active` indexes the one you stepped to. The query stays,
/// so a search for the same query again steps to the next match instead
/// of starting over.
#[derive(Default)]
pub(crate) struct Find {
    matches: Vec<FindMatch>,
    active: usize,
    query: String,
}

impl Find {
    /// All matches plus the active match, for the renderer's highlight
    /// pass.
    pub(crate) fn snapshot(&self) -> (Vec<FindMatch>, Option<FindMatch>) {
        (self.matches.clone(), self.matches.get(self.active).copied())
    }

    /// Search `grid` and step to the next (or previous) match, scrolling
    /// it into view. Returns (current, total) for the toolbar, 1-based;
    /// (0, 0) when there is no match.
    pub(crate) fn run(
        &mut self,
        grid: &mut TermGrid,
        query: &str,
        is_regex: bool,
        case_sensitive: bool,
        whole_word: bool,
        forward: bool,
    ) -> (usize, usize) {
        let matches = collect_matches(grid, query, is_regex, case_sensitive, whole_word);
        if matches.is_empty() {
            self.clear();
            return (0, 0);
        }
        let total = matches.len();
        let active = if self.query == query {
            let prev = self.active.min(total - 1);
            if forward {
                (prev + 1) % total
            } else {
                (prev + total - 1) % total
            }
        } else if forward {
            0
        } else {
            total - 1
        };
        scroll_to_grid_line(grid, matches[active].0);
        self.query = query.to_string();
        self.matches = matches;
        self.active = active;
        (active + 1, total)
    }

    /// Clear the matches, the active match and the query.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

// The find of the shared grid, taken after the grid's lock.
static FIND: Mutex<Find> = Mutex::new(Find {
    matches: Vec::new(),
    active: 0,
    query: String::new(),
});

fn build_find_regex(
    query: &str,
    is_regex: bool,
    case_sensitive: bool,
    whole_word: bool,
) -> Option<regex::Regex> {
    if query.is_empty() {
        return None;
    }
    let mut pattern = if is_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    if whole_word {
        pattern = format!(r"\b{pattern}\b");
    }
    RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .ok()
}

/// A match location: grid line, start column, end column (character cells).
pub(crate) type FindMatch = (i32, usize, usize);

/// Collect every match of `query` in the grid as line/start/end in reading
/// order. Column indices are character cells.
pub(super) fn collect_matches(
    grid: &TermGrid,
    query: &str,
    is_regex: bool,
    case_sensitive: bool,
    whole_word: bool,
) -> Vec<(i32, usize, usize)> {
    let Some(re) = build_find_regex(query, is_regex, case_sensitive, whole_word) else {
        return Vec::new();
    };
    let g = grid.term.grid();
    let cols = g.columns();
    let mut matches = Vec::new();
    for line in g.topmost_line().0..=g.bottommost_line().0 {
        let text: String = (0..cols).map(|c| g[Line(line)][Column(c)].c).collect();
        for m in re.find_iter(&text) {
            let start_col = text[..m.start()].chars().count();
            let end_col = text[..m.end()].chars().count();
            if end_col > start_col {
                matches.push((line, start_col, end_col));
            }
        }
    }
    matches
}

/// All matches plus the active match, for the renderer's highlight pass.
pub(crate) fn find_snapshot() -> (Vec<FindMatch>, Option<FindMatch>) {
    FIND.lock()
        .map_or_else(|_| (Vec::new(), None), |find| find.snapshot())
}

/// Search the shared grid and step to the next (or previous) match, see
/// [`Find::run`]. (0, 0) before the grid exists.
pub(crate) fn find_run(
    query: &str,
    is_regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    forward: bool,
) -> (usize, usize) {
    let Ok(mut slot) = grid_slot().lock() else {
        return (0, 0);
    };
    let Ok(mut find) = FIND.lock() else {
        return (0, 0);
    };
    match slot.as_mut() {
        Some(grid) => find.run(grid, query, is_regex, case_sensitive, whole_word, forward),
        None => {
            find.clear();
            (0, 0)
        }
    }
}

/// Scroll the display so `line` sits near the middle of the screen.
fn scroll_to_grid_line(grid: &mut TermGrid, line: i32) {
    let g = grid.term.grid();
    let screen = g.screen_lines();
    let history = g.total_lines().saturating_sub(screen);
    let target = (screen as i32 / 2 - line).max(0) as usize;
    let target = target.min(history);
    let delta = target as i32 - g.display_offset() as i32;
    if delta != 0 {
        grid.term.scroll_display(Scroll::Delta(delta));
    }
}

/// Clear the find state (matches, active, query).
pub(crate) fn find_clear() {
    if let Ok(mut find) = FIND.lock() {
        find.clear();
    }
}
