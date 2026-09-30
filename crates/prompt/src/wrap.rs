//! Word wrap for terminal text, shared by both renderers.
//!
//! Moved from `src-tauri/src/term_grid.rs`. It is a faithful port of the
//! frontend `WordWrapper` (`src/lib/wordWrap.ts`), and both run against
//! `fixtures/wrap/cases.json`, so the native grid and xterm break a line at
//! the same place. Complete lines wrap at the last whitespace before the
//! width, the trailing partial line wraps and emits at once, and escape
//! sequences take no width.

/// Word wrap a chunk of terminal text at `cols`. Complete lines (any `\r`
/// or `\n` terminator) wrap in place, and the trailing partial line wraps
/// and emits at once.
pub fn wrap_stream(text: &str, cols: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut line = String::new();
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            out.push_str(&wrap_line(&line, cols));
            out.push(ch);
            line.clear();
        } else {
            line.push(ch);
        }
    }
    out.push_str(&wrap_line(&line, cols));
    out
}

/// Walk `line` once, tracking the visible column with escape sequences at
/// zero width. When the column passes `cols`, put a CRLF in place of the
/// last whitespace so the wrap lands between words. A single token wider
/// than the width breaks at the edge so the line still ends.
pub fn wrap_line(line: &str, cols: usize) -> String {
    enum AnsiState {
        Normal,
        Esc,
        Csi,
        Osc,
    }
    let cols = cols.max(1);
    let mut out = String::with_capacity(line.len());
    let mut visible_col = 0usize;
    // Byte position in `out` of the most recent whitespace on this line,
    // with its visible column. None when the line (or the current wrapped
    // segment) starts with a word.
    let mut last_ws: Option<(usize, usize)> = None;
    let mut state = AnsiState::Normal;

    for ch in line.chars() {
        match state {
            AnsiState::Esc => {
                out.push(ch);
                state = match ch {
                    '[' => AnsiState::Csi,
                    ']' => AnsiState::Osc,
                    _ => AnsiState::Normal,
                };
                continue;
            }
            AnsiState::Csi => {
                out.push(ch);
                if ('\u{40}'..='\u{7e}').contains(&ch) {
                    state = AnsiState::Normal;
                }
                continue;
            }
            AnsiState::Osc => {
                out.push(ch);
                if ch == '\u{07}' || ch == '\u{9c}' {
                    state = AnsiState::Normal;
                } else if ch == '\u{1b}' {
                    state = AnsiState::Esc;
                }
                continue;
            }
            AnsiState::Normal => {}
        }
        if ch == '\u{1b}' {
            out.push(ch);
            state = AnsiState::Esc;
            continue;
        }

        out.push(ch);
        visible_col += 1;

        if ch == ' ' || ch == '\t' {
            last_ws = Some((out.len() - 1, visible_col));
        }

        if visible_col > cols {
            if let Some((ws_pos, ws_col)) = last_ws {
                // Put CRLF in place of the whitespace so the wrap lands
                // between words. Everything after it starts the next line.
                out.replace_range(ws_pos..=ws_pos, "\r\n");
                visible_col -= ws_col;
                last_ws = None;
            } else {
                // A single token wider than the terminal. Break at the
                // edge, keeping the current char on the new line.
                let pos = out.len() - ch.len_utf8();
                out.insert_str(pos, "\r\n");
                visible_col = 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        cols: usize,
        input: String,
        expected: String,
    }

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    #[test]
    fn wraps_every_case_in_the_shared_fixture() {
        let text = include_str!("../../../fixtures/wrap/cases.json");
        let fixture: Fixture = serde_json::from_str(text).expect("the wrap fixture parses");
        assert!(!fixture.cases.is_empty());
        for case in fixture.cases {
            assert_eq!(
                wrap_stream(&case.input, case.cols),
                case.expected,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn wrap_line_leaves_line_ends_to_the_stream() {
        assert_eq!(
            wrap_line("the quick brown fox", 10),
            "the quick\r\nbrown fox"
        );
        assert_eq!(wrap_line("abcdefgh", 5), "abcde\r\nfgh");
    }
}
