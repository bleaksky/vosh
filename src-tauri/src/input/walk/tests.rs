use vosh_automation::alias::{Alias, ExpandStep};

use super::*;
use crate::input::process;

use Dir::{Down, East, North, South, Up, West};

/// What Vosh says when it cannot read `text` after `#walk`.
fn cannot_read(text: &str) -> String {
    format!(
        "[#walk cannot read {text}. Use n, e, s, w, u, and d, each with an optional count, like 3n2e.]"
    )
}

fn command(text: &str) -> ExpandStep {
    ExpandStep::Command(text.to_string())
}

/// The walk `line` starts, with what it holds, and no bytes or echo.
fn started(p: &mut Profile, line: &str) -> (Vec<Dir>, Vec<ExpandStep>) {
    let r = process(p, line);
    assert!(r.bytes.is_empty(), "{line:?} sent {:?}", r.bytes);
    assert!(r.echo.is_empty(), "{line:?} echoed {:?}", r.echo);
    match r.walk {
        Some(WalkCommand::Start { plan, rest }) => {
            assert_eq!(plan.route, None);
            (plan.steps, rest)
        }
        other => panic!("{line:?} started no walk: {other:?}"),
    }
}

#[test]
fn the_steps_read_as_the_help_says() {
    let mut p = Profile::default();
    for (line, steps) in [
        ("#walk 3n2e", vec![North, North, North, East, East]),
        ("#walk 2w u", vec![West, West, Up]),
        ("#walk ne", vec![North, East]),
        ("#walk N2E d", vec![North, East, East, Down]),
        ("#walk  s  1s ", vec![South, South]),
    ] {
        assert_eq!(started(&mut p, line), (steps, Vec::new()), "{line:?}");
    }
}

#[test]
fn a_part_that_does_not_read_says_what_and_walks_nowhere() {
    let mut p = Profile::default();
    for (line, what) in [
        ("#walk 3x", "x"),
        ("#walk north", "north"),
        ("#walk 2n nort", "nort"),
        ("#walk 3n2x4e", "x"),
        ("#walk n!", "!"),
        ("#walk 3", "3"),
        ("#walk 3 n", "3"),
        ("#walk 0n", "0"),
        ("#walk 100n", "100"),
    ] {
        let r = process(&mut p, line);
        assert_eq!(r.echo, [cannot_read(what)], "{line:?}");
        assert!(r.bytes.is_empty(), "{line:?}");
        assert_eq!(r.walk, None, "{line:?}");
    }
}

#[test]
fn a_walk_takes_at_most_two_hundred_steps() {
    let mut p = Profile::default();
    let (steps, _) = started(&mut p, "#walk 99n99s2e");
    assert_eq!(steps.len(), MAX_STEPS);
    let r = process(&mut p, "#walk 99n99s3e");
    assert_eq!(r.echo, ["[#walk takes at most 200 steps.]"]);
    assert_eq!(r.walk, None);
    assert_eq!(
        parse_steps(&"n".repeat(MAX_STEPS + 1)),
        Err(StepsError::TooMany)
    );
}

#[test]
fn bare_walk_asks_how_far_and_stop_stops() {
    let mut p = Profile::default();
    assert_eq!(
        process(&mut p, "#walk").walk,
        Some(WalkCommand::Status { rest: Vec::new() })
    );
    for line in ["#walk stop", "#walk STOP", "#walk  stop "] {
        assert_eq!(
            process(&mut p, line).walk,
            Some(WalkCommand::Stop {
                key: false,
                rest: Vec::new()
            }),
            "{line:?}"
        );
    }
}

#[test]
fn what_you_type_after_walk_waits_for_it() {
    let mut p = Profile::default();
    p.aliases.set(Alias::new("dep", "deposit %1"));
    p.vars.set("path", "2w");
    let (steps, rest) = started(&mut p, "#walk $path u;dep all;say done");
    assert_eq!(steps, [West, West, Up]);
    assert_eq!(rest, [command("deposit all"), command("say done")]);
}

