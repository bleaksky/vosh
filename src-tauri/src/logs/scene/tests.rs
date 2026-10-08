//! Save a scene over a stretch of play: the look at Thickening
//! Woods from fixtures/room-colors/looks.json, Maren's arrival, then a
//! newbie line, a say, a tell and your own say and tell from
//! fixtures/room-colors/lines.json, with the prompts and commands between.

use chrono::{Local, TimeZone};
use vosh_log::{LineKind, LogEntry, LogStore};

use super::*;

const PROMPT: &str = "<1020hp 800m 930mv> ";

/// 21:14 on October 3, 2026, on the local clock, plus `seconds`.
fn at(minute: u32, seconds: i64) -> i64 {
    Local
        .with_ymd_and_hms(2026, 10, 3, 21, minute, 0)
        .single()
        .expect("a local time")
        .timestamp_millis()
        + seconds * 1000
}

/// The board's stretch in a log of its own, with the `> look` before it.
/// Returns the store and the range from the room's name to your tell.
fn board() -> (LogStore, SceneRange) {
    let mut store = LogStore::in_memory().expect("a log");
    let id = store
        .start_session("play.theforsakenlands.com", 1848, at(2, 0))
        .expect("a session");
    store.set_session_character(id, "Orla").expect("a name");
    let rows: Vec<(i64, &[u8], LineKind)> = vec![
        (at(14, 0), b"> look", LineKind::Sent),
        (
            at(14, 1),
            b"\x1b[38;5;28m\x1b[0;32mThickening Woods\x1b[0;0m\x1b[0;0m",
            LineKind::Text,
        ),
        (
            at(14, 1),
            b"  The trees thicken slightly around you but your path is still clearly",
            LineKind::Text,
        ),
        (
            at(14, 1),
            b"visible having been worn down by many feet.  The land under foot becomes a",
            LineKind::Text,
        ),
        (
            at(14, 1),
            b"little more rugged as you sense the terrain becoming slightly more uneven.  ",
            LineKind::Text,
        ),
        (at(14, 1), b"", LineKind::Text),
        (at(14, 1), b"[Exits: east west]", LineKind::Text),
        (
            at(14, 1),
            b"A horde of Demons hovers overhead, their talons dripping blood.\x1b[0;0m",
            LineKind::Text,
        ),
        (
            at(14, 1),
            b"A large murder of crows nearly turns the trees black here.",
            LineKind::Text,
        ),
        (at(14, 1), b"Maren walks in.", LineKind::Text),
        (at(14, 1), b"", LineKind::Text),
        (at(14, 1), PROMPT.as_bytes(), LineKind::Prompt),
        (
            at(15, 0),
            b"\x1b[0;1;32mTolliver NEWBIE chats: 'The night has begun.'\x1b[0;0m",
            LineKind::Channel("newbie".into()),
        ),
        (
            at(15, 0),
            b"Tolliver says '\x1b[0;1;33mThe day has begun.\x1b[0;0m'",
            LineKind::Channel("say".into()),
        ),
        (at(15, 0), PROMPT.as_bytes(), LineKind::Prompt),
        (at(15, 1), b"> say The day has begun.", LineKind::Sent),
        (
            at(15, 1),
            b"You say '\x1b[0;1;33mThe day has begun.\x1b[0;0m'",
            LineKind::Channel("say".into()),
        ),
        (
            at(15, 1),
            b"Tolliver tells you '\x1b[0;32m[Exits: north east south west]\x1b[0;0m'",
            LineKind::Channel("tell".into()),
        ),
        (at(15, 1), PROMPT.as_bytes(), LineKind::Prompt),
        (
            at(15, 2),
            b"> tell tolliver [Exits: north east south west]",
            LineKind::Sent,
        ),
        (
            at(15, 2),
            b"You tell Tolliver '\x1b[0;32m[Exits: north east south west]\x1b[0;0m'",
            LineKind::Channel("tell".into()),
        ),
    ];
    let entries: Vec<LogEntry> = rows
        .into_iter()
        .map(|(ts_ms, bytes, kind)| {
            let sent = kind == LineKind::Sent;
            LogEntry {
                session_id: id,
                ts_ms,
                text: vosh_protocol::ansi::plain_text(bytes),
                raw: (!sent).then(|| bytes.to_vec()),
                kind,
            }
        })
        .collect();
    store.append_batch(&entries).expect("the rows");
    let from = store
        .scene_lines(id, at(14, 1), at(14, 1), 1)
        .expect("rows")[0]
        .id;
    let range = SceneRange {
        log: id,
        from_ms: at(14, 0),
        to_ms: at(15, 59),
        from_id: Some(from),
        to_id: None,
    };
    (store, range)
}

