use super::find::collect_matches;
use super::links::url_in_line;
use super::regions::{erase_back, find_mark, CursorReport, Mark, RegionStart};
use super::*;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// The session the app starts with, whose grid shows.
const ONE: SessionId = SessionId::FIRST;

/// A second session's number, as the session map gives it.
fn two() -> SessionId {
    let mut sessions = crate::sessions::Sessions::default();
    let defaults = sessions.selected().profile();
    sessions.open(defaults).id
}

fn cell_fg(g: &TermGrid, line: usize, col: usize) -> Color {
    g.term.grid()[Line(line as i32)][Column(col)].fg
}

#[test]
fn plain_text_lands_in_the_grid() {
    let mut g = TermGrid::new(80, 24);
    g.feed(b"hello");
    assert_eq!(&g.row_string(0)[..5], "hello");
    assert_eq!(g.char_at(0, 0), 'h');
}

#[test]
fn crlf_moves_to_the_next_row() {
    let mut g = TermGrid::new(80, 24);
    g.feed(b"ab\r\ncd");
    assert_eq!(&g.row_string(0)[..2], "ab");
    assert_eq!(&g.row_string(1)[..2], "cd");
}

#[test]
fn sgr_sets_the_foreground_color() {
    let mut g = TermGrid::new(80, 24);
    g.feed(b"\x1b[31mR");
    assert_eq!(cell_fg(&g, 0, 0), Color::Named(NamedColor::Red));
}

#[test]
fn semicolon_truecolor_sgr_sets_spec_fg() {
    use alacritty_terminal::vte::ansi::Rgb;
    let mut g = TermGrid::new(80, 24);
    g.feed(b"\x1b[38;2;100;100;100mX");
    assert_eq!(
        cell_fg(&g, 0, 0),
        Color::Spec(Rgb {
            r: 100,
            g: 100,
            b: 100
        })
    );
}

/// The underline kind of each of the first `n` cells on the top row.
fn underlines(g: &TermGrid, n: usize) -> Vec<Underline> {
    (0..n)
        .map(|col| g.cell_at_line(0, col).3.underline)
        .collect()
}

#[test]
fn each_underline_kind_comes_through_from_the_sgr_bytes() {
    let mut g = TermGrid::new(80, 24);
    // SGR 4 and each 4:x sub parameter, then 4:0 and 24 to clear.
    g.feed(b"\x1b[4mA\x1b[4:1mB\x1b[4:2mC\x1b[4:3mD\x1b[4:4mE\x1b[4:5mF");
    g.feed(b"\x1b[4:0mG\x1b[4:3mH\x1b[24mI");
    assert_eq!(
        underlines(&g, 9),
        vec![
            Underline::Single,
            Underline::Single,
            Underline::Double,
            Underline::Curly,
            Underline::Dotted,
            Underline::Dashed,
            Underline::None,
            Underline::Curly,
            Underline::None,
        ]
    );
}

#[test]
fn a_new_underline_kind_replaces_the_last_one() {
    let mut g = TermGrid::new(80, 24);
    g.feed(b"\x1b[4:2m\x1b[4:4mA\x1b[0mB");
    assert_eq!(underlines(&g, 2), vec![Underline::Dotted, Underline::None]);
}

#[test]
fn the_underline_color_comes_through_in_every_sgr_58_form() {
    use alacritty_terminal::vte::ansi::Rgb;
    let rose = Color::Spec(Rgb {
        r: 191,
        g: 97,
        b: 106,
    });
    let mut g = TermGrid::new(80, 24);
    // Colon true color with the empty color space id, semicolon true
    // color, a 256 color index, 59 back to the text color, then a
    // reset that drops the color with the underline.
    g.feed(b"\x1b[4:3;58:2::191:97:106mA");
    g.feed(b"\x1b[58;2;191;97;106mB");
    g.feed(b"\x1b[58;5;196mC");
    g.feed(b"\x1b[59mD");
    g.feed(b"\x1b[58;2;1;2;3m\x1b[0mE");
    let color = |col| g.cell_at_line(0, col).3.underline_color;
    assert_eq!(color(0), Some(rose));
    assert_eq!(color(1), Some(rose));
    assert_eq!(color(2), Some(Color::Indexed(196)));
    assert_eq!(color(3), None);
    assert_eq!(color(4), None);
    // The color rides with the curl it was set with.
    assert_eq!(g.cell_at_line(0, 0).3.underline, Underline::Curly);
}

#[test]
fn the_strike_and_hidden_flags_come_through_and_clear() {
    let mut g = TermGrid::new(80, 24);
    g.feed(b"\x1b[9mS\x1b[29m\x1b[8mH\x1b[28mV");
    let flags = |col| g.cell_at_line(0, col).3;
    assert!(flags(0).strikeout && !flags(0).hidden);
    assert!(flags(1).hidden && !flags(1).strikeout);
    assert!(!flags(2).hidden && !flags(2).strikeout);
}

#[test]
fn blink_marks_the_cell_and_overline_leaves_no_mark() {
    // SGR 5 blinks, and 25 or a reset ends it, in the order the
    // parameters come. The rapid 6 draws steady, as xterm draws it.
    // alacritty's parser has no SGR 53, so an overlined cell reads as
    // plain text.
    let mut g = TermGrid::new(80, 24);
    g.feed(b"\x1b[6mA\x1b[5mB\x1b[25mC\x1b[53mD\x1b[5;0mE\x1b[0;5mF\x1b[mG");
    let blinks: Vec<bool> = (0..7).map(|col| g.cell_at_line(0, col).3.blink).collect();
    assert_eq!(blinks, [false, true, false, false, false, true, false]);
    for col in 0..7 {
        let (_, fg, bg, flags) = g.cell_at_line(0, col);
        assert_eq!(fg, Color::Named(NamedColor::Foreground));
        assert_eq!(bg, Color::Named(NamedColor::Background));
        assert_eq!(flags.underline, Underline::None);
        assert!(!flags.bold && !flags.inverse && !flags.hidden);
    }
    assert_eq!(&g.row_string(0)[..7], "ABCDEFG");
}

#[test]
fn a_five_inside_a_color_never_blinks() {
    // A 5 that a 38, 48 or 58 takes as its own is part of the color,
    // as alacritty reads it. So is any number of a colon color, and
    // 5 with a sub parameter is no blink.
    let mut g = TermGrid::new(80, 24);
    g.feed(b"\x1b[38;5;5mA\x1b[48;2;5;5;5mB\x1b[58;5;5mC\x1b[38:5:5mD\x1b[5:1mE");
    // A color that ends early hands the rest back as styles.
    g.feed(b"\x1b[0;38;2;300;5mF\x1b[0m");
    let blinks: Vec<bool> = (0..6).map(|col| g.cell_at_line(0, col).3.blink).collect();
    assert_eq!(blinks, [false, false, false, false, false, true]);
}

#[test]
fn the_blink_mark_survives_a_split_write_and_a_reflow() {
    let mut g = TermGrid::new(10, 4);
    g.feed(b"ab\x1b[");
    g.feed(b"5mcd\x1b[0mef");
    assert!(g.cell_at_line(0, 2).3.blink && g.cell_at_line(0, 3).3.blink);
    assert!(!g.cell_at_line(0, 4).3.blink);
    // Narrower, the row wraps and the blinking cells keep their mark.
    g.resize(3, 4);
    assert_eq!(g.char_at(0, 2), 'c');
    assert!(g.cell_at_line(0, 2).3.blink && g.cell_at_line(1, 0).3.blink);
    assert!(!g.cell_at_line(1, 1).3.blink);
    // The mark is a bit alacritty leaves free.
    assert!(Flags::from_bits(BLINK.bits()).is_none());
}

#[test]
fn a_synchronized_update_blinks_in_the_order_it_was_written() {
    // The parser holds an update's bytes back and reads them at its
    // end, blink and reset alike.
    let mut g = TermGrid::new(10, 4);
    g.feed(b"\x1b[?2026h\x1b[5mA\x1b[0mB\x1b[5mC\x1b[?2026l\x1b[25mD");
    let blinks: Vec<bool> = (0..4).map(|col| g.cell_at_line(0, col).3.blink).collect();
    assert_eq!(blinks, [true, false, true, false]);
    assert_eq!(&g.row_string(0)[..4], "ABCD");
}

/// Whether two grids hold the same terminal: cursor, saved cursor,
/// modes, cursor look, colors, and every cell on screen and in
/// history. A link's id counts up across terminals, so a link
/// compares by its address.
fn assert_same_terminal(a: &TermGrid, b: &TermGrid, step: &str) {
    let (ga, gb) = (a.term.grid(), b.term.grid());
    assert_eq!(ga.cursor, gb.cursor, "cursor after {step}");
    assert_eq!(
        ga.saved_cursor, gb.saved_cursor,
        "saved cursor after {step}"
    );
    assert_eq!(a.term.mode(), b.term.mode(), "modes after {step}");
    assert_eq!(
        a.term.cursor_style(),
        b.term.cursor_style(),
        "cursor look after {step}"
    );
    for i in 0..alacritty_terminal::term::color::COUNT {
        assert_eq!(
            a.term.colors()[i],
            b.term.colors()[i],
            "color {i} after {step}"
        );
    }
    assert_eq!(ga.history_size(), gb.history_size(), "history after {step}");
    let look = |cell: &Cell| {
        let link = cell.hyperlink().map(|link| link.uri().to_owned());
        let marks = cell.zerowidth().map(<[char]>::to_vec);
        let line = cell.underline_color();
        (cell.c, cell.fg, cell.bg, cell.flags, line, link, marks)
    };
    let top = -i32::try_from(ga.history_size()).unwrap();
    for line in top..i32::try_from(ga.screen_lines()).unwrap() {
        for col in 0..ga.columns() {
            let (x, y) = (&ga[Line(line)][Column(col)], &gb[Line(line)][Column(col)]);
            assert_eq!(look(x), look(y), "line {line} col {col} after {step}");
        }
    }
}

