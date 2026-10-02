//! Collapse repeated lines, played through the session's own steps.
//!
//! A child of `session`, so it drives the same private steps the socket
//! loop runs, through the driver in `session_show_tests.rs`. Lines the
//! same as the one before them on screen show once with the count before
//! them, on the native grid, wherever the reads split. The log keeps
//! every line, triggers see each one, and the scrollback ring keeps the
//! run once, as the screen shows it. The webview test replays the same
//! payloads into xterm from a stored file.
//!
//! The lines are the game's own, from `fight.c`, `act_info.c` and
//! `update.c` in the server source, with an invented name.

use super::show_tests::{base64_decode, cuts, profile, rows_of, showing, Read, Session, CODES, HP};
use super::*;
use vosh_prompt::stage::Repeat;
use vosh_prompt::testkit::{Build, Mud, Options};
use vosh_prompt::PromptShow;

const DODGE: &str = "You dodge Quenby's attack.";
const PARRY: &str = "You parry Quenby's attack.";
/// Longer than 40 columns once the count stands before it.
const REDIRECT: &str = "You dodge Quenby's attack and redirect the momentum!";
const HUNGRY: &str = "You are hungry.";
const THIRSTY: &str = "You are thirsty.";

/// What the fake Aabahran shows when you log in, before the lines.
const LOGIN: [&str; 4] = [
    "Welcome to the fake Aabahran, Tester.",
    "The Bank of Aabahran",
    "  Marble counters line the hall, and a clerk nods at you.",
    "[Exits: south]",
];

/// A profile that reads the fake Aabahran's prompt, draws `<hp>` where
/// `show` says, collapses repeated lines, and has a Line trigger that
/// asks to send `seen` for each line that names Quenby, so a test can
/// count the lines triggers saw.
fn collapsing(show: PromptShow) -> Profile {
    let mut p = showing(profile(CODES, HP, true), show);
    p.ui.collapse_repeats = true;
    p.triggers
        .set(vosh_trigger::Trigger {
            name: "quenby".into(),
            patterns: vec![vosh_trigger::TriggerPattern {
                pattern: "Quenby".into(),
                enabled: true,
            }],
            priority: 0,
            enabled: true,
            actions: vec![vosh_trigger::TriggerAction::Send {
                template: "seen".into(),
            }],
            preset: None,
            group: None,
            target: vosh_trigger::TriggerTarget::Line,
        })
        .expect("the trigger compiles");
    p
}

/// What the fake Aabahran writes after you log in, one pulse per entry
/// of `pulses`, each line of an entry in the same pulse. `compact` leaves
/// out the blank line before each prompt. Returns the login and the
/// pulses.
fn played(compact: bool, pulses: &[&[&str]]) -> (Vec<u8>, Vec<u8>) {
    let mut mud = Mud::playing(Options {
        compact,
        ..Options::new(Build::New)
    });
    let login = mud.login();
    let mut bytes = Vec::new();
    for lines in pulses {
        bytes.extend(mud.pulse_later(&lines.join("\n\r")));
    }
    (login, bytes)
}

/// A fight with a compact prompt: you attack the guard, then a round for
/// each entry of `rounds`, which brings that line before the battle line,
/// with the tank line your prompt prints before the prompt. Returns the
/// login and the fight.
fn battle(rounds: &[&str]) -> (Vec<u8>, Vec<u8>) {
    let mut mud = Mud::playing(Options {
        compact: true,
        ..Options::new(Build::New)
    });
    let login = mud.login();
    let mut bytes = Vec::new();
    for write in mud.command("fight") {
        bytes.extend(write.bytes);
    }
    for round in rounds {
        bytes.extend(mud.pulse_later(round));
    }
    (login, bytes)
}

/// The streams the screens are checked on: pulses with a compact prompt,
/// the same pulses with the blank line before each prompt, a fight whose
/// battle line comes round after round, and lines with no prompt at all.
fn streams() -> Vec<(&'static str, Vec<u8>, Vec<u8>)> {
    let pulses: &[&[&str]] = &[
        &[DODGE],
        &[DODGE],
        &[DODGE, DODGE],
        &[REDIRECT],
        &[REDIRECT, REDIRECT],
        &[PARRY],
    ];
    let (login, compact) = played(true, pulses);
    let (spaced_login, spaced) = played(false, pulses);
    let (fight_login, fight) = battle(&["", "", DODGE]);
    let ticks = format!("{HUNGRY}\n\r{HUNGRY}\n\r{THIRSTY}\n\r{THIRSTY}\n\r{THIRSTY}\n\r");
    vec![
        ("compact", login, compact),
        ("spaced", spaced_login, spaced),
        ("fight", fight_login, fight),
        ("no-prompt", Vec::new(), ticks.into_bytes()),
    ]
}

