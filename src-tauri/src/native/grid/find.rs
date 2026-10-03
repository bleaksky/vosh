//! Find in the terminal. A search over every line the shared grid holds,
//! with the matches the renderer marks and the one you stepped to.

use std::sync::Mutex;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use regex::RegexBuilder;

use super::{grid_slot, TermGrid};

// Find/search state. Matches are (grid_line, col_start, col_end) in reading
// order (top of scrollback to bottom); active is an index into them. The
// query is remembered so repeated calls with the same query advance the
// active match instead of resetting it.
static FIND_MATCHES: Mutex<Vec<(i32, usize, usize)>> = Mutex::new(Vec::new());
static FIND_ACTIVE: Mutex<usize> = Mutex::new(0);
static FIND_QUERY: Mutex<String> = Mutex::new(String::new());

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
    let matches = match FIND_MATCHES.lock() {
        Ok(m) => m.clone(),
        Err(_) => Vec::new(),
    };
    let active = FIND_ACTIVE
        .lock()
        .ok()
        .and_then(|i| matches.get(*i).copied());
    (matches, active)
}

/// Run a search and step to the next (or previous) match, scrolling it into
/// view. Returns (current, total) for the toolbar, 1-based; (0, 0) when
/// there is no match.
pub(crate) fn find_run(
    query: &str,
    is_regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    forward: bool,
) -> (usize, usize) {
    let matches = match grid_slot().lock() {
        Ok(slot) => match slot.as_ref() {
            Some(grid) => collect_matches(grid, query, is_regex, case_sensitive, whole_word),
            None => Vec::new(),
        },
        Err(_) => Vec::new(),
    };
    if matches.is_empty() {
        find_clear();
        return (0, 0);
    }
    let total = matches.len();
    let query_changed = FIND_QUERY.lock().map_or(true, |q| *q != query);
    let active = if query_changed {
        if forward {
            0
        } else {
            total - 1
        }
    } else {
        let prev = FIND_ACTIVE.lock().map_or(0, |i| *i).min(total - 1);
        if forward {
            (prev + 1) % total
        } else {
            (prev + total - 1) % total
        }
    };
    let target_line = matches[active].0;
    if let Ok(mut q) = FIND_QUERY.lock() {
        *q = query.to_string();
    }
    if let Ok(mut m) = FIND_MATCHES.lock() {
        *m = matches;
    }
    if let Ok(mut a) = FIND_ACTIVE.lock() {
        *a = active;
    }
    scroll_to_grid_line(target_line);
    (active + 1, total)
}

/// Scroll the display so `line` sits near the middle of the screen.
fn scroll_to_grid_line(line: i32) {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
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
    }
}

/// Clear the find state (matches, active, query).
pub(crate) fn find_clear() {
    if let Ok(mut m) = FIND_MATCHES.lock() {
        m.clear();
    }
    if let Ok(mut a) = FIND_ACTIVE.lock() {
        *a = 0;
    }
    if let Ok(mut q) = FIND_QUERY.lock() {
        q.clear();
    }
}