#[test]
fn the_blink_handler_hands_every_other_sequence_to_the_terminal() {
    // Text, moves, erases, scrolls, tabs, modes, a scroll region,
    // charsets, colors, a link, the cursor's look, keyboard modes and
    // a reset, each a step that leaves its own mark. Through
    // `Blinking` the terminal must stay as it is straight from the
    // parser after every step, so a `Handler` method the wrapper
    // drops, as on an update of alacritty_terminal that adds one,
    // shows up here. Only the calls the terminal keeps nothing for,
    // such as the title, the bell and the reports, cannot.
    let steps: &[&[u8]] = &[
        b"Before",
        b"\x1bc",
        b"\x1b#8",
        b"\x1b[2J",
        b"\x1b[H",
        b"\x1b[1;31;44mHello\x1b[0m",
        b"\x1b[3;5HX",
        b"\x1b[2dY",
        b"\x1b[10GZ",
        b"\x1b[1;1H\x1b[2@",
        b"\x1b[4;4H\x1b[AU",
        b"\x1b[BD",
        b"\x1b[2CR",
        b"\x1b[DL",
        b"\x1b[Ee",
        b"\x1b[Ff",
        b"\r\ta",
        b"\x1b[Ib",
        b"\x1b[Zc",
        b"\x08x",
        b"\ry",
        b"\nz",
        b"\x1bEw",
        b"\x1b[6;4H\x1bH\r\t!",
        b"\x1b[3g\r\t?",
        b"\x1b[S",
        b"\x1b[T",
        b"\x1b[2;1H\x1b[L",
        b"\x1b[M",
        b"\x1b[1;1Habcdefghij\r\nklmnopqrst\r\nuvwxyz",
        b"\x1b[1;2H\x1b[2X",
        b"\x1b[P",
        b"\x1b[3;3H\x1b7\x1b[5;5Hs",
        b"\x1b8r",
        b"\x1b[2;2H\x1b[s\x1b[4;4H\x1b[u!",
        b"\x1b[2;8H\x1b[K",
        b"\x1b[1;4H\x1b[1K",
        b"\x1b[2;3H\x1b[1J",
        b"\x1b[1;1H\x1bMq",
        b"\x1b[3;1H\x1b[4hI",
        b"\x1b[4lJ",
        b"\x1b[?7l\x1b[4;9Hwrapping",
        b"\x1b[?7h\x1b[?1h\x1b=",
        b"\x1b>",
        b"\x1b[2;4r",
        b"\x1b[?6h\x1b[1;1Ho",
        b"\x1b[?6l\x1b[4;1H\r\nnew",
        b"\x1b[r\x1b[6;1H",
        b"\x1b(0lqk",
        b"\x1b(B\x1b)0\x0eq",
        b"\x0fj",
        b"\x1b]4;1;rgb:12/34/56\x07",
        b"\x1b]4;2;rgb:65/43/21\x07\x1b]104;2\x07",
        b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07",
        b"\x1b]2;title\x07\x1b[22t\x1b[23t",
        b"\x1b[3 q",
        b"\x1b]50;CursorShape=1\x07",
        b"\x1b[>1u\x1b[>4;1m",
        b"\x1a\x07",
    ];
    let mut wrapped = TermGrid::new(12, 6);
    let mut plain = TermGrid::new(12, 6);
    for bytes in steps {
        wrapped.feed(bytes);
        for &byte in *bytes {
            plain.parser.advance(&mut plain.term, byte);
        }
        assert_same_terminal(&wrapped, &plain, &String::from_utf8_lossy(bytes));
    }
}

#[test]
fn bracket_right_after_truecolor_renders() {
    let mut g = TermGrid::new(80, 24);
    g.feed(b"\x1b[38;2;100;100;100m[\x1b[0mABC");
    assert_eq!(g.char_at(0, 0), '[');
    assert_eq!(g.char_at(0, 1), 'A');
    assert_eq!(g.char_at(0, 2), 'B');
    assert_eq!(g.char_at(0, 3), 'C');
}

#[test]
fn long_line_wraps_to_the_next_row() {
    let mut g = TermGrid::new(4, 24);
    g.feed(b"abcdef");
    assert_eq!(&g.row_string(0)[..4], "abcd");
    assert_eq!(&g.row_string(1)[..2], "ef");
}

#[test]
fn each_session_seeds_its_grid_from_scrollback_once() {
    let _shared = lock_shared_grid_for_test();
    let two = two();
    assert!(claim_seed(ONE));
    assert!(!claim_seed(ONE));
    assert!(claim_seed(two));
    assert!(!claim_seed(two));
    assert!(!claim_seed(ONE));
}

#[test]
fn a_local_write_creates_and_fills_the_shared_grid() {
    let _shared = lock_shared_grid_for_test();
    feed_local(ONE, b"shared");
    let row = with_grid_mut(ONE, |g| g.row_string(0)).expect("grid created on first feed");
    assert!(row.starts_with("shared"));
}

#[test]
fn a_local_write_names_the_newest_output_of_the_stage_the_grid_took() {
    let _shared = lock_shared_grid_for_test();
    blank_shared_grid_for_test(40, 10);
    assert_eq!(feed_local(ONE, b"restored\r\n"), 0, "none yet");
    let mut first = Output::new(false);
    first.text(&marked(1, b"<1020hp> "));
    feed_session_output(ONE, &first, Some(first.id()));
    // Output from elsewhere, such as a slash command's echo, is none
    // of the stage's.
    let mut other = Output::new(false);
    other.text(b"[not connected]\r\n");
    feed_session_output(ONE, &other, None);
    assert_eq!(feed_local(ONE, b"look\r\n"), first.id());
    let mut next = Output::new(false);
    next.text(&marked(2, b"<1000hp> "));
    feed_session_output(ONE, &next, Some(next.id()));
    assert_eq!(feed_local(ONE, b"x"), next.id());
}

/// The rows on the screen of the grid of `session`, trailing blanks
/// trimmed.
fn rows_of(session: SessionId) -> Vec<String> {
    screen_rows(session).map(|r| r.rows).unwrap_or_default()
}

#[test]
fn two_sessions_write_each_to_a_grid_of_its_own() {
    let _shared = lock_shared_grid_for_test();
    let two = two();
    feed_session_output(ONE, &text(b"You rest.\r\n"), None);
    feed_session_output(two, &text(b"You wake.\r\n"), None);
    feed_local(two, b"look\r\n");
    assert_eq!(rows_of(ONE)[..2], ["You rest.", ""]);
    assert_eq!(rows_of(two)[..3], ["You wake.", "look", ""]);
    // A scroll or a selection in one grid leaves the other's alone.
    for n in 0..30 {
        feed_session_output(two, &text(format!("{n}\r\n").as_bytes()), None);
    }
    scroll(two, 3);
    start_selection(two, 0, 0);
    update_selection(two, 1, 2);
    assert!(reader_busy(two));
    assert!(!reader_busy(ONE));
    assert_eq!(selection_text(ONE), None);
}

#[test]
fn a_find_in_one_session_leaves_the_others_matches() {
    let _shared = lock_shared_grid_for_test();
    let two = two();
    feed_session_output(ONE, &text(b"a goblin\r\nan orc\r\na goblin\r\n"), None);
    feed_session_output(two, &text(b"an orc\r\n"), None);
    assert_eq!(
        find::find_run(ONE, "goblin", false, false, false, true),
        (1, 2)
    );
    assert_eq!(
        find::find_run(two, "orc", false, false, false, true),
        (1, 1)
    );
    find::find_clear(two);
    let ones = with_shown(|shown| shown.expect("the first grid").find().snapshot());
    assert_eq!(ones, (vec![(0, 2, 8), (2, 2, 8)], Some((0, 2, 8))));
    // The same query again steps on in its own session only.
    assert_eq!(
        find::find_run(ONE, "goblin", false, false, false, true),
        (2, 2)
    );
    assert_eq!(
        find::find_run(two, "goblin", false, false, false, true),
        (0, 0)
    );
}

#[test]
fn selecting_a_session_shows_its_grid_and_sizes_only_the_hidden_one() {
    let _shared = lock_shared_grid_for_test();
    let state = crate::app::state::AppState::default();
    let two = state.open_session(state.selected_session().profile()).id;
    feed_session_output(ONE, &text(b"You rest.\r\n"), None);
    feed_session_output(two, &text(b"You wake.\r\n"), None);
    set_prompt_bands(two, true);
    assert_eq!(shared_screen_rows_for_test()[0], "You rest.");
    assert_eq!(state.select_session(two), Ok(()));
    assert_eq!(super::shown(), two);
    // A frame draws the second session's rows and its bands, and sizes
    // its grid alone.
    assert_eq!(shared_screen_rows_for_test()[0], "You wake.");
    assert!(with_shown(
        |shown| shown.is_some_and(SessionGrid::prompt_bands)
    ));
    resize_grid(60, 20);
    // The hidden grid takes the size its window size gives, and the
    // shown one keeps the frame's.
    size_hidden(ONE, 40, 12);
    size_hidden(two, 100, 50);
    let size = |session| with_grid_mut(session, |g| (g.columns(), g.screen_lines()));
    assert_eq!((size(ONE), size(two)), (Some((40, 12)), Some((60, 20))));
    // Selecting the first again shows its rows as they were.
    assert_eq!(state.select_session(ONE), Ok(()));
    assert_eq!(shared_screen_rows_for_test()[0], "You rest.");
}

