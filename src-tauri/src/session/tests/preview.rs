//! Previews on your prompt, played through the session's own steps.
//!
//! The card shows a preview on the open row, and only live renders reach
//! history: a preview left on while game text arrives, in one read or split
//! at any byte, leaves the screen the live session leaves with the card
//! open once your echo lands, on the native grid. In the text the open card
//! lends the row Lifted's band, trailing space included, so that screen
//! keeps your echo a cell after a design that ends on a character. The
//! webview test replays the same payloads into xterm from a stored file.

use super::*;
use crate::output::{base64_encode, OutputPayload};
use vosh_prompt::values::overrides::{Overrides, Preview, PromptPreview};
use vosh_prompt::PromptShow;

/// The card's Low health preview, which draws `<180>` for [`HP`].
fn low_health() -> PromptPreview {
    PromptPreview {
        preview: Some(Preview::LowHealth),
        ..PromptPreview::default()
    }
}

/// `bytes` read on a new connection of `session`, cut at `at`, with
/// `preview` on from the start, as the card would have set it.
fn replay_with(
    session: &mut Session,
    bytes: &[u8],
    at: &[usize],
    preview: Option<&PromptPreview>,
) -> Vec<Read> {
    session.restart();
    session.c.prompt.set_preview(preview.cloned());
    vosh_prompt::testkit::reads(bytes, at)
        .into_iter()
        .map(|read| session.read(read))
        .collect()
}

/// `bytes` read on a new connection of `session`, cut at `at`, live with
/// the card open: no preview, and in the text the band the card lends.
fn replay_live_with_card(session: &mut Session, bytes: &[u8], at: &[usize]) -> Vec<Read> {
    session.restart();
    session.c.prompt.set_preview(None);
    session.c.prompt.stage.set_card(true);
    vosh_prompt::testkit::reads(bytes, at)
        .into_iter()
        .map(|read| session.read(read))
        .collect()
}

/// The native grid's screen, `columns` wide, after `outputs` and then
/// your echo of `look`, which lands after the open row.
fn screen_after_echo<'a>(
    outputs: impl IntoIterator<Item = &'a Output>,
    columns: usize,
) -> Vec<String> {
    let mut grid = crate::native::grid::TermGrid::new(columns, 60);
    for out in outputs {
        grid.session_output(out);
    }
    grid.local_write(b"look\r\n");
    rows_of(&grid)
}

/// The native grid's screen, `columns` wide, after `outputs`.
fn screen<'a>(outputs: impl IntoIterator<Item = &'a Output>, columns: usize) -> Vec<String> {
    let mut grid = crate::native::grid::TermGrid::new(columns, 60);
    for out in outputs {
        grid.session_output(out);
    }
    rows_of(&grid)
}

fn outs(reads: &[Read]) -> impl Iterator<Item = &Output> {
    reads.iter().map(|read| &read.out)
}

#[test]
fn a_prompt_drawn_with_a_preview_shows_it_until_something_lands_after_it() {
    let mut session = Session::new(profile(CODES, HP, true));
    session.c.prompt.set_preview(Some(low_health()));
    let read = session.read(&wire_fixture("quiet"));
    // The open row shows the preview, with the live render as its
    // restore.
    let restore = read.out.restore.clone().expect("the live render");
    assert!(String::from_utf8_lossy(&restore).contains("<1020>"));
    let rows = screen([&read.out], 80);
    assert_eq!(rows.last().map(String::as_str), Some("<180>"));
    // Your echo puts the live render back first, with the card's band
    // and the space after it.
    let rows = screen_after_echo([&read.out], 80);
    assert_eq!(rows.last().map(String::as_str), Some("<1020> look"));
    // The panes keep the live values.
    let vars = session.c.prompt.take_prompt_vars(true).expect("the vars");
    assert_eq!(vars.get("hp").map(String::as_str), Some("1020"));
    // The open row keeps the spans of what it shows.
    let spans = &session.c.prompt.stage.open_row().expect("the row").spans;
    assert_eq!(spans.len(), 3);
}

