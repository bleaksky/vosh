//! Save as file over two logs of Orla's: a login with its password and the
//! look at Thickening Woods on the evening of October 3, 2026, and a say
//! just after midnight. The lines come from the scene tests, which take
//! them from fixtures/room-colors. The goldens in fixtures/saved-logs hold
//! each kind of file, and `VOSH_WRITE_SAVED_LOGS=1` writes them again.

use chrono::{Local, TimeZone};
use vosh_log::{LineKind, LogEntry, LogStore, Scope};

use super::*;

/// A made up secret. It is nobody's password.
const SECRET: &str = "Zq7vellumSparrow";

/// `hour:minute` on the local clock of `day` in October 2026, plus
/// `seconds`.
fn at(day: u32, hour: u32, minute: u32, seconds: i64) -> i64 {
    Local
        .with_ymd_and_hms(2026, 10, day, hour, minute, 0)
        .single()
        .expect("a local time")
        .timestamp_millis()
        + seconds * 1000
}

/// Add `rows` to log `id`, a sent row keeping no bytes as the session
/// writes it.
fn add(store: &mut LogStore, id: i64, rows: &[(i64, &[u8], LineKind)]) {
    let entries: Vec<LogEntry> = rows
        .iter()
        .map(|(ts_ms, bytes, kind)| {
            let sent = *kind == LineKind::Sent;
            LogEntry {
                session_id: id,
                ts_ms: *ts_ms,
                text: vosh_protocol::ansi::plain_text(bytes),
                raw: (!sent).then(|| bytes.to_vec()),
                kind: kind.clone(),
            }
        })
        .collect();
    store.append_batch(&entries).expect("the rows");
}

/// The evening's log, from the main menu to Maren walking in.
fn evening(store: &mut LogStore) -> i64 {
    let id = store
        .start_session("play.theforsakenlands.com", 1848, at(3, 21, 12, 0))
        .expect("a log");
    store.set_session_character(id, "Orla").expect("a name");
    let secret = format!("> {SECRET}");
    add(
        store,
        id,
        &[
            (
                at(3, 21, 12, 0),
                b"\x1b[0mAbandon hope, all ye who enter here...",
                LineKind::Login,
            ),
            (at(3, 21, 12, 1), b"> e", LineKind::Sent),
            (at(3, 21, 12, 1), b"\x1b[0m", LineKind::Login),
            (at(3, 21, 12, 2), b"> tester", LineKind::Sent),
            (at(3, 21, 12, 2), b"\x1b[0m", LineKind::Login),
            (at(3, 21, 12, 3), secret.as_bytes(), LineKind::Sent),
            (at(3, 21, 14, 0), b"> look", LineKind::Sent),
            (
                at(3, 21, 14, 1),
                b"\x1b[38;5;28m\x1b[0;32mThickening Woods\x1b[0;0m\x1b[0;0m",
                LineKind::Text,
            ),
            (at(3, 21, 14, 1), b"[Exits: east west]", LineKind::Text),
            (at(3, 21, 14, 1), b"Maren walks in.", LineKind::Text),
        ],
    );
    id
}

/// The log just after midnight, Tolliver's say.
fn after_midnight(store: &mut LogStore) -> i64 {
    let id = store
        .start_session("play.theforsakenlands.com", 1848, at(4, 0, 2, 0))
        .expect("a log");
    store.set_session_character(id, "Orla").expect("a name");
    add(
        store,
        id,
        &[(
            at(4, 0, 2, 5),
            b"Tolliver says '\x1b[0;1;33mThe day has begun.\x1b[0;0m'",
            LineKind::Channel("say".into()),
        )],
    );
    id
}

fn palette() -> ScenePalette {
    ScenePalette {
        background: "#050403".into(),
        foreground: "#c0bdbb".into(),
        muted: "#646260".into(),
        ansi: (0..16).map(|n| format!("#1111{n:02x}")).collect(),
    }
}

fn options(format: SceneFormat, times: bool) -> FileOptions {
    FileOptions {
        format,
        times,
        palette: (format == SceneFormat::Html).then(palette),
    }
}

/// The file `scope` saves as with `options`.
fn saved(store: &LogStore, scope: &Scope, options: &FileOptions) -> Vec<u8> {
    let mut out = Vec::new();
    write(store, scope, options, "Vosh log, last 7 days", &mut out).expect("written");
    out
}

/// Hold `bytes` to the golden `name` in fixtures/saved-logs.
fn golden(name: &str, bytes: &[u8]) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/saved-logs")
        .join(name);
    if std::env::var_os("VOSH_WRITE_SAVED_LOGS").is_some() {
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
        std::fs::write(&path, bytes).expect("the golden");
    }
    let want = std::fs::read(&path).expect("the golden, VOSH_WRITE_SAVED_LOGS=1 writes it");
    assert_eq!(
        String::from_utf8_lossy(bytes),
        String::from_utf8_lossy(&want),
        "{name}"
    );
}