/// A mock app with the app state, for the terminal commands.
#[cfg(native_surface)]
fn app() -> tauri::App<tauri::test::MockRuntime> {
    use tauri::Manager;
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    app.manage::<crate::app::state::SharedState>(std::sync::Arc::default());
    app
}

// The command reads the grid only where the surface draws it.
#[cfg(native_surface)]
#[test]
fn terminal_cursor_reports_the_shared_grid() {
    use tauri::Manager;
    let _shared = lock_shared_grid_for_test();
    let app = app();
    let cursor = || crate::ipc::terminal::terminal_cursor(app.state(), None);
    assert_eq!(cursor(), Ok(None), "no grid yet");
    blank_shared_grid_for_test(40, 10);
    let mut out = Output::new(false);
    out.text(b"You are hungry.\r\n");
    out.text(&marked(3, b"<1020hp> "));
    feed_session_output(ONE, &out, Some(out.id()));
    let report = serde_json::to_value(cursor().expect("the session")).expect("json");
    assert_eq!(
        report,
        serde_json::json!({
            "line": 1,
            "col": 9,
            "at_bottom": true,
            "cols": 40,
            "region": {"gen": 3, "line": 1, "col": 0},
        })
    );
    feed_local(ONE, b"look\r\n");
    assert_eq!(cursor_report(ONE).and_then(|r| r.region), None);
}

// The command reads the grid only where the surface draws it.
#[cfg(native_surface)]
#[test]
fn terminal_screen_rows_reads_the_shared_screen_as_text() {
    use tauri::Manager;
    let _shared = lock_shared_grid_for_test();
    let app = app();
    let rows = || crate::ipc::terminal::terminal_screen_rows(app.state(), None);
    assert_eq!(rows(), Ok(None), "no grid yet");
    blank_shared_grid_for_test(20, 4);
    let mut out = Output::new(false);
    out.text("You rest.\r\n<1020hp> 中文 ".as_bytes());
    feed_session_output(ONE, &out, Some(out.id()));
    let report = serde_json::to_value(rows().expect("the session")).expect("json");
    // A wide character takes two cells and reads once.
    assert_eq!(
        report,
        serde_json::json!({
            "rows": ["You rest.", "<1020hp> 中文", "", ""],
            "cols": 20,
            "at_bottom": true,
        })
    );
}

#[test]
fn clear_history_drops_the_scrollback_and_keeps_the_screen() {
    let mut g = TermGrid::new(10, 2);
    g.feed(b"one\r\ntwo\r\nthree");
    g.scroll(1);
    assert!(g.scrollback_len() > 0);
    g.select_all();
    g.clear_history();
    assert_eq!(g.scrollback_len(), 0);
    assert_eq!(g.display_offset(), 0);
    assert!(g.selection_text().is_none());
    assert_eq!(g.row_string(0).trim_end(), "two");
    assert_eq!(g.row_string(1).trim_end(), "three");
}

#[test]
fn select_all_spans_scrollback_and_the_live_screen() {
    let mut g = TermGrid::new(10, 2);
    g.feed(b"one\r\ntwo\r\nthree");
    assert!(g.scrollback_len() > 0);
    g.select_all();
    let text = g.term.selection_to_string().expect("a selection");
    assert_eq!(text.trim_end(), "one\ntwo\nthree");
    let (start_line, start_col, end_line, _) = g.selection_bounds().expect("bounds");
    assert_eq!((start_line, start_col), (-1, 0));
    assert_eq!(end_line, 1);
}

#[test]
fn collect_matches_finds_plain_substrings_with_columns() {
    let mut g = TermGrid::new(80, 24);
    g.feed(b"the cat sat\r\nthe cat ran");
    let m = collect_matches(&g, "cat", false, false, false);
    assert_eq!(m.len(), 2);
    assert_eq!(m[0], (0, 4, 7));
    assert_eq!(m[1], (1, 4, 7));
}

#[test]
fn sim_prompt_then_echo_lands_after_prompt() {
    let mut g = TermGrid::new(80, 24);
    // Server blank line then the response block (as the line pipeline
    // emits them), then the gagged prompt replaced by the rendered
    // template WITHOUT trailing newline.
    g.feed(b"\r\nPlayers matched: 9\r\n\r\n");
    g.feed(
        vosh_prompt::wrap::wrap_stream(
            "\x1b[3m\x1b[38;5;240m[\x1b[0m329(\x1b[38;5;42m100%\x1b[0m)h\x1b[0m",
            80,
        )
        .as_bytes(),
    );
    // Local echo of a typed command, written at the cursor.
    g.feed(b"\x1b[38;2;200;200;100mwho\x1b[0m\r\n");
    let row3 = g.row_string(3);
    eprintln!("row3: {:?}", row3.trim_end());
    assert!(row3.starts_with("[329(100%)hwho"), "got: {row3:?}");
}

/// Values for a drawn prompt test. Health reads 1020 of 1020 and mana
/// is hidden.
struct PromptValues;

impl vosh_prompt::Values for PromptValues {
    fn resolve(&self, field: &vosh_prompt::FieldRef) -> vosh_prompt::Resolved {
        use vosh_prompt::{Resolved, Value};
        match field.name.as_str() {
            "hp" => Resolved::Value(Value::Gauge {
                cur: 1020,
                max: Some(1020),
                pct: None,
            }),
            "maxhp" => Resolved::Value(Value::Num(1020)),
            "mana" => Resolved::Hidden,
            _ => Resolved::Unknown,
        }
    }
}

#[test]
fn a_drawn_prompt_keeps_italic_across_c_default() {
    let out = vosh_prompt::render_str(
        "%s_italic%c_red%hp%c_default/%c_hp%{maxhp} %c_blue%mana!",
        &PromptValues,
        vosh_prompt::RenderOptions::default(),
    );
    let mut g = TermGrid::new(80, 24);
    g.feed(out.ansi.as_bytes());
    let cell = |col| g.cell_at_line(0, col);
    // 1020 in red, then the slash back in the text color, still italic.
    let (c, fg, _, flags) = cell(0);
    assert_eq!((c, fg), ('1', Color::Named(NamedColor::Red)));
    assert!(flags.italic);
    let (c, fg, _, flags) = cell(4);
    assert_eq!((c, fg), ('/', Color::Named(NamedColor::Foreground)));
    assert!(flags.italic);
    // Color by how full is the theme green at full health.
    let (c, fg, _, flags) = cell(5);
    assert_eq!((c, fg), ('1', Color::Named(NamedColor::Green)));
    assert!(flags.italic);
    // The hidden mark is bright black, and the blue before it comes back.
    let (c, fg, _, flags) = cell(10);
    assert_eq!((c, fg), ('?', Color::Named(NamedColor::BrightBlack)));
    assert!(flags.italic);
    let (c, fg, _, _) = cell(11);
    assert_eq!((c, fg), ('!', Color::Named(NamedColor::Blue)));
    // The render ends in a reset, so what follows is plain.
    g.feed(b"x");
    let (c, fg, _, flags) = g.cell_at_line(0, 12);
    assert_eq!((c, fg), ('x', Color::Named(NamedColor::Foreground)));
    assert!(!flags.italic);
}

/// A session output that writes `bytes`.
fn text(bytes: &[u8]) -> Output {
    let mut out = Output::new(false);
    out.text(bytes);
    out
}

/// A session output that replaces region `gen` with `bytes`.
fn replace(gen: u64, bytes: &[u8], fresh: bool) -> Output {
    let mut out = Output::new(false);
    out.replace(gen, bytes.to_vec(), fresh);
    out
}

fn marked(gen: u64, bytes: &[u8]) -> Vec<u8> {
    [vosh_prompt::stage::mark(gen).as_slice(), bytes].concat()
}