/// `bytes` read on a new connection of `session` after `login` in one
/// read, cut at `at`.
fn replay(session: &mut Session, login: &[u8], bytes: &[u8], at: &[usize]) -> Vec<Read> {
    session.restart();
    let mut reads = Vec::new();
    if !login.is_empty() {
        reads.push(session.read(login));
    }
    reads.extend(
        vosh_prompt::testkit::reads(bytes, at)
            .into_iter()
            .map(|read| session.read(read)),
    );
    reads
}

/// The rows a native grid `columns` wide shows after `reads`, trimmed.
fn grid_rows(reads: &[Read], columns: usize) -> Vec<String> {
    let mut grid = crate::term_grid::TermGrid::new(columns, 60);
    for read in reads {
        grid.session_output(&read.out);
    }
    rows_of(&grid)
}

/// The lines the scrollback ring keeps after `reads`, as the session
/// keeps them, plain, the blank ones left out.
fn ring_of(reads: &[Read]) -> Vec<String> {
    let mut ring = crate::log_state::Scrollback::default();
    for read in reads {
        for (line, repeat) in read.kept.iter().zip(&read.repeats) {
            ring.keep(line.clone(), *repeat);
        }
    }
    ring.lines()
        .map(vosh_ansi::plain_text)
        .filter(|line| !line.is_empty())
        .collect()
}

/// `line` as a run of `count` shows it, plain.
fn times(count: u32, line: &str) -> String {
    vosh_ansi::plain_text(&vosh_prompt::stage::counted(count, line.as_bytes()))
}

fn rows(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_string()).collect()
}

#[test]
fn repeated_lines_show_once_with_their_count() {
    let streams = streams();
    let screen = |name: &str, show: PromptShow| {
        let (_, login, bytes) = streams.iter().find(|(n, _, _)| *n == name).expect("it");
        let mut session = Session::new(collapsing(show));
        grid_rows(&replay(&mut session, login, bytes, &[]), 80)
    };
    // Pinned with a compact prompt, the pulses follow each other, so a run
    // goes on from one to the next.
    let mut want = rows(&LOGIN);
    want.extend([times(4, DODGE), times(3, REDIRECT), PARRY.to_string()]);
    assert_eq!(screen("compact", PromptShow::Pinned), want);
    // The blank line before each prompt shows, so only the lines of one
    // pulse collapse.
    let mut want = rows(&LOGIN);
    want.extend(
        ["", DODGE, "", DODGE, ""]
            .map(str::to_string)
            .into_iter()
            .chain([times(2, DODGE), String::new(), REDIRECT.to_string()])
            .chain([String::new(), times(2, REDIRECT), String::new()])
            .chain([PARRY.to_string()]),
    );
    assert_eq!(screen("spaced", PromptShow::Pinned), want);
    // A prompt in the text ends each run.
    let text = screen("compact", PromptShow::Text);
    assert_eq!(
        text.iter()
            .filter(|row| !row.starts_with('<'))
            .cloned()
            .collect::<Vec<_>>()[LOGIN.len()..],
        [
            DODGE.to_string(),
            DODGE.to_string(),
            times(2, DODGE),
            REDIRECT.to_string(),
            times(2, REDIRECT),
            PARRY.to_string(),
        ]
    );
    // In a fight the battle line comes round after round, and the tank
    // line leaves the text with your prompt.
    let battle = "A Blackwatch guard has quite a few wounds.";
    let mut want = rows(&LOGIN);
    want.extend([
        "A Blackwatch guard attacks you!".to_string(),
        times(3, battle),
        DODGE.to_string(),
        battle.to_string(),
    ]);
    assert_eq!(screen("fight", PromptShow::Pinned), want);
    assert_eq!(
        screen("no-prompt", PromptShow::Pinned),
        [times(2, HUNGRY), times(3, THIRSTY)]
    );
}

