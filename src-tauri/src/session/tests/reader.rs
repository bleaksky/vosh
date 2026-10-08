//! What a screen reader reads of each read, played through the session's
//! own steps (R21 and R25 review, Q19 to Q21). It reads the plain text of
//! what shows, after gags, routes and replaces, keeps your prompt apart
//! for when you ask, and reads a partial the end of a read paints once.
//!
//! The lines are the game's own, from `update.c`, `fight.c` and `comm.c`
//! in the server source, with an invented name.

use super::*;
use vosh_automation::trigger::{Trigger, TriggerAction};
use vosh_prompt::testkit::{Build, Mud, Options};

const HUNGRY: &str = "You are hungry.";
const THIRSTY: &str = "You are thirsty.";
const DODGE: &str = "You dodge Maren's attack.";

/// What the fake Aabahran's prompt reads at full, the `%c` line end
/// dropped.
const PROMPT_TEXT: &str = "[1020/1020hp 800/800mn 930/930mv]";

/// The fake Aabahran after you log in as Orla.
fn game() -> Mud {
    Mud::playing(Options {
        name: "Orla".into(),
        ..Options::new(Build::New)
    })
}

/// A session that reads the fake Aabahran's prompt and draws `<hp>` in
/// its place, with Read new game lines on when `on` says, past the login.
/// Returns it with the game.
fn session_with(on: bool, edit: impl FnOnce(&mut Profile)) -> (Session, Mud) {
    let (mut p, c) = profile(CODES, HP, true);
    p.ui.screen_reader = on;
    edit(&mut p);
    let mut session = Session::new((p, c));
    let mut mud = game();
    session.read(&mud.login());
    (session, mud)
}

/// A trigger on `pattern` with these actions.
fn trigger(name: &str, pattern: &str, actions: Vec<TriggerAction>) -> Trigger {
    let mut actions = actions.into_iter();
    let mut trigger = Trigger::new(name, pattern, actions.next().expect("an action"));
    trigger.actions.extend(actions);
    trigger
}

/// The lines a read handed the reader.
fn lines(read: &Read) -> Vec<&str> {
    read.reader.lines.iter().map(String::as_str).collect()
}

#[test]
fn the_reader_off_reads_nothing() {
    let (mut session, mut mud) = session_with(false, |_| {});
    let read = session.read(&mud.pulse_later(HUNGRY));
    assert!(read.reader.lines.is_empty(), "{:?}", read.reader);
    assert_eq!(read.reader.count, 0);
    assert_eq!(read.reader.prompt, None);
}

#[test]
fn a_line_reads_and_the_prompt_goes_apart() {
    let (mut session, mut mud) = session_with(true, |_| {});
    let read = session.read(&mud.pulse_later(&format!("{HUNGRY}\n\r{THIRSTY}")));
    // The blank line before the prompt reads nothing.
    assert_eq!(lines(&read), [HUNGRY, THIRSTY]);
    assert_eq!(read.reader.count, 2);
    assert_eq!(read.reader.prompt.as_deref(), Some(PROMPT_TEXT));
}

#[test]
fn a_gag_hides_a_line_and_a_route_that_gags_hides_it_too() {
    let (mut session, mut mud) = session_with(true, |p| {
        let gag = trigger("hungry", "hungry", vec![TriggerAction::Gag]);
        let route = trigger(
            "thirsty",
            "thirsty",
            vec![
                TriggerAction::Route {
                    pane: "chat".into(),
                },
                TriggerAction::Gag,
            ],
        );
        p.triggers.set(gag).expect("the gag compiles");
        p.triggers.set(route).expect("the route compiles");
    });
    let read = session.read(&mud.pulse_later(&format!("{HUNGRY}\n\r{THIRSTY}\n\r{DODGE}")));
    assert_eq!(lines(&read), [DODGE]);
}

#[test]
fn a_replace_reads_the_new_text() {
    let (mut session, mut mud) = session_with(true, |p| {
        let replace = trigger(
            "hungry",
            "hungry",
            vec![TriggerAction::Replace {
                template: "starving".into(),
            }],
        );
        p.triggers.set(replace).expect("the replace compiles");
    });
    let read = session.read(&mud.pulse_later(HUNGRY));
    assert_eq!(lines(&read), ["You are starving."]);
}

