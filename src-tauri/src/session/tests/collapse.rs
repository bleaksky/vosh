//! Collapse repeated lines, played through the session's own steps.
//!
//! Inside `session`, so it drives the same private steps the socket
//! loop runs, through the harness's `Session` driver. Lines the same as
//! the one before them on screen show once with the count before them,
//! on the native grid, wherever the reads split. The log keeps
//! every line, triggers see each one, and the scrollback ring keeps the
//! run once, as the screen shows it. The webview test replays the same
//! payloads into xterm from a stored file.
//!
//! The lines are the game's own, from `fight.c`, `act_info.c` and
//! `update.c` in the server source, with an invented name.

use super::*;
use crate::output::{base64_encode, OutputPayload};
use vosh_prompt::stage::Repeat;
use vosh_prompt::testkit::{Build, Mud, Options, TickOrder};
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
fn collapsing(show: PromptShow) -> Live {
    let (mut p, c) = showing(profile(CODES, HP, true), show);
    p.ui.collapse_repeats = true;
    p.triggers
        .set(vosh_automation::trigger::Trigger::new(
            "quenby",
            "Quenby",
            vosh_automation::trigger::TriggerAction::Send {
                template: "seen".into(),
            },
        ))
        .expect("the trigger compiles");
    (p, c)
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
    let mut grid = crate::native::grid::TermGrid::new(columns, 60);
    for read in reads {
        grid.session_output(&read.out);
    }
    rows_of(&grid)
}

/// The scrollback ring after `reads`, kept as the session keeps it.
fn ring_after(reads: &[Read]) -> crate::logs::Scrollback {
    let mut ring = crate::logs::Scrollback::default();
    for read in reads {
        for (line, repeat) in read.kept.iter().zip(&read.repeats) {
            ring.keep(line.clone(), *repeat);
        }
    }
    ring
}

/// The lines the scrollback ring keeps after `reads`, as the session
/// keeps them, plain, blank ones included.
fn ring_of(reads: &[Read]) -> Vec<String> {
    ring_after(reads)
        .lines()
        .map(vosh_protocol::ansi::plain_text)
        .collect()
}

/// What Collapse repeated lines made of each line `read` kept.
fn made(read: &Read) -> Vec<Option<Repeat>> {
    read.repeats
        .iter()
        .map(|run| run.map(|run| run.repeat))
        .collect()
}