/// The screen's rows, trailing blanks trimmed, up to the last row
/// that shows anything.
fn screen(g: &TermGrid) -> Vec<String> {
    let mut rows: Vec<String> = (0..g.screen_lines())
        .map(|line| g.row_string(line).trim_end().to_string())
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

/// An output that writes `bytes` and holds `hold` back.
fn held(bytes: &[u8], hold: &[u8]) -> Output {
    let mut out = Output::new(false);
    out.bytes = bytes.to_vec();
    out.hold = hold.to_vec();
    out
}

fn cursor(g: &TermGrid) -> (i32, usize) {
    g.cursor()
}

fn lift(id: u64, inner: &[u8]) -> Vec<u8> {
    [
        vosh_prompt::stage::lift_start(id).as_slice(),
        inner,
        &vosh_prompt::stage::lift_end(id),
    ]
    .concat()
}

/// Every lift row on the grid, as (id, line, first, end).
fn spans(g: &TermGrid) -> Vec<(u64, i32, usize, usize)> {
    let top = g.term.grid().topmost_line().0;
    g.lift_spans(top, g.screen_lines() as i32)
        .into_iter()
        .map(|s| (s.id, s.line, s.first, s.end))
        .collect()
}

#[test]
fn a_lift_tags_its_cells_and_leaves_your_echo_plain() {
    let mut g = TermGrid::new(40, 10);
    let mut prompt = b"room\r\n\r\n".to_vec();
    prompt.extend(lift(
        3,
        &[b"Tester: [===]\r\n".as_slice(), &marked(4, b"<1020hp>")].concat(),
    ));
    prompt.push(b' ');
    g.session_output(&text(&prompt));
    g.local_write(b"look\r\n");
    assert_eq!(screen(&g), ["room", "", "Tester: [===]", "<1020hp> look"]);
    assert_eq!(spans(&g), [(3, 2, 0, 13), (3, 3, 0, 8)]);
    // The space after the end mark and your echo carry no tag.
    let grid = g.term.grid();
    assert!(grid[Line(3)][Column(8)].hyperlink().is_none());
    assert!(grid[Line(3)][Column(9)].hyperlink().is_none());
}

#[test]
fn a_lift_row_says_when_your_echo_shows_after_it() {
    let mut g = TermGrid::new(40, 10);
    let mut prompt = lift(
        3,
        &[b"Tester: [===]\r\n".as_slice(), &marked(4, b"<1020hp>")].concat(),
    );
    prompt.push(b' ');
    g.session_output(&text(&prompt));
    let after =
        |g: &TermGrid| -> Vec<bool> { g.lift_spans(0, 10).into_iter().map(|s| s.after).collect() };
    assert_eq!(after(&g), [false, false], "nothing after it yet");
    g.local_write(b"look\r\n");
    assert_eq!(after(&g), [false, true]);
}

#[test]
fn a_lift_that_scrolls_the_screen_between_its_marks_still_starts_right() {
    let mut g = TermGrid::new(20, 3);
    let body = lift(1, b"one\r\ntwo\r\nthree");
    g.session_output(&text(&[b"a\r\nb\r\nc\r\n".as_slice(), &body].concat()));
    assert_eq!(screen(&g), ["one", "two", "three"]);
    assert_eq!(spans(&g), [(1, 0, 0, 3), (1, 1, 0, 3), (1, 2, 0, 5)]);
    // Word wrapped by the grid, a long lift covers every row it takes.
    let mut g = TermGrid::new(10, 4);
    g.session_output(&text(&lift(2, b"1020/1020hp 800/800mn")));
    assert_eq!(screen(&g), ["1020/1020h", "p", "800/800mn"]);
    assert_eq!(spans(&g), [(2, 0, 0, 10), (2, 1, 0, 1), (2, 2, 0, 9)]);
}

#[test]
fn a_repaint_tags_the_new_prompt_from_its_region() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(&lift(
        1,
        &[b"Tank\r\n".as_slice(), &marked(2, b"<1020hp>")].concat(),
    )));
    g.session_output(&replace(
        2,
        &[
            marked(3, b"<999hp 800m>").as_slice(),
            &vosh_prompt::stage::lift_end(1),
        ]
        .concat(),
        false,
    ));
    assert_eq!(screen(&g), ["Tank", "<999hp 800m>"]);
    assert_eq!(spans(&g), [(1, 0, 0, 4), (1, 1, 0, 12)]);
}

#[test]
fn a_repaint_that_wraps_tags_every_row_it_takes() {
    // At 8 columns the new prompt wraps under the head line.
    let mut g = TermGrid::new(8, 10);
    g.session_output(&text(&lift(
        1,
        &[b"Tank\r\n".as_slice(), &marked(2, b"<1020hp>")].concat(),
    )));
    assert_eq!(spans(&g), [(1, 0, 0, 4), (1, 1, 0, 8)]);
    g.session_output(&replace(
        2,
        &[
            marked(3, b"<999hp 800m>").as_slice(),
            &vosh_prompt::stage::lift_end(1),
        ]
        .concat(),
        false,
    ));
    assert_eq!(screen(&g), ["Tank", "<999hp", "800m>"]);
    assert_eq!(spans(&g), [(1, 0, 0, 4), (1, 1, 0, 6), (1, 2, 0, 5)]);
    // A repaint back to one row leaves no tag on the row below.
    g.session_output(&replace(
        3,
        &[
            marked(4, b"<1020hp>").as_slice(),
            &vosh_prompt::stage::lift_end(1),
        ]
        .concat(),
        false,
    ));
    assert_eq!(screen(&g), ["Tank", "<1020hp>"]);
    assert_eq!(spans(&g), [(1, 0, 0, 4), (1, 1, 0, 8)]);
}

#[test]
fn tags_stay_on_their_text_through_history_and_a_resize() {
    let mut g = TermGrid::new(30, 4);
    g.session_output(&text(&lift(5, b"1020/1020hp 800/800mn")));
    g.session_output(&text(b"\r\nx\r\ny\r\nz\r\nw\r\n"));
    // Scrolled into history, the tags are still there.
    assert_eq!(spans(&g), [(5, -2, 0, 21)]);
    // Reflowed at 12 the space ends the first row, and a span ends at
    // its last glyph.
    g.resize(12, 4);
    let rows: Vec<(u64, usize)> = spans(&g).into_iter().map(|s| (s.0, s.3 - s.2)).collect();
    assert_eq!(rows, [(5, 11), (5, 9)]);
    g.resize(40, 4);
    assert_eq!(spans(&g).len(), 1);
}

#[test]
fn a_lift_that_starts_past_a_full_row_starts_on_the_next() {
    let mut g = TermGrid::new(10, 4);
    g.local_write(b"0123456789");
    g.session_output(&text(&lift(1, b"prompt")));
    assert_eq!(screen(&g), ["0123456789", "prompt"]);
    assert_eq!(spans(&g), [(1, 1, 0, 6)]);
}

#[test]
fn held_line_ends_wait_until_the_next_write_lands() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&held(b"room\r\n[Exits: south]", b"\r\n\r\n"));
    assert_eq!(screen(&g), ["room", "[Exits: south]"]);
    assert_eq!(cursor(&g), (1, 14), "the text ends on its last line");
    assert_eq!(g.pending_hold(), b"\r\n\r\n");
    // The next output writes them first, then holds its own.
    g.session_output(&held(b"tell", b"\r\n\r\n"));
    assert_eq!(screen(&g), ["room", "[Exits: south]", "", "tell"]);
    // Your echo takes them first too, once.
    g.local_write(b"look\r\n");
    g.local_write(b"x");
    assert_eq!(
        screen(&g),
        ["room", "[Exits: south]", "", "tell", "", "look", "x"]
    );
    let leftover = &g.pending_hold();
    assert!(leftover.is_empty(), "{leftover:?}");
}

/// The rows the grid shows at its display offset, trailing blanks
/// trimmed on each.
fn shown(g: &TermGrid) -> Vec<String> {
    (0..g.screen_lines())
        .map(|line| {
            let row: String = (0..g.columns()).map(|col| g.cell(line, col).0).collect();
            row.trim_end().to_string()
        })
        .collect()
}

#[test]
fn the_newest_line_stays_above_the_pinned_band_as_it_borrows_a_row() {
    // Thirty lines and a pinned prompt, whose line end the grid holds,
    // on a screen of ten rows. A fight grows the band by a row and the
    // grid gives up a row for it, then takes it back, three times.
    let lines: Vec<String> = (1..=30).map(|n| format!("line {n}")).collect();
    let mut g = TermGrid::new(40, 10);
    g.session_output(&held(lines.join("\r\n").as_bytes(), b"\r\n"));
    let calm = screen(&g);
    assert_eq!(calm.first().map(String::as_str), Some("line 21"));
    for _ in 0..3 {
        g.resize(40, 9);
        // The top line leaves for the scrollback and the newest stays
        // on the last row, right above the band.
        let fight = screen(&g);
        assert_eq!(fight.len(), 9);
        assert_eq!(fight.first().map(String::as_str), Some("line 22"));
        assert_eq!(fight.last().map(String::as_str), Some("line 30"));
        assert_eq!(cursor(&g), (8, 7), "the cursor stays after the newest line");
        g.resize(40, 10);
        // The line comes back at the top, and nothing else moved.
        assert_eq!(screen(&g), calm);
        assert_eq!(cursor(&g), (9, 7));
    }
    // The held line end still lands first when the next text comes.
    g.session_output(&held(b"tell", b"\r\n"));
    assert_eq!(screen(&g).last().map(String::as_str), Some("tell"));
    assert_eq!(
        screen(&g).iter().rev().nth(1).map(String::as_str),
        Some("line 30")
    );
}

#[test]
fn a_reader_scrolled_back_keeps_the_lines_in_view_as_the_band_borrows_a_row() {
    let lines: Vec<String> = (1..=60).map(|n| format!("line {n}")).collect();
    let mut g = TermGrid::new(40, 10);
    g.session_output(&held(lines.join("\r\n").as_bytes(), b"\r\n"));
    g.scroll(20);
    let reading = shown(&g);
    assert_eq!(reading.first().map(String::as_str), Some("line 31"));
    g.resize(40, 9);
    // The view keeps its top line, and the row the band took goes
    // from its bottom.
    assert_eq!(shown(&g), reading[..9]);
    g.resize(40, 10);
    assert_eq!(shown(&g), reading);
    assert_eq!(g.display_offset(), 20);
}

#[test]
fn an_output_that_writes_nothing_keeps_the_longer_hold() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&held(b"room", b"\r\n\r\n"));
    // A pulse whose lines were all swallowed or hidden.
    let mut band = held(b"", b"");
    band.pin = Some(b"<1020>".to_vec());
    g.session_output(&band);
    assert_eq!(g.pending_hold(), b"\r\n\r\n");
    // A shorter hold from such an output never shortens it, and a
    // longer one never stacks with it.
    g.session_output(&held(b"", b"\r\n"));
    assert_eq!(g.pending_hold(), b"\r\n\r\n");
    g.session_output(&held(b"", b"\r\n\r\n\r\n"));
    assert_eq!(g.pending_hold(), b"\r\n\r\n\r\n");
    assert_eq!(screen(&g), ["room"]);
    // Text in a later output replaces it with its own.
    g.session_output(&held(b"tell", b"\r\n"));
    assert_eq!(screen(&g), ["room", "", "", "tell"]);
    assert_eq!(g.pending_hold(), b"\r\n");
}