#[test]
fn an_alias_that_walks_holds_what_follows() {
    let mut p = Profile::default();
    p.aliases.set(Alias::new("bank", "#walk 3n2e;deposit all"));
    let (steps, rest) = started(&mut p, "bank");
    assert_eq!(steps, [North, North, North, East, East]);
    assert_eq!(rest, [command("deposit all")]);

    // What the line holds after the alias waits too.
    let (_, rest) = started(&mut p, "bank;look");
    assert_eq!(rest, [command("deposit all"), command("look")]);
}

#[test]
fn a_piece_of_a_typed_line_walks_after_the_pieces_before_it() {
    let mut p = Profile::default();
    let r = process(&mut p, "look;#walk 2s;get all");
    assert_eq!(r.bytes, b"look\r\n");
    assert_eq!(r.echo, Vec::<String>::new());
    assert_eq!(
        r.walk,
        Some(WalkCommand::Start {
            plan: WalkPlan {
                steps: vec![South, South],
                route: None,
            },
            rest: vec![command("get all")],
        })
    );
}

#[test]
fn a_script_alias_after_walk_waits_and_runs_nothing_yet() {
    let mut p = Profile::default();
    p.aliases
        .set(Alias::new("kk", "ignored").with_script("mud.send('kick')"));
    p.aliases.set(Alias::new("go", "#walk w;kk"));
    let (steps, rest) = started(&mut p, "go");
    assert_eq!(steps, [West]);
    assert!(
        matches!(rest.as_slice(), [ExpandStep::Script(call)] if call.body == "mud.send('kick')"),
        "{rest:?}"
    );
}

#[test]
fn every_other_hash_command_from_an_alias_still_goes_out_as_text() {
    let mut p = Profile::default();
    p.aliases.set(Alias::new("x", "#echo hi;# walkies;look"));
    let r = process(&mut p, "x");
    assert_eq!(r.bytes, b"#echo hi\r\n# walkies\r\nlook\r\n");
    assert_eq!(r.walk, None);
}

#[test]
fn a_walk_that_does_not_read_drops_the_rest_of_its_line() {
    let mut p = Profile::default();
    p.aliases
        .set(Alias::new("bank", "look;#walk 3x;deposit all"));
    let r = process(&mut p, "bank");
    assert_eq!(r.bytes, b"look\r\n");
    assert_eq!(r.echo, [cannot_read("x")]);
    assert_eq!(r.walk, None);
    let r = process(&mut p, "#walk 3x;deposit all");
    assert_eq!(r.bytes, Vec::<u8>::new());
    assert_eq!(r.echo, [cannot_read("x")]);
}

#[test]
fn stop_and_status_carry_the_rest_of_their_line() {
    let mut p = Profile::default();
    assert_eq!(
        process(&mut p, "#walk stop;look").walk,
        Some(WalkCommand::Stop {
            key: false,
            rest: vec![command("look")],
        })
    );
    assert_eq!(
        process(&mut p, "#walk;look").walk,
        Some(WalkCommand::Status {
            rest: vec![command("look")],
        })
    );
}

#[test]
fn walk_args_reads_a_piece_as_the_dispatcher_does() {
    assert_eq!(walk_args("#walk 3n"), Some("3n"));
    assert_eq!(walk_args("  # walk 3n"), Some("3n"));
    assert_eq!(walk_args("#walk"), Some(""));
    assert_eq!(walk_args("#walkies"), None);
    assert_eq!(walk_args("walk 3n"), None);
    assert_eq!(walk_args("#echo #walk"), None);
}

#[test]
fn the_summary_lists_walk() {
    let lines: Vec<&str> = crate::input::slash::HELP_TEXT.lines().collect();
    for (command, says) in [
        ("#walk <steps> ", "walk the steps one room at a time"),
        ("#walk ", "say how many steps are left"),
        ("#walk stop ", "stop walking"),
    ] {
        assert!(
            lines
                .iter()
                .any(|l| l.trim_start().starts_with(command) && l.contains(says)),
            "{command:?}"
        );
    }
}

#[test]
fn a_walk_line_asks_for_no_save() {
    for line in ["#walk 3n2e", "#walk;#alias x y", "#walk stop", "#walk"] {
        let mut effects = crate::input::LineEffects::default();
        effects.note(line, false);
        assert!(!effects.dirty, "{line:?}");
    }
    let mut effects = crate::input::LineEffects::default();
    effects.note("#walkies", false);
    assert!(effects.dirty);
}
