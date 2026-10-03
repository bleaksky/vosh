//! The clock repaint, played through the session's own steps. While your
//! design draws the tick, the time or the date, your idle prompt repaints
//! as what the piece shows changes, at most once a second: the open row in
//! the text and lifted while it is the last thing on screen and you neither
//! select nor read back, and the band while pinned, which no selection or
//! read back holds.

use super::show_tests::{profile, showing, wire_fixture, Session, CODES};
use super::*;
use vosh_prompt::values::overrides::{Preview, PromptPreview};
use vosh_prompt::PromptShow;

/// Your health and the seconds left of the tick.
const TICK: &str = "<%hp> %tick";

fn plain(bytes: &[u8]) -> String {
    vosh_protocol::ansi::plain_text(bytes)
}

/// What `f` gives, and how many times the session drew your design while
/// it ran.
fn drawing<T>(f: impl FnOnce() -> T) -> (T, u64) {
    let before = RENDERS.with(std::cell::Cell::get);
    let out = f();
    (out, RENDERS.with(std::cell::Cell::get) - before)
}

/// A session showing `show` that drew `template` over the quiet pulse,
/// with the tick restarted at `t0`, the moment it read the prompt.
fn at_prompt(template: &str, show: PromptShow) -> (Session, Instant) {
    let mut session = Session::new(showing(profile(CODES, template, true), show));
    let t0 = Instant::now();
    session.p.tick.start_session(t0);
    let read = session.read(&wire_fixture("quiet"));
    assert!(read.prompt, "{show:?}");
    (session, t0)
}

/// What a clock repaint writes where your prompt shows: the replace's
/// text in the text and lifted, the band's while pinned. The space a lift
/// leaves after the band goes, so every place reads alike.
fn shown(out: &Output, show: PromptShow) -> Option<String> {
    let bytes = match show {
        PromptShow::Pinned => out.pin.as_deref(),
        _ => out.replace.as_ref().map(|r| r.bytes.as_slice()),
    };
    bytes.map(|b| plain(b).trim_end().to_string())
}

#[test]
fn the_tick_repaints_your_idle_prompt_each_time_it_shows_another_second() {
    for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
        let (mut session, t0) = at_prompt(TICK, show);
        let later = t0 + Duration::from_millis(1_500);
        let out = clock_step(&mut session.p, false, false, later);
        assert_eq!(shown(&out, show).as_deref(), Some("<1020> 29"), "{show:?}");
        // Nothing follows it in the text, so it never lands on your echo
        // and never reaches history.
        assert!(out.bytes.is_empty(), "{show:?}");
        assert!(out.restore.is_none(), "{show:?}");
        // The same second again writes nothing.
        let again = t0 + Duration::from_millis(1_900);
        assert!(
            clock_step(&mut session.p, false, false, again).is_empty(),
            "{show:?}"
        );
        let next = t0 + Duration::from_millis(2_100);
        let out = clock_step(&mut session.p, false, false, next);
        assert_eq!(shown(&out, show).as_deref(), Some("<1020> 28"), "{show:?}");
    }
}

#[test]
fn the_row_waits_while_you_select_or_read_back_and_the_band_does_not() {
    for show in [PromptShow::Text, PromptShow::Lifted] {
        let (mut session, t0) = at_prompt(TICK, show);
        let later = t0 + Duration::from_millis(1_500);
        let (out, drawn) = drawing(|| clock_step(&mut session.p, false, true, later));
        assert!(out.is_empty(), "{show:?}");
        assert_eq!(drawn, 0, "{show:?}: waiting draws nothing");
        // Once you let go, the row catches up at the next repaint.
        let out = clock_step(&mut session.p, false, false, later);
        assert_eq!(shown(&out, show).as_deref(), Some("<1020> 29"), "{show:?}");
    }
    // The band is not in the text, so it keeps counting.
    let (mut session, t0) = at_prompt(TICK, PromptShow::Pinned);
    let later = t0 + Duration::from_millis(1_500);
    let out = clock_step(&mut session.p, false, true, later);
    assert_eq!(
        shown(&out, PromptShow::Pinned).as_deref(),
        Some("<1020> 29")
    );
}

#[test]
fn a_design_with_no_clock_piece_never_waits_or_draws() {
    for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
        let (mut session, t0) = at_prompt("<%hp>", show);
        assert_eq!(clock_after(&session.p, t0), None, "{show:?}");
        let later = t0 + Duration::from_millis(1_500);
        let (out, drawn) = drawing(|| clock_step(&mut session.p, false, false, later));
        assert!(out.is_empty(), "{show:?}");
        assert_eq!(drawn, 0, "{show:?}");
    }
    // Nor with drawing off, which shows the game's own prompt.
    let mut session = Session::new(profile(CODES, TICK, false));
    let t0 = Instant::now();
    let _ = session.read(&wire_fixture("quiet"));
    assert_eq!(clock_after(&session.p, t0), None);
}