#[test]
fn a_replace_of_the_open_region_goes_before_the_hold_and_a_fresh_one_after() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(&marked(1, b"partial")));
    // Held line ends after an open region, as a later pulse with
    // nothing but line ends would leave them.
    g.session_output(&held(b"", b"\r\n"));
    g.session_output(&replace(1, &marked(2, b"WHOLE"), false));
    assert_eq!(screen(&g), ["WHOLE"]);
    assert_eq!(g.pending_hold(), b"\r\n");
    // A fresh replace of a closed region writes at the cursor, after
    // the held line ends.
    // Text lands after the held line end.
    g.session_output(&text(b"and more"));
    assert_eq!(screen(&g), ["WHOLE", "and more"]);
    g.session_output(&held(b"", b"\r\n\r\n"));
    g.session_output(&replace(9, b"fresh\r\n", true));
    assert_eq!(screen(&g), ["WHOLE", "and more", "", "fresh"]);
}

#[test]
fn a_replace_erases_on_the_default_background() {
    // The region's line ends on a background, and its color reset
    // waits with its line end, so the background is still in force
    // when the replace erases. Every cleared cell stays plain.
    let mut g = TermGrid::new(20, 6);
    g.session_output(&held(&marked(1, b"\x1b[44mhungry"), b"\x1b[0m\r\n"));
    g.session_output(&replace(1, &marked(2, b"\x1b[44mHUNGRY"), false));
    assert_eq!(screen(&g), ["HUNGRY"]);
    let plain = Color::Named(NamedColor::Background);
    assert_eq!(g.cell(0, 0).2, Color::Named(NamedColor::Blue));
    for line in 0..6 {
        for col in usize::from(line == 0) * 6..20 {
            assert_eq!(g.cell(line, col).2, plain, "row {line}, column {col}");
        }
    }
}

#[test]
fn a_replace_rewrites_the_open_region_where_it_starts() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(
        &[b"hungry\r\n".as_slice(), &marked(1, b"PROMPT")].concat(),
    ));
    g.session_output(&replace(1, &marked(2, b"NEW"), false));
    assert_eq!(screen(&g), ["hungry", "NEW"]);
    // The replace opened region 2, so the next one lands too.
    g.session_output(&replace(2, &marked(3, b"LAST"), false));
    assert_eq!(screen(&g), ["hungry", "LAST"]);
    // Region 2 is gone, so a replace for it is dropped.
    g.session_output(&replace(2, b"STALE", false));
    assert_eq!(screen(&g), ["hungry", "LAST"]);
}

#[test]
fn a_replace_after_a_local_write_is_dropped() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(
        &[b"hungry\r\n".as_slice(), &marked(1, b"PROMPT")].concat(),
    ));
    g.local_write(b"look\r\n");
    g.session_output(&replace(1, &marked(2, b"NEW"), false));
    // The echo stays and the prompt shows once.
    assert_eq!(screen(&g), ["hungry", "PROMPTlook"]);
}

#[test]
fn a_replace_after_other_output_is_dropped_unless_fresh() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(&marked(1, b"abc")));
    g.session_output(&text(b"xyz"));
    g.session_output(&replace(1, b"dropped", false));
    assert_eq!(screen(&g), ["abcxyz"]);
    // A fresh one goes on a new row, since the cursor is mid row.
    g.session_output(&replace(1, b"abcdef\r\n", true));
    assert_eq!(screen(&g), ["abcxyz", "abcdef"]);
    // At the start of a row it writes there.
    g.session_output(&text(&marked(2, b"You are hun")));
    g.local_write(b"look\r\n");
    g.session_output(&replace(2, b"You are hungry.\r\n", true));
    assert_eq!(
        screen(&g),
        ["abcxyz", "abcdef", "You are hunlook", "You are hungry."]
    );
}

#[test]
fn a_line_completing_a_painted_partial_replaces_it() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(&marked(1, b"You are hun")));
    let mut next = replace(1, b"You are hungry.\r\n", true);
    next.text(b"You feel better.\r\n");
    g.session_output(&next);
    assert_eq!(screen(&g), ["You are hungry.", "You feel better."]);
    // The completed line carries no mark, so nothing stays open.
    g.session_output(&replace(1, b"again", false));
    assert_eq!(screen(&g), ["You are hungry.", "You feel better."]);
}

#[test]
fn a_region_the_grid_wrapped_is_erased_whole() {
    let mut g = TermGrid::new(12, 10);
    g.session_output(&text(b"before\r\n"));
    g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
    assert_eq!(screen(&g).len(), 4, "{:?}", screen(&g));
    g.session_output(&replace(1, &marked(2, b"NEW"), false));
    assert_eq!(screen(&g), ["before", "NEW"]);
}

#[test]
fn a_replace_after_a_resize_counts_the_rows_at_the_new_width() {
    // A full screen, as in the app, so the prompt sits on the last row.
    let mut g = TermGrid::new(40, 6);
    g.session_output(&text(b"one\r\ntwo\r\nthree\r\nfour\r\nbefore\r\n"));
    g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
    assert_eq!(
        screen(&g),
        [
            "one",
            "two",
            "three",
            "four",
            "before",
            "[1020/1020hp 800/800mn 930/930mv]"
        ]
    );
    // Narrower, the prompt takes three rows.
    g.resize(12, 6);
    assert_eq!(
        screen(&g),
        [
            "three",
            "four",
            "before",
            "[1020/1020hp",
            " 800/800mn 9",
            "30/930mv]"
        ]
    );
    g.session_output(&replace(1, &marked(2, b"NEW"), false));
    assert_eq!(screen(&g), ["three", "four", "before", "NEW"]);
    // A prompt the word wrap broke keeps its break when the grid
    // widens again.
    g.session_output(&replace(2, &marked(3, b"[1020/1020hp 800/800mn]"), false));
    assert_eq!(
        screen(&g),
        ["three", "four", "before", "[1020/1020hp", "800/800mn]"]
    );
    g.resize(40, 6);
    g.session_output(&replace(3, &marked(4, b"WIDE"), false));
    let rows = screen(&g);
    assert_eq!(rows[rows.len() - 2..], ["before", "WIDE"], "{rows:?}");
}

#[test]
fn a_nearly_empty_screen_keeps_its_rows_when_the_grid_narrows() {
    // A partial painted near the top of the screen, a narrower grid,
    // then the line that completes it. The rows over the partial stay
    // on screen, so the grid erases the partial whole, as xterm does.
    let line = b"Some long line of text that wraps a few times at twelve.\r\n";
    for before in [&b""[..], b"one\r\ntwo\r\n"] {
        for (wide, narrow) in [(40, 12), (40, 20), (20, 7)] {
            let mut g = TermGrid::new(wide, 10);
            g.session_output(&text(before));
            g.session_output(&text(&marked(1, b"Some long line of te")));
            g.resize(narrow, 10);
            g.session_output(&replace(1, line, true));
            let mut expect = TermGrid::new(narrow, 10);
            expect.session_output(&text(&[before, line].concat()));
            let case = format!("{wide} to {narrow} wide after {before:?}");
            assert_eq!(screen(&g), screen(&expect), "{case}");
            assert_eq!(g.scrollback_len(), expect.scrollback_len(), "{case}");
        }
    }
    // The drawn prompt alone on the screen stays within reach too.
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
    g.resize(12, 10);
    g.session_output(&replace(1, &marked(2, b"NEW"), false));
    assert_eq!(screen(&g), ["NEW"]);
}

#[test]
fn a_region_a_resize_pushes_above_the_screen_counts_as_closed() {
    // A full screen of three rows, where the narrower prompt takes
    // five, so its start is in history, out of reach. A replace for
    // it is dropped, and a fresh one writes on a new row.
    let mut g = TermGrid::new(40, 3);
    g.session_output(&text(b"one\r\ntwo\r\n"));
    g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
    g.resize(8, 3);
    let before = screen(&g);
    g.session_output(&replace(1, b"dropped", false));
    assert_eq!(screen(&g), before);
    g.session_output(&replace(1, b"fresh", true));
    assert_eq!(screen(&g).last().map(String::as_str), Some("fresh"));
}

#[test]
fn a_mark_after_a_full_row_starts_its_region_on_the_next_row() {
    let mut g = TermGrid::new(10, 10);
    g.local_write(b"0123456789");
    g.session_output(&text(&marked(1, b"PROMPT")));
    assert_eq!(screen(&g), ["0123456789", "PROMPT"]);
    g.session_output(&replace(1, &marked(2, b"NEW"), false));
    assert_eq!(screen(&g), ["0123456789", "NEW"]);
    // A region that wrote nothing yet takes the replace where it is.
    g.local_write(b"\r\n0123456789");
    g.session_output(&text(&marked(3, b"")));
    g.session_output(&replace(3, &marked(4, b"HERE"), false));
    assert_eq!(screen(&g), ["0123456789", "NEW", "0123456789", "HERE"]);
}

/// The open region's start in a cursor report, as (gen, line, col).
fn region_at(g: &TermGrid) -> Option<(u64, i32, usize)> {
    g.cursor_report().region.map(|r| (r.gen, r.line, r.col))
}