/// The board's filter: prompts and your commands off, five channels out.
fn first_filter() -> SceneFilter {
    SceneFilter {
        prompts: false,
        commands: false,
        left_out: ["tell", "newbie", "pray", "immortal", "imp"]
            .map(String::from)
            .to_vec(),
    }
}

#[test]
fn the_preview_draws_what_stays_out_quiet_and_says_why() {
    let (store, range) = board();
    let span = read(&store, &range, PREVIEW_CAP).expect("the span");
    let preview = preview(span, &first_filter(), SceneFormat::Html, None);
    assert_eq!((preview.kept, preview.total), (12, 20));
    assert!(!preview.older && !preview.capped);
    assert_eq!(preview.file_name, "Thickening Woods, October 3.html");
    let why: Vec<Option<&str>> = preview.lines.iter().map(|l| l.out.as_deref()).collect();
    let mut expected = vec![None; 10];
    expected.extend([
        Some("prompt"),
        Some("newbie"),
        None,
        Some("prompt"),
        Some("your command"),
        None,
        Some("tell"),
        Some("prompt"),
        Some("your command"),
        Some("tell"),
    ]);
    assert_eq!(why, expected);
}

#[test]
fn the_file_holds_what_stays_with_a_header_and_a_footer() {
    let (store, range) = board();
    let dir = tempfile::tempdir().expect("a folder");
    let span = read(&store, &range, usize::MAX).expect("the span");
    let palette = ScenePalette {
        background: "#050403".into(),
        foreground: "#c0bdbb".into(),
        muted: "#646260".into(),
        ansi: (0..16).map(|n| format!("#1111{n:02x}")).collect(),
    };
    let name = save(
        &span,
        &first_filter(),
        SceneFormat::Html,
        Some(&palette),
        None,
        dir.path(),
    )
    .expect("saved");
    assert_eq!(name, "Thickening Woods, October 3.html");
    let html = std::fs::read_to_string(dir.path().join(&name)).expect("the file");
    assert!(
        html.contains("<title>Thickening Woods, October 3</title>"),
        "{html}"
    );
    assert!(html.contains("<h1>Thickening Woods</h1>"));
    assert!(
        html.contains("<p>Orla in The Forsaken Lands, October 3, 2026, from 21:14 to 21:15</p>")
    );
    assert!(html.contains(
        "<footer>Saved from Vosh. Prompts, your commands and five channels were left out.</footer>"
    ));
    assert!(html.contains(
        "Maren walks in.\n\nTolliver says '<span class=\"c11 b\">The day has begun.</span>'\nYou say '"
    ));
    assert!(!html.contains("NEWBIE"));
    assert!(!html.contains("tells you"));
    assert!(!html.contains("&lt;1020hp"));

    // A second scene of the same room that day gains a number.
    let again = save(
        &span,
        &first_filter(),
        SceneFormat::Html,
        Some(&palette),
        None,
        dir.path(),
    )
    .expect("saved again");
    assert_eq!(again, "Thickening Woods, October 3 (2).html");
}

