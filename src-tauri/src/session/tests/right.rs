//! A design that pushes part of a row to the right edge (`%{right}`),
//! played through the session's own steps. It reaches to the width the
//! session knows, in the text, lifted and pinned, and draws again when
//! the width changes, on the open row or on the band. The native grid
//! keeps a lifted band on its row and your echo on the next.

use super::*;
use vosh_prompt::PromptShow;

/// Your health on the left and your mana pushed to the right edge.
const RIGHT: &str = "<%hp>%{right}%mana!";

/// `<1020>` and `800!` with the row's width between them.
fn row(cols: usize) -> String {
    format!("<1020>{}800!", " ".repeat(cols - 10))
}

/// A session that shows `show` and draws [`RIGHT`] at `cols` wide, as a
/// connection starts it, with the negotiator holding the same size.
fn at(show: PromptShow, cols: u16) -> (Session, Negotiator) {
    let mut session = Session::new(showing(profile(CODES, RIGHT, true), show));
    let mut negotiator = Negotiator::new();
    negotiator.set_window_size(cols, 40);
    session.c.prompt.set_cols(usize::from(cols));
    (session, negotiator)
}

/// What the band shows, as text.
fn band(out: &Output) -> Option<String> {
    out.pin.as_deref().map(vosh_protocol::ansi::plain_text)
}

#[test]
fn a_push_reaches_the_last_column_the_session_knows() {
    for show in [PromptShow::Text, PromptShow::Lifted] {
        let (mut session, _) = at(show, 40);
        let mut grid = crate::native::grid::TermGrid::new(40, 20);
        grid.session_output(&session.read(&wire_fixture("quiet")).out);
        let rows = rows_of(&grid);
        assert_eq!(rows.last(), Some(&row(40)), "{show:?}: {rows:?}");
    }
    let (mut session, _) = at(PromptShow::Pinned, 40);
    let read = session.read(&wire_fixture("quiet"));
    assert_eq!(band(&read.out), Some(row(40)));
    // The push's span takes the spaces, and mana sits at the edge.
    let spans: Vec<(usize, usize, usize)> = read
        .out
        .pin_spans
        .iter()
        .flatten()
        .map(|s| (s.piece, s.col, s.width))
        .collect();
    assert_eq!(
        spans,
        [
            (0, 0, 1),
            (1, 1, 4),
            (2, 5, 1),
            (3, 6, 30),
            (4, 36, 3),
            (5, 39, 1)
        ]
    );
}

#[test]
fn a_new_width_draws_the_push_again_where_your_prompt_shows() {
    for show in [PromptShow::Text, PromptShow::Lifted] {
        let (mut session, mut negotiator) = at(show, 40);
        let mut grid = crate::native::grid::TermGrid::new(40, 20);
        grid.session_output(&session.read(&wire_fixture("quiet")).out);
        // The size the session holds draws nothing again.
        assert!(!window_size_step(
            &mut session.c,
            &mut negotiator,
            40,
            20,
            false
        ));
        // A new width keeps the row open, and the repaint reaches to it.
        assert!(window_size_step(
            &mut session.c,
            &mut negotiator,
            30,
            20,
            false
        ));
        assert!(session.c.prompt.stage.open_row().is_some(), "{show:?}");
        let out = repaint_step(&session.p, &mut session.c, false, Instant::now());
        assert!(out.replace.is_some(), "{show:?}");
        grid.resize(30, 20);
        grid.session_output(&out);
        let rows = rows_of(&grid);
        assert_eq!(rows.last(), Some(&row(30)), "{show:?}: {rows:?}");
        assert_eq!(
            rows.iter().filter(|r| r.contains("<1020>")).count(),
            1,
            "{show:?}: the row the old width drew is gone: {rows:?}"
        );
        // Wider again, and it follows.
        assert!(window_size_step(
            &mut session.c,
            &mut negotiator,
            50,
            20,
            false
        ));
        let out = repaint_step(&session.p, &mut session.c, false, Instant::now());
        grid.resize(50, 20);
        grid.session_output(&out);
        assert_eq!(rows_of(&grid).last(), Some(&row(50)), "{show:?}");
    }
    // Pinned, the band draws again at the new width.
    let (mut session, mut negotiator) = at(PromptShow::Pinned, 40);
    let _ = session.read(&wire_fixture("quiet"));
    assert!(window_size_step(
        &mut session.c,
        &mut negotiator,
        30,
        20,
        false
    ));
    let out = repaint_step(&session.p, &mut session.c, false, Instant::now());
    assert_eq!(band(&out), Some(row(30)));
}

#[test]
fn a_design_with_no_push_still_closes_the_row_at_a_new_width() {
    let mut session = Session::new(showing(profile(CODES, "<%hp>", true), PromptShow::Text));
    let mut negotiator = Negotiator::new();
    negotiator.set_window_size(40, 20);
    let _ = session.read(&wire_fixture("quiet"));
    assert!(!window_size_step(
        &mut session.c,
        &mut negotiator,
        30,
        20,
        false
    ));
    assert!(session.c.prompt.stage.open_row().is_none());
    // Nor does a push draw again while Vosh draws the game's own prompt.
    let mut session = Session::new(showing(profile(CODES, RIGHT, false), PromptShow::Text));
    let _ = session.read(&wire_fixture("quiet"));
    assert!(!window_size_step(
        &mut session.c,
        &mut negotiator,
        40,
        20,
        false
    ));
}

#[test]
fn a_lifted_band_that_fills_its_row_stays_on_it_and_your_echo_takes_the_next() {
    let (mut session, _) = at(PromptShow::Lifted, 40);
    let mut grid = crate::native::grid::TermGrid::new(40, 20);
    grid.session_output(&session.read(&wire_fixture("quiet")).out);
    session.local_write();
    grid.local_write(b"look\r\n");
    let rows = rows_of(&grid);
    let at = rows
        .iter()
        .position(|r| *r == row(40))
        .unwrap_or_else(|| panic!("no prompt row in {rows:?}"));
    assert_eq!(rows[at + 1], "look", "{rows:?}");
    // The band covers the whole row and nothing on the next.
    let line = i32::try_from(at).expect("a row");
    let spans: Vec<(i32, usize, usize)> = grid
        .lift_spans(0, 20)
        .into_iter()
        .map(|s| (s.line, s.first, s.end))
        .collect();
    assert_eq!(spans.last(), Some(&(line, 0, 40)), "{spans:?}");
    assert!(spans.iter().all(|s| s.0 <= line), "{spans:?}");
}
