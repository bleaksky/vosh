//! The late GMCP repaint (section 4, and addendum item 7), played through
//! the session's own steps. A packet that changes a value your design
//! reads, with no text after it, repaints your prompt 60 ms later: the
//! open row in the text and lifted, and the band while pinned, which no
//! text cancels, since it is not in the text.

use super::show_tests::{profile, showing, wire_fixture, Read, Session, CODES};
use super::*;
use vosh_prompt::overrides::{Preview, PromptPreview};
use vosh_prompt::PromptShow;

/// Your health and the game hour, which World.Time sends with no text.
const HOUR: &str = "<%hp> %hour";

/// A GMCP packet as the game sends it on the wire.
fn gmcp(package: &str, json: &str) -> Vec<u8> {
    let mut bytes = vec![255, 250, 201];
    bytes.extend_from_slice(package.as_bytes());
    bytes.push(b' ');
    bytes.extend_from_slice(json.as_bytes());
    bytes.extend_from_slice(&[255, 240]);
    bytes
}

/// World.Time at `hour`.
fn time(hour: u8) -> Vec<u8> {
    gmcp(
        "World.Time",
        &format!(
            r#"{{"hour":{hour},"day":3,"month":5,"year":1203,"sunlight":"light","sky":"cloudy"}}"#
        ),
    )
}

/// A read of `bytes`, and when the late repaint fires after it, as the
/// session loop decides with `waiting` the time it had.
fn read_then_wait(
    session: &mut Session,
    bytes: &[u8],
    waiting: Option<Instant>,
    now: Instant,
) -> (Read, Option<Instant>) {
    let read = session.read(bytes);
    let next = late_repaint_after(&session.p, waiting, read.gmcp, read.out.writes_text(), now);
    (read, next)
}

fn plain(bytes: &[u8]) -> String {
    vosh_ansi::plain_text(bytes)
}

#[test]
fn a_packet_with_no_text_after_it_repaints_the_open_row_late() {
    for show in [PromptShow::Text, PromptShow::Lifted] {
        let mut session = Session::new(showing(profile(CODES, HOUR, true), show));
        let now = Instant::now();
        let (_, waiting) = read_then_wait(&mut session, &wire_fixture("quiet"), None, now);
        assert_eq!(waiting, None, "{show:?}: the prompt drew after its packets");
        let (read, waiting) = read_then_wait(&mut session, &time(14), None, now);
        assert!(!read.out.writes_text(), "{show:?}");
        assert_eq!(waiting, Some(now + LATE_REPAINT), "{show:?}");
        // Another packet keeps the time the first one set.
        let later = now + Duration::from_millis(10);
        let (_, again) = read_then_wait(&mut session, &time(15), waiting, later);
        assert_eq!(again, waiting, "{show:?}");
        // It fires: a replace of the open row with nothing after it.
        let out = repaint_step(&mut session.p, false, later);
        let replace = out.replace.as_ref().expect("the repaint");
        assert!(out.bytes.is_empty(), "{show:?}");
        assert!(
            plain(&replace.bytes).starts_with("<1020> 15"),
            "{show:?}: {:?}",
            plain(&replace.bytes)
        );
        // The open row the card reads holds the new pieces.
        let state = crate::prompt_commands::prompt_state(&session.p);
        let open = state.open_row.expect("the open row");
        assert_eq!(open.plain, "<1020> 15", "{show:?}");
        assert_eq!(
            open.gen,
            session.p.prompt.stage.open_row().expect("the row").gen
        );
        // Nothing is left to change.
        assert_eq!(
            late_repaint_after(&session.p, None, true, false, later),
            None,
            "{show:?}"
        );
    }
}

#[test]
fn a_packet_after_the_prompt_in_its_read_repaints_it_late() {
    for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
        let mut session = Session::new(showing(profile(CODES, HOUR, true), show));
        let now = Instant::now();
        // The game sends the hour right after the prompt, in one read.
        let mut bytes = wire_fixture("quiet");
        bytes.extend(time(14));
        let (read, waiting) = read_then_wait(&mut session, &bytes, None, now);
        assert!(read.prompt, "{show:?}");
        assert!(read.gmcp, "{show:?}: the packet came after the prompt");
        assert_eq!(waiting, Some(now + LATE_REPAINT), "{show:?}");
        let out = repaint_step(&mut session.p, false, now);
        let shown = match show {
            PromptShow::Pinned => out.pin.as_deref().map(plain),
            _ => out.replace.as_ref().map(|r| plain(&r.bytes)),
        };
        assert!(
            shown.as_deref().is_some_and(|s| s.starts_with("<1020> 14")),
            "{show:?}: {shown:?}"
        );
    }

    // Packets before the prompt in its read draw with it, so nothing waits.
    for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
        let mut session = Session::new(showing(profile(CODES, HOUR, true), show));
        let now = Instant::now();
        let mut bytes = time(14);
        bytes.extend(wire_fixture("quiet"));
        let (read, waiting) = read_then_wait(&mut session, &bytes, None, now);
        assert!(!read.gmcp, "{show:?}");
        assert_eq!(waiting, None, "{show:?}");
    }

    // Text after the packet still cancels it.
    let now = Instant::now();
    let mut session = Session::new(profile(CODES, HOUR, true));
    let mut bytes = wire_fixture("quiet");
    bytes.extend(time(14));
    bytes.extend_from_slice(b"\n\rTarvik tells you 'back soon'\n\r");
    let (_, waiting) = read_then_wait(&mut session, &bytes, None, now);
    assert_eq!(waiting, None);
}