#[test]
fn repeated_lines_show_the_same_at_every_split_on_the_native_grid() {
    for (name, login, bytes) in streams() {
        for show in [PromptShow::Pinned, PromptShow::Text] {
            let mut session = Session::new(collapsing(show));
            for columns in [40, 12] {
                let whole = grid_rows(&replay(&mut session, &login, &bytes, &[]), columns);
                for at in cuts(&bytes) {
                    let split = grid_rows(&replay(&mut session, &login, &bytes, &[at]), columns);
                    assert_eq!(
                        split, whole,
                        "{name} {show:?} {columns} wide, cut after {at}"
                    );
                }
            }
        }
    }
}

#[test]
fn every_line_is_logged_and_triggers_see_each_one() {
    for (name, login, bytes) in streams() {
        let sent = String::from_utf8_lossy(&bytes);
        let quenby = sent.matches("Quenby").count();
        let mut session = Session::new(collapsing(PromptShow::Pinned));
        let mut splits = vec![Vec::new()];
        splits.extend(cuts(&bytes).into_iter().map(|at| vec![at]));
        for at in &splits {
            let reads = replay(&mut session, &login, &bytes, at);
            let log: Vec<&String> = reads.iter().flat_map(|r| &r.log).collect();
            for line in [DODGE, REDIRECT, PARRY, HUNGRY, THIRSTY] {
                // Each pulse and each tick line names the line on its own.
                let want = sent.split("\n\r").filter(|l| *l == line).count();
                let got = log.iter().filter(|row| row.as_str() == line).count();
                assert_eq!(got, want, "{name} cut {at:?}: {line}");
            }
            let seen = reads
                .iter()
                .flat_map(|r| &r.sends)
                .filter(|send| send.as_str() == "seen")
                .count();
            assert_eq!(seen, quenby, "{name} cut {at:?}");
        }
    }
}

#[test]
fn the_scrollback_ring_keeps_each_run_once() {
    let streams = streams();
    let (_, login, bytes) = &streams[0];
    for show in [PromptShow::Pinned, PromptShow::Text] {
        let mut session = Session::new(collapsing(show));
        let mut splits = vec![Vec::new()];
        splits.extend(cuts(bytes).into_iter().map(|at| vec![at]));
        for at in &splits {
            let reads = replay(&mut session, login, bytes, at);
            let mut want = rows(&LOGIN);
            match show {
                PromptShow::Pinned => {
                    want.extend([times(4, DODGE), times(3, REDIRECT), PARRY.to_string()]);
                }
                _ => want.extend([
                    DODGE.to_string(),
                    DODGE.to_string(),
                    times(2, DODGE),
                    REDIRECT.to_string(),
                    times(2, REDIRECT),
                    PARRY.to_string(),
                ]),
            }
            assert_eq!(ring_of(&reads), want, "{show:?} cut {at:?}");
        }
    }
}

#[test]
fn your_echo_output_from_elsewhere_and_a_new_connection_start_a_new_run() {
    let mut mud = Mud::playing(Options {
        compact: true,
        ..Options::new(Build::New)
    });
    let mut session = Session::new(collapsing(PromptShow::Pinned));
    let mut grid = crate::term_grid::TermGrid::new(60, 30);
    grid.session_output(&session.read(&mud.login()).out);
    grid.session_output(&session.read(&mud.pulse_later(DODGE)).out);
    // You type `wake`: your echo lands after the run.
    let _ = session.send("wake");
    grid.local_write(b"wake\r\n");
    session.local_write();
    grid.session_output(&session.read(&mud.pulse_later(DODGE)).out);
    grid.session_output(&session.read(&mud.pulse_later(DODGE)).out);
    // A slash command's reply.
    grid.session_output(&session.emitted(b"\r\n[logging on]\r\n"));
    grid.session_output(&session.read(&mud.pulse_later(DODGE)).out);
    let mut want = rows(&LOGIN);
    want.extend([
        DODGE.to_string(),
        "wake".to_string(),
        // The game starts its unasked output with a line end, which shows
        // once your echo took the row your prompt held.
        String::new(),
        times(2, DODGE),
        "[logging on]".to_string(),
        String::new(),
        DODGE.to_string(),
    ]);
    assert_eq!(rows_of(&grid), want);
    // With no prompt and no blank line between them, your echo and a
    // reply from elsewhere still keep the lines on either side apart.
    let mut session = Session::new(collapsing(PromptShow::Pinned));
    let mut grid = crate::term_grid::TermGrid::new(60, 30);
    let hungry = format!("{HUNGRY}\n\r");
    grid.session_output(&session.read(hungry.as_bytes()).out);
    let _ = session.send("eat");
    grid.local_write(b"eat\r\n");
    session.local_write();
    grid.session_output(&session.read(hungry.as_bytes()).out);
    grid.session_output(&session.emitted(b"[logging on]\r\n"));
    grid.session_output(&session.read(hungry.as_bytes()).out);
    grid.session_output(&session.read(hungry.as_bytes()).out);
    assert_eq!(
        rows_of(&grid),
        [
            HUNGRY.to_string(),
            "eat".to_string(),
            HUNGRY.to_string(),
            "[logging on]".to_string(),
            times(2, HUNGRY),
        ]
    );
    // A new connection starts over.
    session.restart();
    let read = session.read(&mud.pulse_later(DODGE));
    assert_eq!(read.repeats, [None, Some(Repeat::Starts)]);
}

