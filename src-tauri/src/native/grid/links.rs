//! The web links in the grid's text, for Cmd+click and the hover
//! underline.

use std::sync::OnceLock;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};

use super::with_grid_mut;
use crate::sessions::SessionId;

/// Compiled URL matcher, built once.
fn url_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"https?://[^\s<>()\[\]]+").expect("valid url regex"))
}

/// The URL spanning the cell at (`grid_line`, `col`) of the grid of
/// `session` as (url, start column, end column), if any. Trailing
/// sentence punctuation is trimmed. Used by Cmd+click and the hover
/// underline on the surface.
pub(crate) fn url_at(
    session: SessionId,
    grid_line: i32,
    col: usize,
) -> Option<(String, usize, usize)> {
    with_grid_mut(session, |g| {
        let grid = g.term.grid();
        if Line(grid_line) < grid.topmost_line() || Line(grid_line) > grid.bottommost_line() {
            return None;
        }
        let cols = grid.columns();
        let text: String = (0..cols)
            .map(|c| grid[Line(grid_line)][Column(c)].c)
            .collect();
        url_in_line(&text, col)
    })
    .flatten()
}

/// The URL spanning char index `col` in a grid line's text. `text` holds one
/// char per grid column (wide-char spacer cells read as a space), so char
/// offsets are grid columns even with double-width glyphs on the line.
pub(super) fn url_in_line(text: &str, col: usize) -> Option<(String, usize, usize)> {
    for m in url_regex().find_iter(text) {
        let start_col = text[..m.start()].chars().count();
        let end_col = text[..m.end()].chars().count();
        if col >= start_col && col < end_col {
            let url = m
                .as_str()
                .trim_end_matches(['.', ',', ')', ']', '!', '?'])
                .to_string();
            let trimmed_end = start_col + url.chars().count();
            return Some((url, start_col, trimmed_end));
        }
    }
    None
}