/// `line` as a run of `count` shows it, plain.
fn times(count: u32, line: &str) -> String {
    vosh_protocol::ansi::plain_text(&vosh_prompt::stage::counted(
        count,
        line.as_bytes(),
        &vosh_prompt::render::SgrState::default(),
    ))
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

/// A profile like [`collapsing`] that pins your prompt with drawing off,
/// so the ring keeps each prompt as the game sent it.
fn collapsing_undrawn() -> Live {
    let (mut p, c) = showing(profile(CODES, HP, false), PromptShow::Pinned);
    p.ui.collapse_repeats = true;
    (p, c)
}

#[test]
fn the_scrollback_ring_keeps_each_run_once_in_the_order_it_came() {
    let streams = streams();
    let stream = |name: &str| {
        streams
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, login, bytes)| (login.clone(), bytes.clone()))
            .expect("the stream")
    };
    let prompt = "[1020/1020hp 800/800mn 930/930mv]";
    let hurt = "[765/1020hp 800/800mn 930/930mv]";
    let tank = "Tester: [===|===|===|---]";
    let battle = "A Blackwatch guard has quite a few wounds. ";
    let cases: Vec<(&str, Live, &str, Vec<String>)> = vec![
        // Pinned, the ring keeps what the screen shows, and the line end
        // each pinned prompt's row took stays out of it.
        (
            "compact pinned",
            collapsing(PromptShow::Pinned),
            "compact",
            [times(4, DODGE), times(3, REDIRECT), PARRY.to_string()].to_vec(),
        ),
        // In the text, the line end after each prompt shows, as before.
        (
            "compact in the text",
            collapsing(PromptShow::Text),
            "compact",
            [
                "",
                DODGE,
                "",
                DODGE,
                "",
                &times(2, DODGE),
                "",
                REDIRECT,
                "",
                &times(2, REDIRECT),
                "",
                PARRY,
            ]
            .map(str::to_string)
            .to_vec(),
        ),
        // Each round's tank line leaves the text with your prompt, and the
        // ring keeps it as before. The battle line that comes round after
        // it comes after it in the ring too.
        (
            "fight pinned",
            collapsing(PromptShow::Pinned),
            "fight",
            [
                "A Blackwatch guard attacks you!",
                tank,
                tank,
                &times(3, battle),
                tank,
                DODGE,
                battle,
                tank,
            ]
            .map(str::to_string)
            .to_vec(),
        ),
        // Drawing off, the ring keeps each pinned prompt as the game sent
        // it, and the run comes after the prompts it went on past.
        (
            "compact pinned, drawing off",
            collapsing_undrawn(),
            "compact",
            [
                prompt,
                prompt,
                prompt,
                &times(4, DODGE),
                prompt,
                prompt,
                &times(3, REDIRECT),
                prompt,
                PARRY,
                prompt,
            ]
            .map(str::to_string)
            .to_vec(),
        ),
        (
            "fight pinned, drawing off",
            collapsing_undrawn(),
            "fight",
            [
                prompt,
                "A Blackwatch guard attacks you!",
                tank,
                hurt,
                tank,
                hurt,
                &times(3, battle),
                tank,
                hurt,
                DODGE,
                battle,
                tank,
                hurt,
            ]
            .map(str::to_string)
            .to_vec(),
        ),
    ];
    for (label, profile, name, lines) in cases {
        let (login, bytes) = stream(name);
        let mut session = Session::new(profile);
        let mut want = rows(&LOGIN);
        want.extend(lines);
        let mut splits = vec![Vec::new()];
        splits.extend(cuts(&bytes).into_iter().map(|at| vec![at]));
        for at in &splits {
            let reads = replay(&mut session, &login, &bytes, at);
            assert_eq!(ring_of(&reads), want, "{label}, cut {at:?}");
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
    let mut grid = crate::native::grid::TermGrid::new(60, 30);
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
    let mut grid = crate::native::grid::TermGrid::new(60, 30);
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
    assert_eq!(made(&read), [None, Some(Repeat::Starts)]);
}

#[test]
fn a_hidden_line_leaves_the_run_and_another_color_starts_a_new_one() {
    let (mut p, c) = collapsing(PromptShow::Pinned);
    p.triggers
        .set(vosh_automation::trigger::Trigger::new(
            "thirst",
            "thirsty",
            vosh_automation::trigger::TriggerAction::Gag,
        ))
        .expect("the trigger compiles");
    let mut session = Session::new((p, c));
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
        made(&read),
        [
            Some(Repeat::Starts),
            Some(Repeat::Joins(2)),
            Some(Repeat::Starts)
        ]
    );
}

#[test]
fn with_collapse_off_every_line_shows_as_before() {
    let (mut p, c) = collapsing(PromptShow::Pinned);
    p.ui.collapse_repeats = false;
    let mut session = Session::new((p, c));
    let read = session.read(format!("{HUNGRY}\n\r{HUNGRY}\n\r").as_bytes());
    assert_eq!(read.out.bytes, b"You are hungry.\r\nYou are hungry.\r\n");
    assert_eq!(made(&read), [None, None]);
}