#[test]
fn the_cursor_report_names_where_the_open_region_starts() {
    // One row, under a line of text.
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(b"You are hungry.\r\n"));
    g.session_output(&text(&marked(7, b"[1020/1020hp]")));
    assert_eq!(
        g.cursor_report(),
        CursorReport {
            line: 1,
            col: 13,
            at_bottom: true,
            cols: 40,
            region: Some(RegionStart {
                gen: 7,
                line: 1,
                col: 0
            }),
        }
    );

    // Two rows, as a line break in the design draws them.
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(b"You are hungry.\r\n"));
    g.session_output(&text(&marked(2, b"Tank 100%\r\n[1020/1020hp]")));
    assert_eq!(region_at(&g), Some((2, 1, 0)));
    assert_eq!(cursor(&g), (2, 13));

    // Word wrapped at a narrow width, the region still starts on its
    // first row.
    let mut g = TermGrid::new(12, 10);
    g.session_output(&text(b"You are hungry.\r\n"));
    g.session_output(&text(&marked(3, b"[1020/1020hp 800/800mn 930/930mv]")));
    assert_eq!(
        screen(&g),
        [
            "You are",
            "hungry.",
            "[1020/1020hp",
            "800/800mn",
            "930/930mv]"
        ]
    );
    assert_eq!(region_at(&g), Some((3, 2, 0)));

    // A region that starts mid row, and one that wrote nothing yet.
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(b"<10hp> "));
    g.session_output(&text(&marked(4, b"more")));
    assert_eq!(region_at(&g), Some((4, 0, 7)));
    g.session_output(&text(&marked(5, b"")));
    assert_eq!(region_at(&g), Some((5, 0, 11)));
    // A mark after a full row starts its region on the next.
    let mut g = TermGrid::new(10, 10);
    g.local_write(b"0123456789");
    g.session_output(&text(&marked(6, b"")));
    assert_eq!(region_at(&g), Some((6, 1, 0)));

    // Lift marks inside a region take no room.
    let mut g = TermGrid::new(12, 10);
    g.session_output(&text(b"room\r\n"));
    g.session_output(&text(&marked(8, &lift(1, b"[1020/1020hp 800/800mn]"))));
    assert_eq!(screen(&g), ["room", "[1020/1020hp", "800/800mn]"]);
    assert_eq!(region_at(&g), Some((8, 1, 0)));
}

#[test]
fn the_cursor_report_has_no_region_once_anything_lands_after_it() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(&marked(1, b"<1020hp> ")));
    assert_eq!(region_at(&g), Some((1, 0, 0)));
    // Your echo.
    g.local_write(b"look\r\n");
    assert_eq!(region_at(&g), None);
    // Game text after the next prompt.
    g.session_output(&text(&marked(2, b"<1020hp> ")));
    g.session_output(&text(b"\r\nYou are hungry.\r\n"));
    assert_eq!(region_at(&g), None);
    // A replace keeps it open at its new start.
    g.session_output(&text(&marked(3, b"<1020hp> ")));
    g.session_output(&replace(3, &marked(4, b"Tank\r\n<1020hp> "), false));
    assert_eq!(region_at(&g), Some((4, 3, 0)));
}

#[test]
fn the_cursor_report_follows_a_resize_and_says_when_you_scrolled_back() {
    // The card keeps the row open through a resize, and the start is
    // counted again at the new width.
    let mut g = TermGrid::new(40, 6);
    g.session_output(&text(b"one\r\ntwo\r\n"));
    g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
    assert_eq!(region_at(&g), Some((1, 2, 0)));
    g.resize(12, 6);
    let report = g.cursor_report();
    assert_eq!(report.cols, 12);
    let start = report.region.expect("still open");
    assert_eq!(report.line - start.line, 2, "three rows at 12 wide");

    // Scrolled back into history, the report says so.
    let mut g = TermGrid::new(20, 3);
    for n in 0..10 {
        g.session_output(&text(format!("line {n}\r\n").as_bytes()));
    }
    g.session_output(&text(&marked(9, b"<1020hp> ")));
    assert!(g.cursor_report().at_bottom);
    g.scroll(4);
    let report = g.cursor_report();
    assert!(!report.at_bottom);
    assert_eq!(report.region.map(|r| r.line), Some(2));
}

#[test]
fn a_region_that_starts_mid_row_keeps_what_came_before_it() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(b"<10hp> "));
    g.session_output(&text(&marked(1, b"You are hun")));
    g.session_output(&replace(1, b"You are hungry.\r\n", true));
    assert_eq!(screen(&g), ["<10hp> You are hungry."]);
}

#[test]
fn a_restore_goes_back_before_anything_else_lands() {
    // Before a local write.
    let mut g = TermGrid::new(40, 10);
    let mut preview = text(&marked(1, b"PREVIEW"));
    preview.restore = Some(b"LIVE".to_vec());
    g.session_output(&preview);
    assert_eq!(screen(&g), ["PREVIEW"]);
    g.local_write(b"look\r\n");
    assert_eq!(screen(&g), ["LIVElook"]);

    // Before session output.
    let mut g = TermGrid::new(40, 10);
    g.session_output(&preview);
    g.session_output(&text(b"\r\nYou flee!\r\n"));
    assert_eq!(screen(&g), ["LIVE", "You flee!"]);

    // A replace of the region itself takes its place, and its own
    // restore rides the region it opens.
    let mut g = TermGrid::new(40, 10);
    g.session_output(&preview);
    let mut again = replace(1, &marked(2, b"OTHER"), false);
    again.restore = Some(b"LIVE2".to_vec());
    g.session_output(&again);
    assert_eq!(screen(&g), ["OTHER"]);
    g.session_output(&replace(2, &marked(3, b"PLAIN"), false));
    g.local_write(b"look\r\n");
    assert_eq!(screen(&g), ["PLAINlook"]);

    // A replace of the region and text after it, in one output: the
    // replace goes first, and the text lands after it.
    let mut g = TermGrid::new(40, 10);
    g.session_output(&preview);
    let mut next = replace(1, &marked(2, b"NEW> "), false);
    next.text(b"\r\nYou flee!\r\n");
    g.session_output(&next);
    assert_eq!(screen(&g), ["NEW>", "You flee!"]);
}

#[test]
fn a_split_character_decodes_whole_across_outputs() {
    let mut g = TermGrid::new(40, 10);
    let word = "caf\u{e9}".as_bytes();
    g.session_output(&text(&word[..4]));
    g.session_output(&text(&word[4..]));
    assert_eq!(screen(&g), ["caf\u{e9}"]);
}

#[test]
fn marks_are_found_whole_and_only_whole() {
    let mark = vosh_prompt::stage::mark(42);
    let bytes = [b"ab".as_slice(), &mark, b"cd"].concat();
    assert_eq!(
        find_mark(&bytes),
        Some((2, 2 + mark.len(), Mark::Region(42)))
    );
    assert_eq!(find_mark(b"\x1b]7717;o;\x07"), None);
    assert_eq!(find_mark(b"\x1b]7717;o;12"), None);
    assert_eq!(find_mark(b"plain"), None);
    let start = vosh_prompt::stage::lift_start(7);
    let end = vosh_prompt::stage::lift_end(7);
    assert_eq!(
        find_mark(&start),
        Some((0, start.len(), Mark::LiftStart(7)))
    );
    assert_eq!(find_mark(&end), Some((0, end.len(), Mark::LiftEnd(7))));
    assert_eq!(find_mark(b"\x1b]7717;x;7\x07"), None);
    assert_eq!(erase_back(0, 0), b"\r\x1b[49m\x1b[0J");
    assert_eq!(erase_back(2, 7), b"\r\x1b[2A\x1b[7C\x1b[49m\x1b[0J");
}

// The wrap itself runs fixtures/wrap/cases.json in crates/prompt and in
// src/terminal/wordWrap.test.ts.
#[test]
fn session_feed_word_wraps_at_the_grid_width() {
    let _shared = lock_shared_grid_for_test();
    blank_shared_grid_for_test(10, 24);
    feed_session_output(ONE, &text(b"the quick brown fox\r\n"), None);
    let rows = with_grid_mut(ONE, |g| [g.row_string(0), g.row_string(1)]).unwrap();
    assert!(rows[0].starts_with("the quick"));
    assert!(rows[1].starts_with("brown fox"));
}