#[test]
fn each_kind_of_file_matches_its_golden() {
    let mut store = LogStore::in_memory().expect("a log");
    let one = evening(&mut store);
    after_midnight(&mut store);
    let both = Scope::default();
    let evening_only = Scope::log(one);
    for (name, scope, options) in [
        (
            "one-day.txt",
            &evening_only,
            options(SceneFormat::Text, true),
        ),
        ("two-days.txt", &both, options(SceneFormat::Text, true)),
        ("two-days.ansi", &both, options(SceneFormat::Ansi, true)),
        ("two-days.html", &both, options(SceneFormat::Html, true)),
        (
            "no-times.html",
            &evening_only,
            options(SceneFormat::Html, false),
        ),
    ] {
        let bytes = saved(&store, scope, &options);
        assert!(
            !String::from_utf8_lossy(&bytes).contains(SECRET),
            "{name} holds the password"
        );
        golden(name, &bytes);
    }
}

#[test]
fn times_off_writes_the_lines_as_before() {
    let mut store = LogStore::in_memory().expect("a log");
    evening(&mut store);
    after_midnight(&mut store);
    let scope = Scope::default();
    for format in [SceneFormat::Text, SceneFormat::Ansi] {
        let mut before = Vec::new();
        store
            .export_scope(&scope, format == SceneFormat::Ansi, true, &mut before)
            .expect("exported");
        assert_eq!(saved(&store, &scope, &options(format, false)), before);
    }
}

#[test]
fn a_line_starts_with_its_time_as_the_log_view_shows_it() {
    let mut store = LogStore::in_memory().expect("a log");
    let id = after_midnight(&mut store);
    let text = saved(&store, &Scope::log(id), &options(SceneFormat::Text, true));
    // One day, so no day above it, and the hour has no leading zero.
    assert_eq!(
        String::from_utf8_lossy(&text),
        " 0:02 Tolliver says 'The day has begun.'\n"
    );
}

#[test]
fn a_day_that_turns_while_saving_is_named_too() {
    // The header saw one day, and a line of the next came after.
    let mut clock = Clock {
        days: false,
        day: None,
    };
    assert_eq!(clock.day_above(at(3, 23, 59, 0)), None);
    assert_eq!(clock.day_above(at(3, 23, 59, 30)), None);
    assert_eq!(
        clock.day_above(at(4, 0, 0, 1)).as_deref(),
        Some("October 4, 2026")
    );
    let mut clock = Clock {
        days: true,
        day: None,
    };
    assert_eq!(
        clock.day_above(at(3, 21, 0, 0)).as_deref(),
        Some("October 3, 2026")
    );
    assert_eq!(clock.day_above(at(3, 22, 0, 0)), None);
}

#[test]
fn the_page_names_who_played_where() {
    let mut store = LogStore::in_memory().expect("a log");
    evening(&mut store);
    let id = store
        .start_session("play.theforsakenlands.com", 1848, at(4, 9, 0, 0))
        .expect("a log");
    store.set_session_character(id, "Maren").expect("a name");
    add(
        &mut store,
        id,
        &[(at(4, 9, 0, 1), b"Orla waves.", LineKind::Text)],
    );
    let spans = store.scope_spans(&Scope::default()).expect("spans");
    assert_eq!(
        place(&spans).as_deref(),
        Some("Orla and Maren in The Forsaken Lands")
    );
    assert_eq!(place(&[]), None);
    assert_eq!(
        and_list(&["Orla", "Maren", "Tolliver"].map(String::from)),
        "Orla, Maren and Tolliver"
    );
}

#[test]
fn an_empty_scope_saves_a_page_that_says_so() {
    let store = LogStore::in_memory().expect("a log");
    let html = saved(&store, &Scope::log(99), &options(SceneFormat::Html, true));
    let html = String::from_utf8_lossy(&html);
    assert!(html.contains("<h1>Vosh log, last 7 days</h1>"), "{html}");
    assert!(html.contains("<p>Nothing saved here yet.</p>"));
    assert!(html.contains("<pre></pre>"));
}

#[test]
fn a_line_of_the_game_never_adds_markup_to_a_saved_page() {
    let mut store = LogStore::in_memory().expect("a log");
    let id = store
        .start_session("play.theforsakenlands.com", 1848, at(3, 21, 0, 0))
        .expect("a log");
    store
        .set_session_character(id, "<b>Orla</b>")
        .expect("a name");
    add(
        &mut store,
        id,
        &[(
            at(3, 21, 0, 1),
            b"Tolliver says '<script>alert(1)</script> & more'",
            LineKind::Text,
        )],
    );
    let html = saved(&store, &Scope::log(id), &options(SceneFormat::Html, true));
    let html = String::from_utf8_lossy(&html);
    assert!(!html.contains("<script>"), "{html}");
    assert!(!html.contains("<b>"), "{html}");
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; more"));
    assert!(html.contains("<h1>&lt;b&gt;Orla&lt;/b&gt; in The Forsaken Lands</h1>"));
}