#[test]
fn a_hidden_line_leaves_the_run_and_another_color_starts_a_new_one() {
    let mut p = collapsing(PromptShow::Pinned);
    p.triggers
        .set(vosh_trigger::Trigger {
            name: "thirst".into(),
            patterns: vec![vosh_trigger::TriggerPattern {
                pattern: "thirsty".into(),
                enabled: true,
            }],
            priority: 0,
            enabled: true,
            actions: vec![vosh_trigger::TriggerAction::Gag],
            preset: None,
            group: None,
            target: vosh_trigger::TriggerTarget::Line,
        })
        .expect("the trigger compiles");
    let mut session = Session::new(p);
    let read = session.read(
        format!("{HUNGRY}\n\r{THIRSTY}\n\r{HUNGRY}\n\r\x1b[1;33m{HUNGRY}\x1b[0m\n\r").as_bytes(),
    );
    assert_eq!(
        grid_rows(std::slice::from_ref(&read), 80),
        [times(2, HUNGRY), HUNGRY.to_string()]
    );
    // The hidden line is neither logged nor kept, as before.
    assert_eq!(read.log, [HUNGRY, HUNGRY, HUNGRY]);
    assert_eq!(
        read.repeats,
        [
            Some(Repeat::Starts),
            Some(Repeat::Joins(2)),
            Some(Repeat::Starts)
        ]
    );
}

#[test]
fn with_collapse_off_every_line_shows_as_before() {
    let mut p = collapsing(PromptShow::Pinned);
    p.ui.collapse_repeats = false;
    let mut session = Session::new(p);
    let read = session.read(format!("{HUNGRY}\n\r{HUNGRY}\n\r").as_bytes());
    assert_eq!(read.out.bytes, b"You are hungry.\r\nYou are hungry.\r\n");
    assert_eq!(read.repeats, [None, None]);
}

/// The rows a grid shows at its display offset, trimmed.
fn in_view(grid: &crate::term_grid::TermGrid) -> Vec<String> {
    (0..grid.screen_lines())
        .map(|line| {
            let row: String = (0..grid.columns())
                .map(|col| grid.cell(line, col).0)
                .collect();
            row.trim_end().to_string()
        })
        .collect()
}

#[test]
fn a_run_goes_on_at_the_live_tail_while_you_read_back() {
    let mut session = Session::new(collapsing(PromptShow::Pinned));
    let mut grid = crate::term_grid::TermGrid::new(40, 10);
    // Lines that take turns, so none of them collapse.
    let history = format!("{HUNGRY}\n\r{THIRSTY}\n\r").repeat(15);
    grid.session_output(&session.read(history.as_bytes()).out);
    grid.session_output(&session.read(format!("{DODGE}\n\r").as_bytes()).out);
    grid.scroll(12);
    let reading = in_view(&grid);
    let offset = grid.display_offset();
    for _ in 0..3 {
        grid.session_output(&session.read(format!("{DODGE}\n\r").as_bytes()).out);
    }
    // What you read stays where it was, and the live tail shows the run.
    assert_eq!(in_view(&grid), reading);
    assert_eq!(grid.display_offset(), offset);
    grid.scroll(-(i32::try_from(offset).expect("small")));
    let live = rows_of(&grid);
    assert_eq!(live.last(), Some(&times(4, DODGE)));
    assert_eq!(live[live.len() - 2], THIRSTY);
}