#[test]
fn a_preview_left_on_while_game_text_arrives_leaves_only_live_renders_at_every_split() {
    for (name, bytes, prompt) in pinned_streams() {
        for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
            let mut live = Session::new(showing(profile(prompt, HP, true), show));
            let mut previewing = Session::new(showing(profile(prompt, HP, true), show));
            let mut splits = vec![Vec::new()];
            splits.extend(cuts(&bytes).into_iter().map(|at| vec![at]));
            for at in &splits {
                let want = replay_live_with_card(&mut live, &bytes, at);
                let got = replay_with(&mut previewing, &bytes, at, Some(&low_health()));
                let label = format!("{name} {show:?} cut {at:?}");
                let flat = |reads: &[Read]| {
                    (
                        reads.iter().flat_map(|r| r.log.clone()).collect::<Vec<_>>(),
                        reads
                            .iter()
                            .flat_map(|r| r.kept.clone())
                            .collect::<Vec<_>>(),
                    )
                };
                assert_eq!(flat(&got), flat(&want), "log and scrollback, {label}");
                for columns in [40, 12] {
                    assert_eq!(
                        screen_after_echo(outs(&got), columns),
                        screen_after_echo(outs(&want), columns),
                        "{label} {columns} wide"
                    );
                }
                // Nothing reaches the text in Pinned, where the band shows
                // the preview.
                if show == PromptShow::Pinned {
                    assert_eq!(screen(outs(&got), 40), screen(outs(&want), 40), "{label}");
                    assert!(got.iter().all(|r| r.out.restore.is_none()), "{label}");
                }
            }
        }
    }
}

#[test]
fn a_preview_set_on_the_open_row_gives_way_to_the_next_pulse_at_every_split() {
    let quiet = wire_fixture("quiet");
    let fight = wire_fixture("fight-tank");
    for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
        let mut live = Session::new(showing(profile(CODES, HP, true), show));
        let mut previewing = Session::new(showing(profile(CODES, HP, true), show));
        let mut splits = vec![Vec::new()];
        splits.extend(cuts(&fight).into_iter().map(|at| vec![at]));
        for at in &splits {
            let label = format!("{show:?} cut {at:?}");
            let mut want = replay_with(&mut live, &quiet, &[], None);
            // The card opens on the live row, lending it the band in the
            // text.
            live.c.prompt.stage.set_card(true);
            want.push(Read {
                out: live.repaint(),
                ..Read::default()
            });
            want.extend(
                vosh_prompt::testkit::reads(&fight, at)
                    .into_iter()
                    .map(|read| live.read(read)),
            );
            let mut got = replay_with(&mut previewing, &quiet, &[], None);
            // The card sets Low health on the open row.
            previewing.c.prompt.set_preview(Some(low_health()));
            let repaint = previewing.repaint();
            match show {
                PromptShow::Pinned => {
                    assert_eq!(
                        repaint
                            .pin
                            .as_deref()
                            .map(vosh_protocol::ansi::plain_text)
                            .as_deref(),
                        Some("<180>"),
                        "{label}"
                    );
                    assert!(repaint.restore.is_none(), "{label}");
                }
                _ => {
                    let replace = repaint.replace.as_ref().expect("the repaint");
                    assert!(
                        vosh_protocol::ansi::plain_text(&replace.bytes).contains("<180>"),
                        "{label}"
                    );
                    let restore = repaint.restore.as_ref().expect("the live render");
                    assert!(
                        vosh_protocol::ansi::plain_text(restore).contains("<1020>"),
                        "{label}"
                    );
                }
            }
            got.push(Read {
                out: repaint,
                ..Read::default()
            });
            got.extend(
                vosh_prompt::testkit::reads(&fight, at)
                    .into_iter()
                    .map(|read| previewing.read(read)),
            );
            for columns in [40, 12] {
                assert_eq!(
                    screen_after_echo(outs(&got), columns),
                    screen_after_echo(outs(&want), columns),
                    "{label} {columns} wide"
                );
            }
        }
    }
}