#[test]
fn a_selection_or_a_read_back_keeps_the_reader_busy() {
    let _shared = lock_shared_grid_for_test();
    blank_shared_grid_for_test(20, 4);
    feed_session_output(ONE, &text(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7"), None);
    assert!(!reader_busy(ONE));
    // A click with no drag selects nothing.
    start_selection(ONE, 1, 0);
    assert!(!reader_busy(ONE));
    update_selection(ONE, 2, 1);
    assert!(reader_busy(ONE));
    clear_selection(ONE);
    assert!(!reader_busy(ONE));
    scroll(ONE, 2);
    assert!(reader_busy(ONE));
    scroll(ONE, -2);
    assert!(!reader_busy(ONE));
}

#[test]
fn collect_matches_columns_stay_aligned_after_wide_chars() {
    let mut g = TermGrid::new(80, 24);
    // 日 and 本 are double-width: the glyph occupies its cell and the
    // next holds a spacer that reads as a space. Line text collects one
    // char per column, so char offsets stay 1:1 with grid columns.
    g.feed("ab\u{65e5}\u{672c} cat".as_bytes());
    let m = collect_matches(&g, "cat", false, false, false);
    assert_eq!(m.len(), 1);
    // Columns: a=0 b=1 日=2 (spacer 3) 本=4 (spacer 5) space=6 c=7.
    assert_eq!(m[0], (0, 7, 10));
}

#[test]
fn url_columns_stay_aligned_after_wide_chars() {
    // Same one-char-per-column construction url_at feeds url_in_line:
    // 日=0 spacer=1 本=2 spacer=3, url starts at column 4.
    let text = "\u{65e5} \u{672c} http://x.dev";
    let hit = url_in_line(text, 6).expect("url under cursor");
    assert_eq!(hit, ("http://x.dev".to_string(), 4, 16));
    assert!(url_in_line(text, 3).is_none());
}

#[test]
fn collect_matches_honors_regex_and_case() {
    let mut g = TermGrid::new(80, 24);
    g.feed(b"HP: 100  hp: 50");
    // Regex, case-insensitive: both HP and hp match.
    assert_eq!(collect_matches(&g, r"hp: \d+", true, false, false).len(), 2);
    // Case-sensitive: only the lowercase one.
    assert_eq!(collect_matches(&g, r"hp: \d+", true, true, false).len(), 1);
}

#[test]
fn collect_matches_whole_word_excludes_substrings() {
    let mut g = TermGrid::new(80, 24);
    g.feed(b"cat category");
    // Without whole-word, "cat" matches inside "category" too.
    assert_eq!(collect_matches(&g, "cat", false, false, false).len(), 2);
    // With whole-word, only the standalone "cat".
    assert_eq!(collect_matches(&g, "cat", false, false, true).len(), 1);
}

/// The stage's output driven into the grid, as the session and the
/// native renderer pass it along.
mod stage_into_grid {
    use super::*;
    use vosh_prompt::config::RegexCapture;
    use vosh_prompt::stage::{End, Stage};
    use vosh_prompt::CaptureConfig;

    const CAPTURE: &str = r"\[(?<hp>\d+)/(?<maxhp>\d+)hp\]";
    const GAME: &str = "[1020/1020hp]";
    /// A design of one row at 40 wide and three rows at 12 wide.
    const ONE_ROW: &str = "[1020/1020hp 800/800mn 930/930mv]";
    /// A design of two rows, as `%nl` draws it.
    const TWO_ROWS: &str = "Tank 100%\r\n[1020/1020hp]";

    fn stage() -> Stage {
        let mut stage = Stage::default();
        stage.set_capture(&CaptureConfig::Regex(RegexCapture {
            lines: vec![CAPTURE.to_string()],
            ..RegexCapture::default()
        }));
        stage
    }

    /// A read that brings `before`, then the game's prompt, which the
    /// stage draws as `drawn`.
    fn prompt_read(stage: &mut Stage, before: &[u8], drawn: &str) -> Output {
        let mut out = Output::new(false);
        out.text(before);
        let block = stage
            .recognize(GAME.as_bytes(), GAME, End::Line)
            .expect("the capture reads the game's prompt");
        stage.draw(&mut out, block, None, b"", drawn);
        stage.finish(&mut out);
        out
    }

    /// A repaint of the open row as `drawn`, or as the game sent it.
    fn repaint(stage: &mut Stage, drawn: Option<&str>) -> Output {
        let mut out = Output::new(false);
        stage.repaint(&mut out, drawn);
        out
    }

    /// How many rows of `rows` hold `text`.
    fn count(rows: &[String], text: &str) -> usize {
        rows.iter().filter(|row| row.contains(text)).count()
    }

    #[test]
    fn a_repaint_after_your_echo_is_dropped_at_either_width() {
        for (columns, design, expect) in [
            (
                40,
                ONE_ROW,
                vec!["You are hungry.", "[1020/1020hp 800/800mn 930/930mv]look"],
            ),
            (
                12,
                ONE_ROW,
                vec![
                    "You are",
                    "hungry.",
                    "[1020/1020hp",
                    "800/800mn",
                    "930/930mv]lo",
                    "ok",
                ],
            ),
            (
                40,
                TWO_ROWS,
                vec!["You are hungry.", "Tank 100%", "[1020/1020hp]look"],
            ),
            (
                12,
                TWO_ROWS,
                vec!["You are", "hungry.", "Tank 100%", "[1020/1020hp", "]look"],
            ),
        ] {
            let mut stage = stage();
            let mut g = TermGrid::new(columns, 12);
            g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", design));
            // Your echo lands before the session hears of it, so the
            // stage still holds the row open and repaints it.
            g.local_write(b"look\r\n");
            let out = repaint(&mut stage, Some("NEW"));
            assert!(out.replace.is_some());
            g.session_output(&out);
            let rows = screen(&g);
            assert_eq!(rows, expect, "{columns} wide");
            // The echo shows once, even where it wraps, and the
            // prompt shows once.
            assert_eq!(rows.concat().matches("look").count(), 1);
            assert_eq!(rows.concat().matches("1020hp").count(), 1);
            assert_eq!(count(&rows, "NEW"), 0);
        }
    }

    #[test]
    fn your_echo_after_a_repaint_follows_the_new_prompt_at_either_width() {
        for (columns, design) in [(40, ONE_ROW), (12, ONE_ROW), (40, TWO_ROWS), (12, TWO_ROWS)] {
            let mut stage = stage();
            let mut g = TermGrid::new(columns, 12);
            g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", design));
            g.session_output(&repaint(&mut stage, Some("NEW> ")));
            g.local_write(b"look\r\n");
            let rows = screen(&g);
            let hungry = if columns == 40 {
                vec!["You are hungry."]
            } else {
                vec!["You are", "hungry."]
            };
            assert_eq!(
                rows,
                [hungry, vec!["NEW> look"]].concat(),
                "{columns} wide, {design:?}"
            );
            assert_eq!(count(&rows, "1020"), 0, "the old design is gone whole");
        }
    }

    #[test]
    fn drawing_off_shows_the_game_prompt_where_the_design_was() {
        let mut stage = stage();
        let mut g = TermGrid::new(40, 12);
        g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", TWO_ROWS));
        g.session_output(&repaint(&mut stage, None));
        g.local_write(b"look\r\n");
        assert_eq!(screen(&g), ["You are hungry.", GAME, "look"]);
    }

    #[test]
    fn a_prompt_whole_before_its_line_end_repaints_with_its_line_end() {
        // A capture that settles, on a prompt the game followed with
        // a line end in the same read.
        let mut stage = Stage::default();
        stage.set_capture(&CaptureConfig::Regex(RegexCapture {
            lines: vec![r"^\[(?<hp>\d+)/(?<maxhp>\d+)hp\]$".to_string()],
            settle: true,
            ..RegexCapture::default()
        }));
        for columns in [40, 12] {
            let mut g = TermGrid::new(columns, 12);
            g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", ONE_ROW));
            // Drawing off shows the game's prompt where the design
            // was, and your echo lands on the row after it.
            g.session_output(&repaint(&mut stage, None));
            g.local_write(b"look\r\n");
            let mut expect = TermGrid::new(columns, 12);
            expect.session_output(&text(b"You are hungry.\r\n[1020/1020hp]\r\nlook\r\n"));
            assert_eq!(screen(&g), screen(&expect), "{columns} wide");
        }
    }

    #[test]
    fn a_prompt_split_across_reads_replaces_its_painted_start() {
        let mut stage = stage();
        let mut g = TermGrid::new(12, 12);
        let mut first = Output::new(false);
        first.text(b"You are hungry.\r\n");
        let painted = stage.paint_partial(&mut first, b"[1020/10", None);
        stage.finish(&mut first);
        g.session_output(&first);
        assert_eq!(screen(&g), ["You are", "hungry.", "[1020/10"]);
        let mut second = Output::new(false);
        let block = stage
            .recognize(GAME.as_bytes(), GAME, End::Line)
            .expect("the prompt");
        stage.draw(
            &mut second,
            block,
            painted.map(|(gen, _)| gen),
            b"",
            ONE_ROW,
        );
        g.session_output(&second);
        assert_eq!(
            screen(&g),
            [
                "You are",
                "hungry.",
                "[1020/1020hp",
                "800/800mn",
                "930/930mv]"
            ]
        );
        // The drawn prompt is the open row now.
        g.session_output(&repaint(&mut stage, Some("NEW")));
        assert_eq!(screen(&g), ["You are", "hungry.", "NEW"]);
    }

    #[test]
    fn a_repaint_after_a_resize_while_the_row_stays_open() {
        // With the card open the session keeps the row open through a
        // resize, and the grid finds the region at its new width.
        let mut stage = stage();
        let mut g = TermGrid::new(40, 4);
        g.session_output(&prompt_read(
            &mut stage,
            b"one\r\ntwo\r\nYou are hungry.\r\n",
            ONE_ROW,
        ));
        g.resize(12, 4);
        g.session_output(&repaint(&mut stage, Some("NEW")));
        // The rows over the prompt reflow at 12 wide, and the narrower
        // grid keeps the cursor row, so the first ones move into
        // history. The row just over the prompt stays whole.
        assert_eq!(screen(&g), ["ry.", "NEW"]);
    }

    #[test]
    fn a_preview_goes_back_to_the_live_render_before_your_echo() {
        let mut stage = stage();
        let mut g = TermGrid::new(40, 12);
        g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", "LIVE> "));
        let mut preview = repaint(&mut stage, Some("PREVIEW> "));
        preview.restore = Some(b"LIVE> ".to_vec());
        g.session_output(&preview);
        assert_eq!(screen(&g), ["You are hungry.", "PREVIEW>"]);
        g.local_write(b"look\r\n");
        assert_eq!(screen(&g), ["You are hungry.", "LIVE> look"]);
    }

    #[test]
    fn a_repaint_that_crosses_later_output_is_dropped() {
        let mut stage = stage();
        let mut g = TermGrid::new(40, 12);
        g.session_output(&prompt_read(&mut stage, b"", "DRAWN> "));
        // A repaint the session sent before the next read reached the
        // renderer, which then finds output after the region.
        let stale = repaint(&mut stage.clone(), Some("NEW> "));
        let mut next = Output::new(false);
        next.text(b"\r\nYou flee!\r\n");
        g.session_output(&next);
        g.session_output(&stale);
        assert_eq!(screen(&g), ["DRAWN>", "You flee!"]);
    }
}

/// Your echo with the grey mark Mark your commands draws, as the page
/// writes it after you type `command`.
fn send_typed(g: &mut TermGrid, command: &str) {
    g.local_write(format!("{}{command}\r\n", crate::input::ECHO_CARET).as_bytes());
}

/// A quick key's echo of `command`, as the session sends it.
fn send_quick_key(g: &mut TermGrid, command: &str) {
    let echo = crate::input::command_echo(command, &crate::profile::ui::UiConfig::default());
    g.session_output(&text(format!("{echo}\r\n").as_bytes()));
}

/// One way your echo of a command reaches the grid.
type EchoSend = fn(&mut TermGrid, &str);

/// Each way your echo reaches the grid.
fn sends() -> [(&'static str, EchoSend); 2] {
    [("typed", send_typed), ("quick key", send_quick_key)]
}

/// The game's welcome, from tables.c, which ends on another character
/// with a `>` inside.
const MOTD: &str = "Prepare yourself. For you are about to <Enter> the Forsaken Lands!";

#[test]
fn your_echo_drops_its_mark_after_a_login_prompt() {
    for (how, send) in sends() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(b"\n\rAccount name> "));
        send(&mut g, "Tolliver");
        g.session_output(&text(b"\n\rYour choice> "));
        send(&mut g, "1");
        assert_eq!(
            screen(&g),
            ["", "Account name> Tolliver", "", "Your choice> 1"],
            "{how}"
        );
    }
}

#[test]
fn your_echo_drops_its_mark_after_your_prompt_in_the_text() {
    for (how, send) in sends() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(
            &[
                b"You are hungry.\r\n".as_slice(),
                &marked(1, b"<1020hp 800m 930mv> "),
            ]
            .concat(),
        ));
        send(&mut g, "look");
        assert_eq!(
            screen(&g),
            ["You are hungry.", "<1020hp 800m 930mv> look"],
            "{how}"
        );
    }
}