#[test]
fn a_run_longer_than_the_width_rewrites_every_row_it_takes() {
    let mut session = Session::new(collapsing(PromptShow::Pinned));
    let mut grid = crate::term_grid::TermGrid::new(12, 20);
    let mut reads = Vec::new();
    for _ in 0..10 {
        let read = session.read(format!("{REDIRECT}\n\r").as_bytes());
        grid.session_output(&read.out);
        reads.push(read);
    }
    grid.session_output(&session.read(format!("{PARRY}\n\r").as_bytes()).out);
    let shown = rows_of(&grid).join(" ");
    let squeezed: String = shown.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(squeezed, format!("{} {PARRY}", times(10, REDIRECT)));
}

/// The payloads of every stream in [`streams`], drawing on, pinned, and
/// the compact pulses in the text too, as one read and as two cut at
/// every place [`cuts`] names, each after the payload of the login in one
/// read, with the native grid's screen of one read at 40 and 12 wide. The
/// webview test replays them into xterm and holds its screens to the
/// grid's. The native grid test plays every stream both ways.
fn collapse_splits() -> serde_json::Value {
    let payloads = |reads: &[Read]| -> Vec<serde_json::Value> {
        reads
            .iter()
            .filter(|read| !read.out.is_empty())
            .map(|read| {
                serde_json::to_value(OutputPayload::from_output(&read.out)).expect("it serializes")
            })
            .collect()
    };
    let mut streams = Vec::new();
    for (name, login, bytes) in self::streams() {
        let shows: &[PromptShow] = if name == "compact" {
            &[PromptShow::Pinned, PromptShow::Text]
        } else {
            &[PromptShow::Pinned]
        };
        for &show in shows {
            let mut session = Session::new(collapsing(show));
            let mut splits = vec![Vec::new()];
            splits.extend(cuts(&bytes).into_iter().map(|at| vec![at]));
            // The login reads the same before every split, so it is stored
            // once.
            let lead = usize::from(!login.is_empty());
            let splits: Vec<Vec<serde_json::Value>> = splits
                .iter()
                .map(|at| payloads(&replay(&mut session, &login, &bytes, at)[lead..]))
                .collect();
            let whole = replay(&mut session, &login, &bytes, &[]);
            let screens: serde_json::Map<String, serde_json::Value> = [40, 12]
                .into_iter()
                .map(|columns| {
                    (
                        columns.to_string(),
                        serde_json::json!(grid_rows(&whole, columns)),
                    )
                })
                .collect();
            streams.push(serde_json::json!({
                "name": name,
                "show": show,
                "screens": screens,
                "login": payloads(&whole[..lead]),
                "splits": splits,
            }));
        }
    }
    serde_json::json!({ "streams": streams })
}

/// The file the webview test reads: the JSON of [`collapse_splits`],
/// gzipped, as base64 text, since the webview test can import text only.
fn collapse_splits_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/collapse/splits.b64")
}

/// Write [`collapse_splits`] for the webview test when
/// `VOSH_WRITE_COLLAPSE_SPLITS` is set. Nothing otherwise.
#[test]
fn write_the_collapse_splits_for_the_webview() {
    use std::io::Write as _;
    if std::env::var("VOSH_WRITE_COLLAPSE_SPLITS").is_err() {
        return;
    }
    let text = serde_json::to_string(&collapse_splits()).expect("json");
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(text.as_bytes()).expect("gzip");
    let encoded = base64_encode(&gz.finish().expect("gzip"));
    let mut lines: Vec<&str> = encoded
        .as_bytes()
        .chunks(100)
        .map(|c| std::str::from_utf8(c).expect("ascii"))
        .collect();
    lines.push("");
    let path = collapse_splits_path();
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
    std::fs::write(&path, lines.join("\n")).expect("the file");
}

#[test]
fn the_collapse_splits_the_webview_replays_are_what_the_session_sends() {
    use std::io::Read as _;
    let stored = std::fs::read_to_string(collapse_splits_path())
        .expect("fixtures/collapse/splits.b64, written with VOSH_WRITE_COLLAPSE_SPLITS=1");
    let bytes = base64_decode(&stored);
    let mut text = String::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_string(&mut text)
        .expect("gzip");
    let stored: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert!(
        stored == collapse_splits(),
        "the session changed, so write the file again with VOSH_WRITE_COLLAPSE_SPLITS=1"
    );
}