#[test]
fn the_card_closing_repaints_the_live_render_with_nothing_to_restore() {
    let mut session = Session::new(profile(CODES, "<%hp>%opponent", true));
    let read = session.read(&wire_fixture("quiet"));
    assert_eq!(read.out.restore, None);
    // The card opens: values with nothing to show draw their labels.
    session.c.prompt.set_preview(Some(PromptPreview {
        placeholders: true,
        ..PromptPreview::default()
    }));
    // In the text it lends the row Lifted's band, and the space after it.
    let open = session.repaint();
    let replace = open.replace.as_ref().expect("the repaint");
    assert_eq!(drawn_text(&replace.bytes), "<1020>Opponent ");
    assert_eq!(
        drawn_text(open.restore.as_ref().expect("restore")),
        "<1020> "
    );
    // It reads your codes: the row shows the line the game sent.
    session.c.prompt.set_preview(Some(PromptPreview {
        raw: true,
        ..PromptPreview::default()
    }));
    let raw = session.repaint();
    let replace = raw.replace.as_ref().expect("the repaint");
    assert_eq!(
        drawn_text(&replace.bytes),
        "[1020/1020hp 800/800mn 930/930mv]\r\n"
    );
    assert_eq!(
        drawn_text(raw.restore.as_ref().expect("restore")),
        "<1020> "
    );
    // A preview with values on top of Fight.
    session.c.prompt.set_preview(Some(PromptPreview {
        preview: Some(Preview::Fight),
        overrides: Some(Overrides {
            values: [("opponent".to_string(), serde_json::json!("a rat"))].into(),
            lament: false,
        }),
        ..PromptPreview::default()
    }));
    let fight = session.repaint();
    assert_eq!(
        drawn_text(&fight.replace.as_ref().expect("the repaint").bytes),
        "<1020>a rat "
    );
    // The card closes.
    session.c.prompt.set_preview(None);
    let closed = session.repaint();
    assert_eq!(
        drawn_text(&closed.replace.as_ref().expect("the repaint").bytes),
        "<1020> "
    );
    assert_eq!(closed.restore, None);
    // With nothing to change, nothing goes out.
    assert!(session.repaint().is_empty());
}

/// The plain text a region shows after its mark.
fn drawn_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let after = match text.rfind("\x1b]7717;o;") {
        Some(at) => &text[at..],
        None => &text[..],
    };
    let after = after.find('\x07').map_or(after, |end| &after[end + 1..]);
    vosh_protocol::ansi::plain_text(after.as_bytes())
}

#[test]
fn a_repaint_in_a_payload_carries_the_restore_to_the_webview() {
    let mut session = Session::new(profile(CODES, HP, true));
    let _ = session.read(&wire_fixture("quiet"));
    session.c.prompt.set_preview(Some(low_health()));
    let json = payload(&session.repaint()).expect("a payload");
    let value: serde_json::Value = serde_json::from_str(&json).expect("json");
    let restore = value["restore"].as_str().expect("the restore");
    let bytes = base64_decode(restore);
    assert_eq!(drawn_text(&bytes), "<1020> ");
    // The card lends the row the band of Lifted, so its live render ends
    // the lift too, with Lifted's space after it.
    let text = String::from_utf8_lossy(&bytes);
    let end = text
        .rfind("<1020>\x1b[0m\x1b]7717;e;")
        .expect("the lift ends");
    assert!(text[end..].ends_with("\x07 "));
}