#[test]
fn text_after_the_packet_cancels_the_late_repaint() {
    let now = Instant::now();
    let mut session = Session::new(profile(CODES, HOUR, true));
    let _ = session.read(&wire_fixture("quiet"));
    let (_, waiting) = read_then_wait(&mut session, &time(14), None, now);
    assert!(waiting.is_some());
    let tell = b"\n\rTarvik tells you 'back soon'\n\r";
    let (_, after) = read_then_wait(&mut session, tell, waiting, now);
    assert_eq!(after, None);
    // The text closed the row, so a repaint changes nothing.
    assert!(repaint_step(&mut session.p, false, now).is_empty());

    // Text in the same read as the packet cancels it before it starts.
    let mut session = Session::new(profile(CODES, HOUR, true));
    let _ = session.read(&wire_fixture("quiet"));
    let mut bytes = time(14);
    bytes.extend_from_slice(tell);
    let (_, waiting) = read_then_wait(&mut session, &bytes, None, now);
    assert_eq!(waiting, None);

    // A packet that changes nothing your design reads starts nothing.
    let mut session = Session::new(profile(CODES, "<%hp>", true));
    let _ = session.read(&wire_fixture("quiet"));
    let (_, waiting) = read_then_wait(&mut session, &time(14), None, now);
    assert_eq!(waiting, None);
    // Nor does one with drawing off, which shows the game prompt as sent.
    let mut session = Session::new(profile(CODES, HOUR, false));
    let _ = session.read(&wire_fixture("quiet"));
    let (_, waiting) = read_then_wait(&mut session, &time(14), None, now);
    assert_eq!(waiting, None);
}

#[test]
fn the_pinned_band_repaints_late_whatever_text_came() {
    let now = Instant::now();
    let mut session = Session::new(showing(profile(CODES, HOUR, true), PromptShow::Pinned));
    let _ = session.read(&wire_fixture("quiet"));
    let mut bytes = time(14);
    bytes.extend_from_slice(b"\n\rTarvik tells you 'back soon'\n\r");
    let (read, waiting) = read_then_wait(&mut session, &bytes, None, now);
    assert!(read.out.writes_text());
    assert_eq!(waiting, Some(now + LATE_REPAINT));
    // Text that comes while it waits keeps it waiting.
    let (_, still) = read_then_wait(&mut session, b"\n\rA guard arrives.\n\r", waiting, now);
    assert_eq!(still, waiting);
    let out = repaint_step(&mut session.p, false, now);
    assert!(out.replace.is_none() && out.bytes.is_empty() && out.restore.is_none());
    assert_eq!(out.pin.as_deref().map(plain).as_deref(), Some("<1020> 14"));
}

#[test]
fn a_late_repaint_keeps_the_preview_and_carries_the_new_live_render() {
    let now = Instant::now();
    let mut session = Session::new(profile(CODES, HOUR, true));
    let _ = session.read(&wire_fixture("quiet"));
    session.p.prompt.set_preview(Some(PromptPreview {
        preview: Some(Preview::LowHealth),
        ..PromptPreview::default()
    }));
    let _ = session.repaint();
    let (_, waiting) = read_then_wait(&mut session, &time(14), None, now);
    assert!(waiting.is_some());
    let out = repaint_step(&mut session.p, false, now);
    let replace = out.replace.as_ref().expect("the repaint");
    assert!(plain(&replace.bytes).starts_with("<180> 14"));
    let restore = out.restore.as_ref().expect("the live render");
    assert!(plain(restore).starts_with("<1020> 14"));
}

#[test]
fn output_from_elsewhere_before_it_fires_leaves_the_row_alone() {
    let now = Instant::now();
    let mut session = Session::new(profile(CODES, HOUR, true));
    let _ = session.read(&wire_fixture("quiet"));
    let (_, waiting) = read_then_wait(&mut session, &time(14), None, now);
    assert!(waiting.is_some());
    // A slash command echoed in the meantime.
    assert!(repaint_step(&mut session.p, true, now).is_empty());
}
