//! Previews on your prompt, played through the session's own steps
//! (section 4, Live edits and previews, and section 7 step 8).
//!
//! The card shows a preview on the open row, and only live renders reach
//! history: a preview left on while game text arrives, in one read or
//! split at any byte, leaves the screen the live session leaves once your
//! echo lands, on the native grid. The webview test replays the same
//! payloads into xterm from a stored file.

use super::show_tests::{
    cuts, payload, pinned_streams, profile, rows_of, showing, wire_fixture, Read, Session, CODES,
    HP,
};
use super::*;
use vosh_prompt::overrides::{Overrides, Preview, PromptPreview};
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
    session.p.prompt.set_preview(preview.cloned());
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
    let mut grid = crate::term_grid::TermGrid::new(columns, 60);
    for out in outputs {
        grid.session_output(out);
    }
    grid.local_write(b"look\r\n");
    rows_of(&grid)
}

/// The native grid's screen, `columns` wide, after `outputs`.
fn screen<'a>(outputs: impl IntoIterator<Item = &'a Output>, columns: usize) -> Vec<String> {
    let mut grid = crate::term_grid::TermGrid::new(columns, 60);
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
    session.p.prompt.set_preview(Some(low_health()));
    let read = session.read(&wire_fixture("quiet"));
    // The open row shows the preview, with the live render as its
    // restore.
    let restore = read.out.restore.clone().expect("the live render");
    assert!(String::from_utf8_lossy(&restore).contains("<1020>"));
    let rows = screen([&read.out], 80);
    assert_eq!(rows.last().map(String::as_str), Some("<180>"));
    // Your echo puts the live render back first.
    let rows = screen_after_echo([&read.out], 80);
    assert_eq!(rows.last().map(String::as_str), Some("<1020>look"));
    // The panes keep the live values.
    let vars = session.p.prompt.take_prompt_vars(true).expect("the vars");
    assert_eq!(vars.get("hp").map(String::as_str), Some("1020"));
    // The open row keeps the spans of what it shows.
    let spans = &session.p.prompt.stage.open_row().expect("the row").spans;
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
                let want = replay_with(&mut live, &bytes, at, None);
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
            want.extend(
                vosh_prompt::testkit::reads(&fight, at)
                    .into_iter()
                    .map(|read| live.read(read)),
            );
            let mut got = replay_with(&mut previewing, &quiet, &[], None);
            // The card sets Low health on the open row.
            previewing.p.prompt.set_preview(Some(low_health()));
            let repaint = previewing.repaint();
            match show {
                PromptShow::Pinned => {
                    assert_eq!(
                        repaint.pin.as_deref().map(vosh_ansi::plain_text).as_deref(),
                        Some("<180>"),
                        "{label}"
                    );
                    assert!(repaint.restore.is_none(), "{label}");
                }
                _ => {
                    let replace = repaint.replace.as_ref().expect("the repaint");
                    assert!(
                        vosh_ansi::plain_text(&replace.bytes).contains("<180>"),
                        "{label}"
                    );
                    let restore = repaint.restore.as_ref().expect("the live render");
                    assert!(vosh_ansi::plain_text(restore).contains("<1020>"), "{label}");
                }
            }
            got.push(Read {
                out: repaint,
                log: Vec::new(),
                kept: Vec::new(),
                sends: Vec::new(),
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
    session.p.prompt.set_preview(Some(PromptPreview {
        placeholders: true,
        ..PromptPreview::default()
    }));
    let open = session.repaint();
    let replace = open.replace.as_ref().expect("the repaint");
    assert_eq!(drawn_text(&replace.bytes), "<1020>Opponent");
    assert_eq!(
        drawn_text(open.restore.as_ref().expect("restore")),
        "<1020>"
    );
    // It reads your codes: the row shows the line the game sent.
    session.p.prompt.set_preview(Some(PromptPreview {
        raw: true,
        ..PromptPreview::default()
    }));
    let raw = session.repaint();
    let replace = raw.replace.as_ref().expect("the repaint");
    assert_eq!(
        drawn_text(&replace.bytes),
        "[1020/1020hp 800/800mn 930/930mv]\r\n"
    );
    assert_eq!(drawn_text(raw.restore.as_ref().expect("restore")), "<1020>");
    // A preview with values on top of Fight.
    session.p.prompt.set_preview(Some(PromptPreview {
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
        "<1020>a rat"
    );
    // The card closes.
    session.p.prompt.set_preview(None);
    let closed = session.repaint();
    assert_eq!(
        drawn_text(&closed.replace.as_ref().expect("the repaint").bytes),
        "<1020>"
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
    vosh_ansi::plain_text(after.as_bytes())
}

#[test]
fn a_repaint_in_a_payload_carries_the_restore_to_the_webview() {
    let mut session = Session::new(profile(CODES, HP, true));
    let _ = session.read(&wire_fixture("quiet"));
    session.p.prompt.set_preview(Some(low_health()));
    let json = payload(&session.repaint()).expect("a payload");
    let value: serde_json::Value = serde_json::from_str(&json).expect("json");
    let restore = value["restore"].as_str().expect("the restore");
    let bytes = super::show_tests::base64_decode(restore);
    assert!(String::from_utf8_lossy(&bytes).ends_with("<1020>\x1b[0m"));
}