#[test]
fn text_and_ansi_keep_the_same_lines() {
    let (store, range) = board();
    let dir = tempfile::tempdir().expect("a folder");
    let span = read(&store, &range, usize::MAX).expect("the span");
    let keep_all = SceneFilter {
        prompts: true,
        commands: true,
        left_out: Vec::new(),
    };
    let text = save(&span, &keep_all, SceneFormat::Text, None, None, dir.path()).expect("text");
    assert_eq!(text, "Thickening Woods, October 3.txt");
    let plain = std::fs::read_to_string(dir.path().join(&text)).expect("the file");
    assert_eq!(plain.lines().count(), 20);
    assert!(plain.starts_with("Thickening Woods\n"));
    assert!(plain.contains("> say The day has begun.\n"));
    let ansi = save(
        &span,
        &first_filter(),
        SceneFormat::Ansi,
        None,
        None,
        dir.path(),
    )
    .expect("ansi");
    assert_eq!(ansi, "Thickening Woods, October 3.log");
    let colored = std::fs::read(dir.path().join(&ansi)).expect("the file");
    assert!(colored.starts_with(b"\x1b[38;5;28m\x1b[0;32mThickening Woods"));
    assert_eq!(colored.split(|b| *b == b'\n').count() - 1, 12);
}

#[test]
fn a_blank_line_a_dropped_prompt_leaves_folds() {
    let line = |text: &str| SceneLine {
        id: 0,
        ts_ms: 0,
        text: text.to_string(),
        raw: Some(text.as_bytes().to_vec()),
        kind: None,
    };
    let lines = [
        line(""),
        line("Maren walks in."),
        line(""),
        line(PROMPT),
        line(""),
        line("Orla walks in."),
    ];
    let kinds = [
        LineKind::Text,
        LineKind::Text,
        LineKind::Text,
        LineKind::Prompt,
        LineKind::Text,
        LineKind::Text,
    ];
    let out = choose(&lines, &kinds, &SceneFilter::default());
    let why: Vec<Option<&str>> = out.iter().map(Option::as_deref).collect();
    assert_eq!(why, [Some(""), None, None, Some("prompt"), Some(""), None]);
}

#[test]
fn the_footer_names_what_was_left_out() {
    let filter = |prompts: bool, commands: bool, left_out: &[&str]| SceneFilter {
        prompts,
        commands,
        left_out: left_out.iter().map(ToString::to_string).collect(),
    };
    assert_eq!(footer(&filter(true, true, &[])), "Saved from Vosh.");
    assert_eq!(
        footer(&filter(true, true, &["tell"])),
        "Saved from Vosh. The tell channel was left out."
    );
    assert_eq!(
        footer(&filter(false, true, &[])),
        "Saved from Vosh. Prompts were left out."
    );
    assert_eq!(
        footer(&filter(true, false, &["tell", "newbie"])),
        "Saved from Vosh. Your commands and two channels were left out."
    );
}

#[test]
fn a_range_with_no_room_takes_a_plain_title() {
    let (store, mut range) = board();
    range.from_id = None;
    range.from_ms = at(15, 0);
    let span = read(&store, &range, PREVIEW_CAP).expect("the span");
    assert_eq!(file_stem(&span.lines), "Vosh scene, October 3");
    assert!(is_room_name(
        b"\x1b[38;5;255m\x1b[0;1;30mThe Bank of Aabahran\x1b[0;0m"
    ));
    assert!(!is_room_name(b"\x1b[0;32m[Exits: east west]"));
}

#[test]
fn a_room_name_saves_as_a_name_every_system_holds() {
    // Room names from the game's own areas.
    assert_eq!(
        file_safe("What Does RP-Enforced Mean?"),
        "What Does RP-Enforced Mean"
    );
    assert_eq!(file_safe("Room \"01\""), "Room '01'");
    assert_eq!(file_safe("Thickening Woods"), "Thickening Woods");
    assert_eq!(file_safe("a/b: c|d*"), "a-b- cd");
    assert_eq!(
        file_safe("Why is it getting so dark... ?"),
        "Why is it getting so dark"
    );
    assert_eq!(file_safe("??"), "Vosh scene");
}