#[test]
fn a_hidden_line_reads_the_echo_in_its_place() {
    let (mut session, mut mud) = session_with(true, |p| {
        let echo = trigger(
            "hungry",
            "hungry",
            vec![
                TriggerAction::Gag,
                TriggerAction::Script {
                    body: "mud.echo('Eat something soon')".into(),
                },
            ],
        );
        p.triggers.set(echo).expect("the trigger compiles");
    });
    let read = session.read(&mud.pulse_later(HUNGRY));
    assert_eq!(lines(&read), ["Eat something soon"]);
}

#[test]
fn a_collapsed_run_reads_each_line_without_its_count() {
    let (mut session, mut mud) = session_with(true, |p| p.ui.collapse_repeats = true);
    let read = session.read(&mud.pulse_later(&[DODGE; 3].join("\n\r")));
    assert_eq!(lines(&read), [DODGE; 3]);
    // The screen shows the run once with its count.
    let shown = plain(&read.out.bytes);
    assert_eq!(shown.matches(DODGE).count(), 1, "{shown:?}");
}

#[test]
fn a_partial_a_ga_ends_reads_as_a_line() {
    let (mut session, _) = session_with(true, |_| {});
    // The pager's question, as comm.c writes it, then IAC GA.
    let mut pager = b"\r[Hit Return to continue]\r".to_vec();
    pager.extend([telnet_codes::IAC, telnet_codes::GA]);
    let read = session.read(&pager);
    assert_eq!(lines(&read), ["[Hit Return to continue]"]);
    assert_eq!(read.reader.prompt, None);
}

#[test]
fn a_painted_partial_reads_once_and_the_line_that_ends_it_reads_the_rest() {
    let (mut session, _) = session_with(true, |_| {});
    // The login asks with no GA, so the end of the read paints it.
    let read = session.read(b"Password: ");
    assert_eq!(lines(&read), ["Password:"]);
    let read = session.read(b"\n\rPassword must be at least five characters long.\n\r");
    assert_eq!(
        lines(&read),
        ["Password must be at least five characters long."]
    );

    // A line the reads split reads its start, then only the rest.
    let read = session.read(b"You are hun");
    assert_eq!(lines(&read), ["You are hun"]);
    let read = session.read(b"gry.\n\r");
    assert_eq!(lines(&read), ["gry."]);
}

#[test]
fn a_partial_that_waited_reads_as_it_paints_at_the_deadline() {
    let mut wire = Wire::new(super::steps::codes_profile("<%hhp %mm %vmv> ", HP));
    wire.p.ui.screen_reader = true;
    let batch = wire.read_holding(b"<10hp 2");
    assert!(batch.hold);
    assert!(batch.reader.lines.is_empty(), "{:?}", batch.reader);
    let mut out = vosh_prompt::stage::Output::new(false);
    let mut reader = crate::session::reader::ReaderFeed::default();
    hold_step(&wire.p, &mut wire.c, &mut wire.acc, &mut out, &mut reader);
    assert_eq!(Vec::from(reader.lines), ["<10hp 2"]);
    // The rest makes it your prompt, which joins no lines.
    let batch = wire.read_with(b"0m 30mv> ", false, false);
    assert!(batch.reader.lines.is_empty(), "{:?}", batch.reader);
    assert_eq!(batch.reader.prompt.as_deref(), Some("<10hp 20m 30mv>"));
    assert_eq!(wire.c.reader_heard, None);
}

#[test]
fn a_flood_keeps_the_last_500_lines_and_counts_them_all() {
    let (mut session, mut mud) = session_with(true, |_| {});
    let flood: Vec<&str> = (0..600)
        .map(|i| if i % 2 == 0 { HUNGRY } else { THIRSTY })
        .collect();
    let read = session.read(&mud.pulse_later(&flood.join("\n\r")));
    assert_eq!(read.reader.lines.len(), 500);
    assert_eq!(read.reader.count, 600);
    assert_eq!(lines(&read), flood[100..]);
}