#[test]
fn the_next_repaint_waits_for_the_ticks_next_second() {
    let (session, t0) = at_prompt(TICK, PromptShow::Text);
    // 29.7 seconds left shows 30 until 29 seconds are left.
    let now = t0 + Duration::from_millis(300);
    assert_eq!(
        clock_after(&session.p, now),
        Some(t0 + Duration::from_secs(1) + CLOCK_SLACK)
    );
    // On the second, the next change is a second away.
    let now = t0 + Duration::from_secs(2);
    assert_eq!(
        clock_after(&session.p, now),
        Some(now + Duration::from_secs(1) + CLOCK_SLACK)
    );
    // The time alone waits at most a second for the clock's next second.
    let (session, t0) = at_prompt("<%hp> %{time:hms}", PromptShow::Text);
    let next = clock_after(&session.p, t0).expect("a repaint");
    assert!(next > t0 && next <= t0 + Duration::from_secs(1) + CLOCK_SLACK);
}

#[test]
fn the_clock_waits_while_the_card_shows_a_preview() {
    let (mut session, t0) = at_prompt(TICK, PromptShow::Text);
    session.p.prompt.set_preview(Some(PromptPreview {
        preview: Some(Preview::LowHealth),
        ..PromptPreview::default()
    }));
    let _ = session.repaint();
    let later = t0 + Duration::from_millis(1_500);
    assert!(clock_step(&mut session.p, false, false, later).is_empty());
    // The live render comes back with the card closed, and counts again.
    session.p.prompt.set_preview(None);
    let _ = session.repaint();
    let later = t0 + Duration::from_millis(2_500);
    let out = clock_step(&mut session.p, false, false, later);
    assert_eq!(shown(&out, PromptShow::Text).as_deref(), Some("<1020> 28"));
}

#[test]
fn text_after_your_prompt_ends_the_repaints_of_its_row() {
    // Your sent line follows the prompt.
    let (mut session, t0) = at_prompt(TICK, PromptShow::Text);
    let _ = session.send("look");
    let later = t0 + Duration::from_millis(1_500);
    assert!(clock_step(&mut session.p, false, false, later).is_empty());
    // An echo the webview wrote itself.
    let (mut session, t0) = at_prompt(TICK, PromptShow::Text);
    session.local_write();
    let later = t0 + Duration::from_millis(1_500);
    assert!(clock_step(&mut session.p, false, false, later).is_empty());
    // A slash command's output from elsewhere.
    let (mut session, t0) = at_prompt(TICK, PromptShow::Text);
    let later = t0 + Duration::from_millis(1_500);
    assert!(clock_step(&mut session.p, true, false, later).is_empty());
    // Pinned, the band keeps counting after your line.
    let (mut session, t0) = at_prompt(TICK, PromptShow::Pinned);
    let _ = session.send("look");
    let later = t0 + Duration::from_millis(1_500);
    let out = clock_step(&mut session.p, false, false, later);
    assert_eq!(
        shown(&out, PromptShow::Pinned).as_deref(),
        Some("<1020> 29")
    );
}

#[test]
fn a_band_repaint_between_your_echo_and_its_word_keeps_the_next_line_end() {
    // You type look while pinned. The webview writes your echo, which
    // closes the row the prompt left, and tells the session in a call of
    // its own. A band repaint that goes out in between leaves the row as
    // the renderer has it, so the blank line the game sends next shows.
    let mut session = Session::new(showing(profile(CODES, TICK, true), PromptShow::Pinned));
    let t0 = Instant::now();
    session.p.tick.start_session(t0);
    let mut grid = crate::term_grid::TermGrid::new(60, 30);
    let read = session.read(&wire_fixture("quiet"));
    assert!(read.prompt);
    grid.session_output(&read.out);
    let _ = session.send("look");
    grid.local_write(b"look\r\n");
    let band = clock_step(
        &mut session.p,
        false,
        false,
        t0 + Duration::from_millis(1_500),
    );
    assert_eq!(
        shown(&band, PromptShow::Pinned).as_deref(),
        Some("<1020> 29")
    );
    assert_eq!(band.pin_row, None, "a band repaint leaves the row alone");
    // Nothing in it reaches the text, so xterm writes nothing and leaves
    // a live pane you scrolled back where it is.
    let payload = OutputPayload::from_output(&band);
    assert!(payload.b64.is_empty() && payload.replace.is_none());
    assert!(payload.restore.is_none() && payload.hold.is_none() && payload.pin_row.is_none());
    grid.session_output(&band);
    session.local_write();
    grid.session_output(&session.read(b"\r\nSomeone arrives from the south.\r\n").out);
    let rows = super::show_tests::rows_of(&grid);
    assert_eq!(
        rows[rows.len() - 3..],
        ["look", "", "Someone arrives from the south."]
    );
}

#[test]
fn the_seconds_since_the_tick_count_up_past_a_late_tick() {
    for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
        let (mut session, t0) = at_prompt("<%hp> %{tick:since}", show);
        let turned = session.p.tick.last_tick.expect("the tick runs");
        let later = t0 + Duration::from_millis(1_500);
        let out = clock_step(&mut session.p, false, false, later);
        assert_eq!(shown(&out, show).as_deref(), Some("<1020> 1s"), "{show:?}");
        // The tick is late, so no second is left of it, and the count
        // goes on as the old TinTin prompt counted.
        let late = turned + Duration::from_millis(31_500);
        assert_eq!(session.p.tick.remaining(late), Some(Duration::ZERO));
        let out = clock_step(&mut session.p, false, false, late);
        assert_eq!(shown(&out, show).as_deref(), Some("<1020> 31s"), "{show:?}");
        // The next repaint lands on its next second.
        assert_eq!(
            clock_after(&session.p, late),
            Some(turned + Duration::from_secs(32) + CLOCK_SLACK),
            "{show:?}"
        );
    }
}