/// The payloads of every stream in [`pinned_streams`] with Low health on
/// from the start, in the text and lifted, as one read and as two cut at
/// every place [`cuts`] names, with the screen the native grid shows for
/// the live session after your echo, 40 and 12 wide. The webview test
/// replays them into xterm, then your echo, and holds each screen to the
/// grid's.
fn preview_splits() -> serde_json::Value {
    let mut streams = Vec::new();
    for (name, bytes, prompt) in pinned_streams() {
        for show in [PromptShow::Text, PromptShow::Lifted] {
            let mut live = Session::new(showing(profile(prompt, HP, true), show));
            let mut previewing = Session::new(showing(profile(prompt, HP, true), show));
            let mut splits = vec![Vec::new()];
            splits.extend(cuts(&bytes).into_iter().map(|at| vec![at]));
            let payloads: Vec<Vec<serde_json::Value>> = splits
                .iter()
                .map(|at| {
                    replay_with(&mut previewing, &bytes, at, Some(&low_health()))
                        .iter()
                        .filter(|read| !read.out.is_empty())
                        .map(|read| {
                            serde_json::to_value(OutputPayload::from_output(&read.out))
                                .expect("it serializes")
                        })
                        .collect()
                })
                .collect();
            let whole = replay_live_with_card(&mut live, &bytes, &[]);
            let screens: serde_json::Map<String, serde_json::Value> = [40, 12]
                .into_iter()
                .map(|columns| {
                    (
                        columns.to_string(),
                        serde_json::json!(screen_after_echo(outs(&whole), columns)),
                    )
                })
                .collect();
            streams.push(serde_json::json!({
                "name": name,
                "show": show,
                "screens": screens,
                "splits": payloads,
            }));
        }
    }
    serde_json::json!({ "streams": streams })
}

/// The file the webview test reads: the JSON of [`preview_splits`],
/// gzipped, as base64 text.
fn preview_splits_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/prompt/aabahran/preview/splits.b64")
}

/// Write [`preview_splits`] for the webview test when
/// `VOSH_WRITE_PREVIEW_SPLITS` is set. Nothing otherwise.
#[test]
fn write_the_preview_splits_for_the_webview() {
    use std::io::Write as _;
    if std::env::var("VOSH_WRITE_PREVIEW_SPLITS").is_err() {
        return;
    }
    let text = serde_json::to_string(&preview_splits()).expect("json");
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(text.as_bytes()).expect("gzip");
    let encoded = base64_encode(&gz.finish().expect("gzip"));
    let mut lines: Vec<&str> = encoded
        .as_bytes()
        .chunks(100)
        .map(|c| std::str::from_utf8(c).expect("ascii"))
        .collect();
    lines.push("");
    let path = preview_splits_path();
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
    std::fs::write(&path, lines.join("\n")).expect("the file");
}

#[test]
fn the_preview_splits_the_webview_replays_are_what_the_session_sends() {
    use std::io::Read as _;
    let stored = std::fs::read_to_string(preview_splits_path()).expect(
        "fixtures/prompt/aabahran/preview/splits.b64, written with VOSH_WRITE_PREVIEW_SPLITS=1",
    );
    let bytes = base64_decode(&stored);
    let mut text = String::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_string(&mut text)
        .expect("gzip");
    let stored: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert!(
        stored == preview_splits(),
        "the session changed, so write the file again with VOSH_WRITE_PREVIEW_SPLITS=1"
    );
}

/// The native grid `columns` wide after `outputs`, as its rows.
fn grid_after<'a>(
    grid: &mut crate::native::grid::TermGrid,
    outputs: impl IntoIterator<Item = &'a Output>,
) -> Vec<String> {
    for out in outputs {
        grid.session_output(out);
    }
    rows_of(grid)
}

#[test]
fn the_open_row_stays_open_across_a_resize_while_the_card_is_open() {
    for show in [PromptShow::Text, PromptShow::Lifted] {
        let label = format!("{show:?}");
        let mut session = Session::new(showing(profile(CODES, HP, true), show));
        let mut negotiator = vosh_protocol::telnet::Negotiator::new();
        negotiator.set_window_size(80, 40);
        let mut grid = crate::native::grid::TermGrid::new(80, 40);
        let quiet = session.read(&wire_fixture("quiet"));
        let _ = grid_after(&mut grid, [&quiet.out]);

        // The card shows Low health, then the panel opens and the
        // terminal narrows.
        session.c.prompt.set_preview(Some(low_health()));
        let low = session.repaint();
        assert_eq!(
            grid_after(&mut grid, [&low]).last().map(String::as_str),
            Some("<180>"),
            "{label}"
        );
        window_size_step(&mut session.c, &mut negotiator, 60, 40, false);
        grid.resize(60, 40);
        assert!(session.c.prompt.stage.open_row().is_some(), "{label}");

        // Clearing the preview puts the live render back.
        session.c.prompt.set_preview(None);
        let live = session.repaint();
        assert!(live.restore.is_none(), "{label}");
        assert_eq!(
            grid_after(&mut grid, [&live]).last().map(String::as_str),
            Some("<1020>"),
            "{label}"
        );

        // With no preview, the open card keeps the row open too, so an
        // edit repaints it.
        window_size_step(&mut session.c, &mut negotiator, 70, 40, true);
        grid.resize(70, 40);
        assert!(session.c.prompt.stage.open_row().is_some(), "{label}");
        let mut config = session.c.prompt.config().clone();
        config.template = "[%hp]".into();
        take_config(&mut session.p, &mut session.c, config);
        let edited = session.repaint();
        assert_eq!(
            grid_after(&mut grid, [&edited]).last().map(String::as_str),
            Some("[1020]"),
            "{label}"
        );

        // With the card closed, a new size closes the row.
        window_size_step(&mut session.c, &mut negotiator, 80, 40, false);
        assert!(session.c.prompt.stage.open_row().is_none(), "{label}");
        assert!(session.repaint().is_empty(), "{label}");
    }
}