/// The rows a grid shows at its display offset, trimmed.
fn in_view(grid: &crate::native::grid::TermGrid) -> Vec<String> {
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
    let mut grid = crate::native::grid::TermGrid::new(40, 10);
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
    let mut grid = crate::native::grid::TermGrid::new(12, 20);
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

/// A trigger named `name` that highlights `pattern` in `style`.
fn highlight(
    name: &str,
    pattern: &str,
    style: vosh_automation::trigger::HighlightStyle,
) -> vosh_automation::trigger::Trigger {
    vosh_automation::trigger::Trigger::new(
        name,
        pattern,
        vosh_automation::trigger::TriggerAction::Highlight { style },
    )
}

/// The screen row that shows `text`, trimmed.
fn row_of(grid: &crate::native::grid::TermGrid, text: &str) -> usize {
    rows_of(grid)
        .iter()
        .position(|row| row == text)
        .unwrap_or_else(|| panic!("{text:?} in {:?}", rows_of(grid)))
}

/// What the session writes, your prompt pinned, while a Line trigger
/// highlights `pattern` in `style`: the login of the fake Aabahran with a
/// compact prompt, then a pulse for each of `lines`.
fn highlighted(
    pattern: &str,
    style: vosh_automation::trigger::HighlightStyle,
    lines: &[&str],
) -> Vec<Output> {
    let (mut p, c) = collapsing(PromptShow::Pinned);
    p.triggers
        .set(highlight("mark", pattern, style))
        .expect("the trigger compiles");
    let mut session = Session::new((p, c));
    let mut mud = Mud::playing(Options {
        compact: true,
        ..Options::new(Build::New)
    });
    let mut outputs = vec![session.read(&mud.login()).out];
    for line in lines {
        outputs.push(session.read(&mud.pulse_later(line)).out);
    }
    outputs
}

/// A run whose line ends on a blue background, across pinned pulses.
fn blue_run() -> Vec<Output> {
    let style = vosh_automation::trigger::HighlightStyle {
        bg: Some(vosh_automation::trigger::NamedColor::Blue),
        ..vosh_automation::trigger::HighlightStyle::default()
    };
    highlighted(r"attack\.", style, &[DODGE, DODGE, PARRY])
}

/// A run of a line a red wash covers, across pinned pulses.
fn washed_run() -> Vec<Output> {
    let style = vosh_automation::trigger::HighlightStyle {
        fg: Some(vosh_automation::trigger::NamedColor::Red),
        wash: true,
        ..vosh_automation::trigger::HighlightStyle::default()
    };
    highlighted("You dodge", style, &[DODGE, DODGE, DODGE, PARRY])
}

/// A grid `columns` wide and 20 rows tall after `outputs`.
fn grid_after(outputs: &[Output], columns: usize) -> crate::native::grid::TermGrid {
    let mut grid = crate::native::grid::TermGrid::new(columns, 20);
    for out in outputs {
        grid.session_output(out);
    }
    grid
}

#[test]
fn a_run_with_a_background_crossing_a_pinned_pulse_leaves_every_other_cell_plain() {
    use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
    let plain = Color::Named(NamedColor::Background);
    let blue = Color::Named(NamedColor::Blue);
    // A highlight with a background at the end of the line.
    let grid = grid_after(&blue_run(), 40);
    let run = row_of(&grid, &times(2, DODGE));
    assert_eq!(row_of(&grid, PARRY), run + 1);
    let width = times(2, DODGE).len();
    for col in 0..40 {
        let want = if (width - 7..width).contains(&col) {
            blue
        } else {
            plain
        };
        assert_eq!(grid.cell(run, col).2, want, "the run's row, column {col}");
    }
    for col in 0..40 {
        let want = if (PARRY.len() - 7..PARRY.len()).contains(&col) {
            blue
        } else {
            plain
        };
        assert_eq!(
            grid.cell(run + 1, col).2,
            want,
            "the parry row, column {col}"
        );
    }
    for line in run + 2..20 {
        for col in 0..40 {
            assert_eq!(grid.cell(line, col).2, plain, "row {line}, column {col}");
        }
    }
    // A wash: the count stands on it too, so the native renderer still
    // reads the row as washed from its first cell, and the rows below
    // stay plain.
    let grid = grid_after(&washed_run(), 40);
    let (r, g, b) = vosh_automation::trigger::NamedColor::Red.wash_tint();
    let wash = Color::Spec(Rgb { r, g, b });
    let run = row_of(&grid, &times(3, DODGE));
    assert_eq!(
        grid.cell(run, 0),
        ('(', Color::Indexed(244), wash),
        "the count stands on the wash"
    );
    assert_eq!(grid.cell(run, 4).2, wash);
    assert_eq!(grid.cell(run, 4).1, Color::Named(NamedColor::Red));
    for col in times(3, DODGE).len()..40 {
        assert_eq!(grid.cell(run, col).2, plain, "the run's row, column {col}");
    }
    for line in run + 1..20 {
        for col in 0..40 {
            assert_eq!(grid.cell(line, col).2, plain, "row {line}, column {col}");
        }
    }
}

#[test]
fn a_line_that_relies_on_the_color_before_it_keeps_it_with_its_count() {
    use alacritty_terminal::vte::ansi::{Color, NamedColor};
    let green = Color::Named(NamedColor::Green);
    let text = format!("\x1b[32m{HUNGRY}\n\r{HUNGRY}\n\r{HUNGRY}\n\r{THIRSTY}\x1b[0m\n\r");
    for collapse in [false, true] {
        let (mut p, c) = collapsing(PromptShow::Pinned);
        p.ui.collapse_repeats = collapse;
        let mut session = Session::new((p, c));
        let read = session.read(text.as_bytes());
        let mut grid = crate::native::grid::TermGrid::new(40, 10);
        grid.session_output(&read.out);
        let rows = rows_of(&grid);
        for (line, row) in rows.iter().enumerate() {
            let from = usize::from(row.starts_with('('));
            for col in from * 4..row.len() {
                assert_eq!(
                    grid.cell(line, col).1,
                    green,
                    "collapse {collapse}, {row:?} column {col}"
                );
            }
        }
        if collapse {
            assert_eq!(
                rows,
                [HUNGRY.to_string(), times(2, HUNGRY), THIRSTY.to_string()]
            );
            assert_eq!(grid.cell(1, 0).1, Color::Indexed(244));
        } else {
            assert_eq!(rows, [HUNGRY, HUNGRY, HUNGRY, THIRSTY]);
        }
    }
}

/// Your echo reaches the screen after one pulse of a red run, and the
/// session builds the next pulse before it hears of it, so that pulse goes
/// on with the run. Returns the payloads in order, how many come before
/// your echo, and the echo.
fn echo_before_the_run() -> (Vec<Output>, usize, &'static [u8]) {
    let red = format!("\x1b[1;31m{DODGE}\x1b[0m");
    let mut mud = Mud::playing(Options {
        compact: true,
        ..Options::new(Build::New)
    });
    let mut session = Session::new(collapsing(PromptShow::Pinned));
    let login = session.read(&mud.login());
    let first = session.read(&mud.pulse_later(&red));
    let second = session.read(&mud.pulse_later(&red));
    // The line end your pinned prompt's row took is not kept.
    assert_eq!(made(&second), [Some(Repeat::Joins(2))]);
    // The session hears of your echo now, after the screen had the first
    // pulse only, so the run the second pulse went on with stays.
    session.c.prompt.stage.local_write(first.out.id());
    let parry = session.read(&mud.pulse_later(PARRY));
    (
        vec![login.out, first.out, second.out, parry.out],
        2,
        b"kill guard\r\n",
    )
}

#[test]
fn a_run_your_echo_landed_before_goes_on_a_new_row_that_still_ends() {
    use alacritty_terminal::vte::ansi::{Color, NamedColor};
    let (outputs, before, echo) = echo_before_the_run();
    let mut grid = crate::native::grid::TermGrid::new(60, 20);
    for out in &outputs[..before] {
        grid.session_output(out);
    }
    grid.local_write(echo);
    for out in &outputs[before..] {
        grid.session_output(out);
    }
    let mut want = rows(&LOGIN);
    want.extend([
        DODGE.to_string(),
        "kill guard".to_string(),
        times(2, DODGE),
        PARRY.to_string(),
    ]);
    assert_eq!(rows_of(&grid), want);
    let parry = row_of(&grid, PARRY);
    assert_eq!(grid.cell(parry, 0).1, Color::Named(NamedColor::Foreground));
}

/// A pane that opens during a run of three dodges, such as the history
/// of the split, and loads the scrollback the session kept so far, then
/// two more dodges and a parry. Returns what the session wrote before the
/// pane opened, what the pane loads, and what the session wrote after.
fn pane_during_a_run() -> (Vec<Output>, Vec<u8>, Vec<Output>) {
    let mut mud = Mud::playing(Options {
        compact: true,
        ..Options::new(Build::New)
    });
    let mut session = Session::new(collapsing(PromptShow::Pinned));
    let mut reads = vec![session.read(&mud.login())];
    for _ in 0..3 {
        reads.push(session.read(&mud.pulse_later(DODGE)));
    }
    let load = ring_after(&reads).dump_live();
    let after = [DODGE, DODGE, PARRY]
        .iter()
        .map(|line| session.read(&mud.pulse_later(line)).out)
        .collect();
    (
        reads.into_iter().map(|read| read.out).collect(),
        load,
        after,
    )
}

#[test]
fn a_pane_that_loads_the_scrollback_during_a_run_goes_on_with_it_in_place() {
    let (before, load, after) = pane_during_a_run();
    let mut live = crate::native::grid::TermGrid::new(60, 20);
    for out in before.iter().chain(&after) {
        live.session_output(out);
    }
    // The pane takes every byte it loads as xterm does, marks included.
    let mut pane = crate::native::grid::TermGrid::new(60, 20);
    let mut loaded = Output::new(false);
    loaded.text(&load);
    pane.session_output(&loaded);
    for out in &after {
        pane.session_output(out);
    }
    let mut want = rows(&LOGIN);
    want.extend([times(5, DODGE), PARRY.to_string()]);
    assert_eq!(rows_of(&live), want);
    assert_eq!(rows_of(&pane), want);
}

/// The payloads of every stream in [`streams`], drawing on, pinned, and
/// the compact pulses in the text too, as one read and as two cut at
/// every place [`cuts`] names, each after the payload of the login in one
/// read, with the native grid's screen of one read at 40 and 12 wide. The
/// webview test replays them into xterm and holds its screens to the
/// grid's. The native grid test plays every stream both ways. Then the
/// scenes the tests above play on the native grid: a run whose line ends
/// on a blue background and one a wash covers, across pinned pulses, your
/// echo landing before the session heard of it, and a pane that loads
/// the scrollback during a run.
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
    // Scenes the stream replays leave out, each with the native grid's
    // screen: runs whose line ends on a background across pinned pulses,
    // and your echo landing before the session heard of it.
    let scene = |name: &str,
                 load: Option<&[u8]>,
                 outputs: &[Output],
                 echo: Option<(usize, &[u8])>,
                 columns: usize| {
        let mut grid = crate::native::grid::TermGrid::new(columns, 20);
        if let Some(load) = load {
            let mut loaded = Output::new(false);
            loaded.text(load);
            grid.session_output(&loaded);
        }
        for (i, out) in outputs.iter().enumerate() {
            if let Some((_, echo)) = echo.filter(|(before, _)| *before == i) {
                grid.local_write(echo);
            }
            grid.session_output(out);
        }
        let payloads: Vec<serde_json::Value> = outputs
            .iter()
            .map(|out| {
                serde_json::to_value(OutputPayload::from_output(out)).expect("it serializes")
            })
            .collect();
        serde_json::json!({
            "name": name,
            "cols": columns,
            "load": load.map(|load| String::from_utf8_lossy(load).into_owned()),
            "payloads": payloads,
            "before": echo.map(|(before, _)| before),
            "echo": echo.map(|(_, echo)| String::from_utf8_lossy(echo).into_owned()),
            "screen": rows_of(&grid),
        })
    };
    let (outputs, before, echo) = echo_before_the_run();
    let (_, load, after) = pane_during_a_run();
    let scenes = vec![
        scene("blue", None, &blue_run(), None, 40),
        scene("wash", None, &washed_run(), None, 40),
        scene("echo", None, &outputs, Some((before, echo)), 60),
        scene("pane", Some(&load), &after, None, 60),
    ];
    serde_json::json!({ "streams": streams, "scenes": scenes })
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

/// Your slash as `dam_message` in fight.c prints it to you, from the
/// format `Your %s $c%s $N%c`, with the yellow `process_color` in comm.c
/// writes for the damage you deal and the reset after the verb.
const DISMEMBERS: &str = "Your slash \x1b[0;33mDISMEMBERS\x1b[0;0m a Blackwatch guard!";

/// The plain text of each line the scrollback ring keeps after `p` logs
/// in to the fake Aabahran with a compact prompt pinned and plays a round
/// in which you hit twice and dodge twice, first out of a fight, then in
/// one.
fn rounds_of(live: Live) -> Vec<String> {
    let mut session = Session::new(live);
    let mut mud = Mud::playing(Options {
        compact: true,
        ..Options::new(Build::New)
    });
    let round = [DISMEMBERS, DISMEMBERS, DODGE, DODGE].join("\n\r");
    let mut reads = vec![session.read(&mud.login())];
    reads.push(session.read(&mud.pulse_later(&round)));
    let fight: Vec<u8> = mud
        .command("fight")
        .into_iter()
        .flat_map(|write| write.bytes)
        .collect();
    reads.push(session.read(&fight));
    reads.push(session.read(&mud.pulse_later(&round)));
    ring_of(&reads)[LOGIN.len()..].to_vec()
}

/// [`rounds_of`] with In a fight and Attack lines set to `fights` and
/// `attacks`.
fn rounds(fights: bool, attacks: bool) -> Vec<String> {
    let (mut p, c) = collapsing(PromptShow::Pinned);
    p.ui.collapse_fight_lines = fights;
    p.ui.collapse_attack_lines = attacks;
    rounds_of((p, c))
}

#[test]
fn a_profile_starts_with_the_rules_the_stage_starts_with() {
    let ui = crate::profile::ui::UiConfig::default();
    assert_eq!(
        vosh_prompt::stage::CollapseRules {
            fights: ui.collapse_fight_lines,
            attacks: ui.collapse_attack_lines,
        },
        vosh_prompt::stage::CollapseRules::default()
    );
}

#[test]
fn in_a_fight_and_attack_lines_decide_which_lines_collapse() {
    let hit = vosh_protocol::ansi::plain_text(DISMEMBERS.as_bytes());
    let hits = times(2, DISMEMBERS);
    let dodges = times(2, DODGE);
    let attacks = "A Blackwatch guard attacks you!";
    let battle = "A Blackwatch guard has quite a few wounds. ";
    let tank = "Tester: [===|===|===|---]";
    let lines = |want: &[&str]| {
        want.iter()
            .map(|line| (*line).to_string())
            .collect::<Vec<_>>()
    };
    // At first a fight collapses, and each hit shows on its own, in a
    // fight or not.
    let first = lines(&[
        &hit, &hit, &dodges, attacks, battle, tank, &hit, &hit, &dodges, battle, tank,
    ]);
    assert_eq!(rounds_of(collapsing(PromptShow::Pinned)), first);
    assert_eq!(rounds(true, false), first);
    // Attack lines on: the hits collapse too.
    assert_eq!(
        rounds(true, true),
        lines(&[&hits, &dodges, attacks, battle, tank, &hits, &dodges, battle, tank,])
    );
    // In a fight showing every line: nothing collapses while Char.Combat
    // names a target, or after a hit of yours until the prompt, since a
    // server that sends its tick after the text sends a fight's first
    // round before Char.Combat names anyone. The hits show on their own
    // whatever Attack lines says.
    let whole = lines(&[
        &hit, &hit, DODGE, DODGE, attacks, battle, tank, &hit, &hit, DODGE, DODGE, battle, tank,
    ]);
    assert_eq!(rounds(false, false), whole);
    assert_eq!(rounds(false, true), whole);
}

/// The guard's death, as `act` prints `$n is DEAD!!` to the room
/// (fight.c:6295).
const DEAD: &str = "A Blackwatch guard is DEAD!!";

#[test]
fn the_round_that_ends_a_fight_is_a_line_of_the_fight() {
    let attacks = "A Blackwatch guard attacks you!";
    let battle = "A Blackwatch guard has quite a few wounds. ";
    let tank = "Tester: [===|===|===|---]";
    for fights in [true, false] {
        let (mut p, c) = collapsing(PromptShow::Pinned);
        p.ui.collapse_fight_lines = fights;
        let mut session = Session::new((p, c));
        let mut mud = Mud::playing(Options {
            compact: true,
            ..Options::new(Build::New)
        });
        let mut reads = vec![session.read(&mud.login())];
        let fight: Vec<u8> = mud
            .command("fight")
            .into_iter()
            .flat_map(|write| write.bytes)
            .collect();
        reads.push(session.read(&fight));
        // The Char.Combat {} that ends the fight comes before the round's
        // text, and the round is still the fight's.
        reads.push(session.read(&mud.fight_ends_later(&[DODGE, DODGE, DEAD].join("\n\r"))));
        // The prompt ends that round, and out of the fight lines collapse.
        reads.push(session.read(&mud.pulse_later(&[DODGE, DODGE].join("\n\r"))));
        let ring = ring_of(&reads)[LOGIN.len()..].to_vec();
        let dodges = times(2, DODGE);
        let round: &[&str] = if fights { &[&dodges] } else { &[DODGE, DODGE] };
        let want = [&[attacks, battle, tank][..], round, &[DEAD, &dodges]].concat();
        assert_eq!(ring, rows(&want), "In a fight on Collapse: {fights}");
    }
}

/// The ring after a guard starts a fight on you with a round in which
/// you hit twice and dodge twice, from a game that sends its prompt tick
/// in `order`, then a round of two dodges once the prompt ended the
/// first one, then the fight ends and two dodges come out of it.
fn first_round(order: TickOrder, fights: bool) -> Vec<String> {
    let (mut p, c) = collapsing(PromptShow::Pinned);
    p.ui.collapse_fight_lines = fights;
    let mut session = Session::new((p, c));
    let mut mud = Mud::playing(Options {
        compact: true,
        order,
        ..Options::new(Build::New)
    });
    let round = [DISMEMBERS, DISMEMBERS, DODGE, DODGE].join("\n\r");
    let mut reads = vec![session.read(&mud.login())];
    reads.push(session.read(&mud.fight_starts_later(&round)));
    reads.push(session.read(&mud.pulse_later(&[DODGE, DODGE].join("\n\r"))));
    reads.push(session.read(&mud.fight_ends_later(DEAD)));
    reads.push(session.read(&mud.pulse_later(&[DODGE, DODGE].join("\n\r"))));
    ring_of(&reads)[LOGIN.len()..].to_vec()
}

#[test]
fn the_first_round_of_a_fight_is_the_fights_whichever_way_the_tick_comes() {
    let hit = vosh_protocol::ansi::plain_text(DISMEMBERS.as_bytes());
    let battle = "A Blackwatch guard has quite a few wounds. ";
    let tank = "Tester: [===|===|===|---]";
    let dodges = times(2, DODGE);
    for order in [TickOrder::First, TickOrder::Middle] {
        // In a fight showing every line: the first round shows whole,
        // though a game that sends the tick after the text names the
        // guard in Char.Combat only after it.
        assert_eq!(
            first_round(order, false),
            rows(&[
                &hit, &hit, DODGE, DODGE, battle, tank, DODGE, DODGE, battle, tank, DEAD, &dodges
            ]),
            "{order:?}"
        );
        // In a fight on: the dodges of each round collapse.
        assert_eq!(
            first_round(order, true),
            rows(&[&hit, &hit, &dodges, battle, tank, &dodges, battle, tank, DEAD, &dodges]),
            "{order:?}"
        );
    }
}

#[test]
fn a_hit_you_watch_starts_no_fight() {
    let (mut p, c) = collapsing(PromptShow::Pinned);
    p.ui.collapse_fight_lines = false;
    let mut session = Session::new((p, c));
    let mut mud = Mud::playing(Options {
        compact: true,
        order: TickOrder::Middle,
        ..Options::new(Build::New)
    });
    let watched = "Orla's slash \x1b[0;33mDISMEMBERS\x1b[0;0m a Blackwatch guard!";
    let round = [watched, DODGE, DODGE].join("\n\r");
    let reads = vec![
        session.read(&mud.login()),
        session.read(&mud.pulse_later(&round)),
    ];
    let ring = ring_of(&reads)[LOGIN.len()..].to_vec();
    let hit = vosh_protocol::ansi::plain_text(watched.as_bytes());
    assert_eq!(ring, rows(&[&hit, &times(2, DODGE)]));
}

#[test]
fn the_battle_line_shows_every_round_while_a_fight_shows_every_line() {
    let battle = "A Blackwatch guard has quite a few wounds. ";
    let streams = streams();
    let (_, login, fight) = streams
        .iter()
        .find(|(name, _, _)| *name == "fight")
        .expect("the fight stream");
    let sent = String::from_utf8_lossy(fight)
        .split("\n\r")
        .filter(|line| *line == battle)
        .count();
    assert!(sent > 1, "{sent}");
    for fights in [true, false] {
        let (mut p, c) = collapsing(PromptShow::Pinned);
        p.ui.collapse_fight_lines = fights;
        let mut session = Session::new((p, c));
        let reads = replay(&mut session, login, fight, &[]);
        let ring = ring_of(&reads);
        let joins = reads
            .iter()
            .flat_map(made)
            .filter(|made| matches!(made, Some(Repeat::Joins(_))))
            .count();
        if fights {
            assert!(joins > 0, "{ring:?}");
            assert!(ring.contains(&times(3, battle)), "{ring:?}");
        } else {
            assert_eq!(joins, 0, "{ring:?}");
            let shown = ring.iter().filter(|line| line.as_str() == battle).count();
            assert_eq!(shown, sent, "{ring:?}");
        }
    }
}