#[test]
fn your_echo_drops_its_mark_after_a_lifted_prompt() {
    // The space after the band keeps your echo a cell away, on the
    // prompt's row.
    for (how, send) in sends() {
        let mut g = TermGrid::new(40, 10);
        let mut prompt = lift(3, &marked(4, b"<1020hp 800m 930mv>"));
        prompt.push(b' ');
        g.session_output(&text(&prompt));
        send(&mut g, "look");
        assert_eq!(screen(&g), ["<1020hp 800m 930mv> look"], "{how}");
    }
}

#[test]
fn your_echo_keeps_its_mark_on_an_empty_row_and_after_another_character() {
    for (how, send) in sends() {
        let mut g = TermGrid::new(80, 10);
        g.session_output(&text(b"You are hungry.\r\n"));
        send(&mut g, "look");
        g.session_output(&text(MOTD.as_bytes()));
        send(&mut g, "look");
        assert_eq!(
            screen(&g),
            [
                "You are hungry.".to_string(),
                "\u{203a} look".to_string(),
                format!("{MOTD}\u{203a} look"),
            ],
            "{how}"
        );
        // A prompt that fills its row sends your echo to the next one.
        let mut g = TermGrid::new(19, 10);
        g.session_output(&text(b"<1020hp 800m 930mv>"));
        send(&mut g, "look");
        assert_eq!(
            screen(&g),
            ["<1020hp 800m 930mv>", "\u{203a} look"],
            "{how}"
        );
    }
}

#[test]
fn your_echo_keeps_its_mark_on_the_row_held_line_ends_start() {
    for (how, send) in sends() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&held(b"<1020hp 800m 930mv> ", b"\r\n"));
        send(&mut g, "look");
        assert_eq!(
            screen(&g),
            ["<1020hp 800m 930mv>", "\u{203a} look"],
            "{how}"
        );
    }
}

#[test]
fn your_echo_keeps_its_mark_on_the_row_a_pinned_prompt_left() {
    for (how, send) in sends() {
        let mut g = TermGrid::new(40, 10);
        let mut out = held(b"You are hungry.", b"\r\n\r\n");
        out.pin_row = Some(true);
        g.session_output(&out);
        send(&mut g, "look");
        assert_eq!(
            screen(&g),
            ["You are hungry.", "", "\u{203a} look"],
            "{how}"
        );
    }
}

#[test]
fn your_echo_reads_its_row_once_the_live_render_is_back() {
    const DRAWN: &[u8] = b"[1020/1020hp 800/800mn 930/930mv] ";
    const GAME: &[u8] = b"<1020hp 800m 930mv> ";
    for (how, send) in sends() {
        // The card previews your design over the game's own line.
        let mut g = TermGrid::new(40, 10);
        let mut preview = text(&marked(1, DRAWN));
        preview.restore = Some(marked(1, GAME));
        g.session_output(&preview);
        send(&mut g, "look");
        assert_eq!(screen(&g), ["<1020hp 800m 930mv> look"], "{how}");
        // The card previews the game's line over your design.
        let mut g = TermGrid::new(40, 10);
        let mut preview = text(&marked(1, GAME));
        preview.restore = Some(marked(1, DRAWN));
        g.session_output(&preview);
        send(&mut g, "look");
        assert_eq!(
            screen(&g),
            ["[1020/1020hp 800/800mn 930/930mv] \u{203a} look"],
            "{how}"
        );
    }
}

#[test]
fn scrollback_size_sets_the_history_a_grid_keeps() {
    let mut g = TermGrid::new(80, 24);
    g.set_history(1_000);
    for i in 0..3_000 {
        g.feed(format!("line {i}\r\n").as_bytes());
    }
    assert_eq!(g.scrollback_len(), 1_000);
    g.set_history(20_000);
    for i in 0..15_000 {
        g.feed(format!("more {i}\r\n").as_bytes());
    }
    assert_eq!(g.scrollback_len(), 16_000);
    // A grid made after the session set its size keeps it from the start.
    let mut held = SessionGrid {
        history: Some(1_000),
        ..SessionGrid::default()
    };
    let grid = held.written();
    for i in 0..3_000 {
        grid.feed(format!("line {i}\r\n").as_bytes());
    }
    assert_eq!(grid.scrollback_len(), 1_000);
}

#[test]
fn a_page_is_the_history_rows_the_split_shows_less_one() {
    // 0.66 of 40 rows shows 26 whole history rows.
    assert_eq!(page_lines(40, 0.66), 25);
    // The divider clamps to the drag limits, so a page does too.
    assert_eq!(page_lines(40, 0.15), 5);
    assert_eq!(page_lines(40, 0.85), 33);
    // Past the edges the split still keeps one row on each side.
    assert_eq!(page_lines(6, 0.0), 1);
    assert_eq!(page_lines(6, 1.0), 4);
    // Too short to split, a page is the rows less one, and never 0.
    assert_eq!(page_lines(5, 0.66), 4);
    assert_eq!(page_lines(2, 0.66), 1);
    assert_eq!(page_lines(1, 0.66), 1);
    assert_eq!(page_lines(0, 0.66), 1);
}

#[test]
fn page_up_opens_the_split_by_one_page_and_page_down_comes_back() {
    let _shared = lock_shared_grid_for_test();
    blank_shared_grid_for_test(40, 40);
    for n in 0..200 {
        feed_local(ONE, format!("{n}\r\n").as_bytes());
    }
    assert_eq!(current_display_offset(ONE), 0);
    scroll_page(ONE, true, 0.66);
    assert_eq!(current_display_offset(ONE), 25);
    scroll_page(ONE, true, 0.66);
    assert_eq!(current_display_offset(ONE), 50);
    scroll_page(ONE, false, 0.66);
    scroll_page(ONE, false, 0.66);
    assert_eq!(current_display_offset(ONE), 0);
}

/// A line Vosh prints about itself, which starts a row of its own.
fn own_line(line: &str) -> Output {
    let mut out = text(format!("{line}\r\n").as_bytes());
    out.fresh = true;
    out
}

#[test]
fn a_line_vosh_prints_starts_a_row_after_a_prompt_that_came_after_your_echo() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(b"room\r\n"));
    g.local_write(b"#walk stop\r\n");
    g.session_output(&text(b"<1020hp 800m> "));
    g.session_output(&own_line("[walk] You are not walking."));
    assert_eq!(
        screen(&g),
        [
            "room",
            "#walk stop",
            "<1020hp 800m>",
            "[walk] You are not walking."
        ]
    );
}

#[test]
fn a_line_vosh_prints_adds_no_blank_row_at_a_row_start_or_after_held_line_ends() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(b"<1020hp 800m> "));
    g.local_write(b"#walk stop\r\n");
    g.session_output(&own_line("[walk] You are not walking."));
    g.session_output(&held(b"room", b"\r\n"));
    g.session_output(&own_line("[lua] boom"));
    assert_eq!(
        screen(&g),
        [
            "<1020hp 800m> #walk stop",
            "[walk] You are not walking.",
            "room",
            "[lua] boom"
        ]
    );
}

#[test]
fn the_echo_of_a_command_vosh_draws_itself_stays_after_the_prompt() {
    let mut g = TermGrid::new(40, 10);
    g.session_output(&text(b"<1020hp 800m> "));
    g.session_output(&text(b"kick goblin\r\n"));
    assert_eq!(screen(&g), ["<1020hp 800m> kick goblin"]);
}