#[test]
fn a_new_height_leaves_the_open_row_open_with_the_card_closed() {
    for show in [PromptShow::Text, PromptShow::Lifted] {
        let label = format!("{show:?}");
        let mut session = Session::new(showing(profile(CODES, HP, true), show));
        let mut negotiator = vosh_protocol::telnet::Negotiator::new();
        negotiator.set_window_size(80, 40);
        let mut grid = crate::native::grid::TermGrid::new(80, 40);
        let quiet = session.read(&wire_fixture("quiet"));
        let _ = grid_after(&mut grid, [&quiet.out]);

        // Your prompt leaves the band for the text and the terminal
        // grows, then a panel shortens it, all with the card closed. The
        // width stays, so nothing wraps again and a design change still
        // repaints the row in place on the native grid.
        for (rows, template, drawn) in [(44, "[%hp]", "[1020]"), (30, "{%hp}", "{1020}")] {
            window_size_step(&mut session.c, &mut negotiator, 80, rows, false);
            grid.resize(80, usize::from(rows));
            assert!(
                session.c.prompt.stage.open_row().is_some(),
                "{label} {rows}"
            );
            let mut config = session.c.prompt.config().clone();
            config.template = template.into();
            take_config(&mut session.p, &mut session.c, config);
            let edited = session.repaint();
            let screen = grid_after(&mut grid, [&edited]);
            assert_eq!(
                screen.last().map(String::as_str),
                Some(drawn),
                "{label} {rows}"
            );
            assert_eq!(
                screen.iter().filter(|row| row.contains("1020")).count(),
                1,
                "{label} {rows} {screen:#?}"
            );
        }
    }
}

#[test]
fn the_live_render_comes_back_when_the_connection_ends_during_a_preview() {
    let now = Instant::now();
    for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
        let label = format!("{show:?}");
        let mut session = Session::new(showing(profile(CODES, HP, true), show));
        let mut grid = crate::native::grid::TermGrid::new(80, 40);
        let quiet = session.read(&wire_fixture("quiet"));
        session.c.prompt.set_preview(Some(low_health()));
        let low = session.repaint();
        let _ = grid_after(&mut grid, [&quiet.out, &low]);
        // You disconnect with the card open, and nothing else lands.
        let out = end_preview_step(&session.p, &mut session.c, false, now);
        assert!(session.c.prompt.preview().is_none(), "{label}");
        assert!(out.restore.is_none(), "{label}");
        if show == PromptShow::Pinned {
            assert_eq!(
                out.pin
                    .as_deref()
                    .map(vosh_protocol::ansi::plain_text)
                    .as_deref(),
                Some("<1020>"),
                "{label}"
            );
        } else {
            assert_eq!(
                grid_after(&mut grid, [&out]).last().map(String::as_str),
                Some("<1020>"),
                "{label}"
            );
        }
    }

    // With no preview, the connection ends with nothing to write.
    let mut session = Session::new(profile(CODES, HP, true));
    let _ = session.read(&wire_fixture("quiet"));
    assert!(end_preview_step(&session.p, &mut session.c, false, now).is_empty());
}
