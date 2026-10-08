//! The walker, one event at a time, on a clock the tests hold. The rooms
//! and tiles are the Caranduin ones the fake game of the walk tests
//! plays, from the Map.Tiles fixtures.

use std::time::Duration;

use serde_json::{json, Value};
use tokio::time::Instant;
use vosh_automation::alias::ExpandStep;

use crate::input::walk::{parse_steps, Route, WalkCommand, WalkPlan};
use crate::session::walk::{answer, WalkOut, WalkProgress, Walker, Why, BACKSTOP, NOT_WALKING};
use crate::tests::walk::{map_tiles, room_info, FOUNTAIN, ROAD};

/// The room west of the Common Road, which the fixtures hold no tiles
/// for.
const ROAD_WEST: i64 = 4404;

fn start(steps: &str, rest: &[&str]) -> WalkCommand {
    WalkCommand::Start {
        plan: WalkPlan {
            steps: parse_steps(steps).expect("the steps read"),
            route: None,
        },
        rest: held(rest),
    }
}

fn held(rest: &[&str]) -> Vec<ExpandStep> {
    rest.iter()
        .map(|c| ExpandStep::Command((*c).to_string()))
        .collect()
}

fn stop() -> WalkCommand {
    WalkCommand::Stop {
        key: false,
        rest: Vec::new(),
    }
}

fn status() -> WalkCommand {
    WalkCommand::Status { rest: Vec::new() }
}

/// The look in room `num`, its tiles first when there are any, as the
/// game sends them.
fn arrive(w: &mut Walker, num: i64, now: Instant) -> WalkOut {
    if let Some(tiles) = map_tiles(num) {
        w.tiles(&tiles);
    }
    w.room_info(&room_info(num), now)
}

/// A walker that stands in `num`, having seen its look.
fn standing_in(num: i64, now: Instant) -> Walker {
    let mut w = Walker::default();
    assert_eq!(arrive(&mut w, num, now), WalkOut::default());
    w
}

/// What the walker sends for one step.
fn step(letter: char) -> WalkOut {
    WalkOut {
        send: format!("{letter}\r\n").into_bytes(),
        ..WalkOut::default()
    }
}

fn said(lines: &[&str]) -> WalkOut {
    WalkOut {
        lines: lines.iter().map(|l| (*l).to_string()).collect(),
        ..WalkOut::default()
    }
}

#[test]
fn a_walk_sends_one_step_per_room_and_lets_go_of_the_rest_on_arrival() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    assert_eq!(w.command(start("2w2e", &["deposit all"]), t), step('w'));
    assert_eq!(w.deadline(), Some(t + BACKSTOP));
    // The tiles at the fountain said west leads to the Common Road.
    let t1 = t + Duration::from_millis(300);
    assert_eq!(arrive(&mut w, ROAD, t1), step('w'));
    assert_eq!(w.deadline(), Some(t1 + BACKSTOP));
    // No tiles came for the next room, so any new room will do there.
    assert_eq!(arrive(&mut w, ROAD_WEST, t1), step('e'));
    assert_eq!(arrive(&mut w, ROAD, t1), step('e'));
    assert_eq!(
        arrive(&mut w, FOUNTAIN, t1),
        WalkOut {
            release: held(&["deposit all"]),
            ..WalkOut::default()
        }
    );
    assert_eq!(w.deadline(), None);
    assert_eq!(w.command(status(), t1), said(&[NOT_WALKING]));
}

#[test]
fn bare_walk_counts_the_steps_left_and_stop_stops() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    assert_eq!(w.command(status(), t), said(&[NOT_WALKING]));
    assert_eq!(w.command(stop(), t), said(&[NOT_WALKING]));
    let _ = w.command(start("2w", &[]), t);
    assert_eq!(w.command(status(), t), said(&["[walk] 2 of 2 steps left."]));
    let _ = arrive(&mut w, ROAD, t);
    assert_eq!(w.command(status(), t), said(&["[walk] 1 of 2 steps left."]));
    assert_eq!(
        w.command(stop(), t),
        said(&["[walk] Stopped after 1 of 2 steps."])
    );
    // The step on its way still lands, and nothing follows it.
    assert_eq!(arrive(&mut w, ROAD_WEST, t), WalkOut::default());
    // A walk of one step says step.
    let _ = w.command(start("e", &[]), t);
    assert_eq!(w.command(status(), t), said(&["[walk] 1 of 1 step left."]));
}

#[test]
fn stop_and_status_let_go_of_the_rest_of_their_line() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let out = w.command(
        WalkCommand::Status {
            rest: held(&["look"]),
        },
        t,
    );
    assert_eq!(out.release, held(&["look"]));
    let out = w.command(
        WalkCommand::Stop {
            key: false,
            rest: held(&["look"]),
        },
        t,
    );
    assert_eq!(out.lines, [NOT_WALKING]);
    assert_eq!(out.release, held(&["look"]));
}

#[test]
fn esc_stops_a_walk_and_says_nothing_otherwise() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let esc = || WalkCommand::Stop {
        key: true,
        rest: Vec::new(),
    };
    assert_eq!(w.command(esc(), t), WalkOut::default());
    let _ = w.command(start("2w", &["get all"]), t);
    assert_eq!(
        w.command(esc(), t),
        said(&["[walk] Stopped after 0 of 2 steps, so Vosh did not send the rest of the line."])
    );
}

#[test]
fn a_line_that_says_the_step_failed_stops_the_walk_where_you_stand() {
    let t = Instant::now();
    // A closed door names its whole keyword, one word or more
    // (`comm.c:7224`).
    for line in [
        "Alas, you cannot go that way.",
        "The door is closed.",
        "The gate is closed.",
        "The iron door is closed.",
        "You need a boat to go there.",
        "You can't fly.",
        "You are too exhausted.",
        "Better stand up first.",
        "No way!  You are still fighting!",
        "Nah... You feel too relaxed...",
        "That room is private right now.",
    ] {
        let mut w = standing_in(FOUNTAIN, t);
        let _ = w.command(start("w2e", &[]), t);
        let _ = arrive(&mut w, ROAD, t);
        assert_eq!(
            w.line(line, t),
            said(&["[walk] Stopped after 1 of 3 steps."]),
            "{line:?}"
        );
        assert_eq!(w.deadline(), None, "{line:?}");
    }
}

#[test]
fn lines_that_only_look_like_a_failure_leave_the_walk_going() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &[]), t);
    // Lines a step can print on its way, and a container's, from
    // `act_move.c:529`, `437` and `736` and `act_info.c:2726`.
    for line in [
        "You attempt to climb in that direction.",
        "A magical barrier parts about you.",
        "You feel sluggish.",
        "It is closed.",
        "[Exits: north south west]",
    ] {
        assert_eq!(w.line(line, t), WalkOut::default(), "{line:?}");
    }
    assert_eq!(arrive(&mut w, ROAD, t), step('w'));
    // With no step on its way the walker reads no line.
    let _ = arrive(&mut w, ROAD_WEST, t);
    assert_eq!(
        w.line("Alas, you cannot go that way.", t),
        WalkOut::default()
    );
}

#[test]
fn the_answer_to_a_step_is_what_runs_on_from_an_unread_prompt() {
    let prompt = "<1020hp 800m 930mv> ";
    let line = "<1020hp 800m 930mv> Alas, you cannot go that way.";
    assert_eq!(answer(line, Some(prompt)), "Alas, you cannot go that way.");
    assert_eq!(
        answer("It is pitch black ... ", None),
        "It is pitch black ... "
    );
    // A partial the line does not start with leaves the line whole.
    assert_eq!(
        answer("Alas, you cannot go that way.", Some("<1020hp")),
        "Alas, you cannot go that way."
    );

    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &[]), t);
    assert_eq!(w.line(line, t), WalkOut::default());
    assert_eq!(
        w.line(answer(line, Some(prompt)), t),
        said(&["[walk] Stopped after 0 of 2 steps."])
    );
}

#[test]
fn another_room_than_the_tiles_promised_stops_the_walk() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &["get all"]), t);
    // The step led elsewhere, so it does not count.
    assert_eq!(
        w.room_info(&room_info(4631), t),
        said(&["[walk] Stopped after 0 of 2 steps, so Vosh did not send the rest of the line."])
    );
}

#[test]
fn the_room_you_left_again_is_a_look_and_the_step_waits_on() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("w", &["get all"]), t);
    // A look you typed before the step, or a trigger's, shows the room
    // you stand in.
    assert_eq!(arrive(&mut w, FOUNTAIN, t), WalkOut::default());
    assert_eq!(w.command(status(), t), said(&["[walk] 1 of 1 step left."]));
    assert_eq!(arrive(&mut w, ROAD, t).release, held(&["get all"]));
}

#[test]
fn a_fight_or_a_position_but_standing_stops_the_walk() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &[]), t);
    assert_eq!(w.combat(&json!({})), WalkOut::default());
    assert_eq!(
        w.state(&json!({"position": "standing", "language": "common"})),
        WalkOut::default()
    );
    assert_eq!(
        w.combat(&json!({"target": "a Blackwatch guard", "hidden": true})),
        said(&["[walk] Stopped after 0 of 2 steps."])
    );
    // With no walk under way neither says anything.
    assert_eq!(
        w.combat(&json!({"target": "a Blackwatch guard"})),
        WalkOut::default()
    );

    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &[]), t);
    let _ = arrive(&mut w, ROAD, t);
    assert_eq!(
        w.state(&json!({"position": "sitting", "language": "common"})),
        said(&["[walk] Stopped after 1 of 2 steps."])
    );
}

#[test]
fn the_blind_and_dark_looks_lose_sight_of_the_room() {
    let t = Instant::now();
    for line in ["It is pitch black ... ", "You can't see a thing!"] {
        let mut w = standing_in(FOUNTAIN, t);
        let _ = w.command(start("3w", &[]), t);
        let _ = arrive(&mut w, ROAD, t);
        assert_eq!(
            w.line(line, t),
            said(&["[walk] Stopped after 2 of 3 steps. Vosh lost sight of the room."]),
            "{line:?}"
        );
        // Vosh no longer knows where you stand, so the next walk takes
        // any new room, even one the old tiles would have refused.
        let _ = w.command(start("e", &["look"]), t);
        assert_eq!(w.room_info(&room_info(4631), t).release, held(&["look"]));
    }
}

#[test]
fn a_command_you_send_stops_the_walk_and_a_bare_enter_does_not() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &[]), t);
    assert_eq!(w.typed(b"\r\n"), WalkOut::default());
    assert_eq!(
        w.typed(b"look\r\n"),
        said(&["[walk] Stopped after 0 of 2 steps."])
    );
    assert_eq!(w.typed(b"look\r\n"), WalkOut::default());
}

#[test]
fn a_new_walk_takes_over_once_the_step_in_flight_lands() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("3w", &["get all"]), t);
    // Nothing leaves while the step is on its way.
    assert_eq!(w.command(start("e", &["say back"]), t), WalkOut::default());
    assert_eq!(w.command(status(), t), said(&["[walk] 3 of 3 steps left."]));
    // The old walk stops where the step lands, and the new one plans from
    // there.
    assert_eq!(
        arrive(&mut w, ROAD, t),
        WalkOut {
            send: b"e\r\n".to_vec(),
            lines: vec![
                "[walk] Stopped after 1 of 3 steps, so Vosh did not send the rest of the line."
                    .to_string()
            ],
            ..WalkOut::default()
        }
    );
    assert_eq!(arrive(&mut w, FOUNTAIN, t).release, held(&["say back"]));
}

#[test]
fn a_walk_that_arrives_as_a_new_one_waits_lets_go_of_what_it_held() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("w", &["deposit all"]), t);
    assert_eq!(w.command(start("e", &["say back"]), t), WalkOut::default());
    // The last step arrived, so the walk ended as planned. What it held
    // goes, and the new walk starts after it.
    assert_eq!(
        arrive(&mut w, ROAD, t),
        WalkOut {
            send: b"e\r\n".to_vec(),
            release: held(&["deposit all"]),
            ..WalkOut::default()
        }
    );
    assert_eq!(arrive(&mut w, FOUNTAIN, t).release, held(&["say back"]));
}

#[test]
fn a_step_that_leads_elsewhere_hands_over_to_the_walk_that_waits() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &["get all"]), t);
    let _ = w.command(start("e", &["say back"]), t);
    // The new walk plans from the room the step reached, as it would
    // after a step that failed.
    assert_eq!(
        w.room_info(&room_info(4631), t),
        WalkOut {
            send: b"e\r\n".to_vec(),
            lines: vec![
                "[walk] Stopped after 0 of 2 steps, so Vosh did not send the rest of the line."
                    .to_string()
            ],
            ..WalkOut::default()
        }
    );
    assert_eq!(
        w.room_info(&room_info(4446), t).release,
        held(&["say back"])
    );
}

#[test]
fn a_new_walk_waits_for_the_step_of_a_walk_you_stopped() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &[]), t);
    let _ = w.command(stop(), t);
    assert_eq!(w.command(start("e", &[]), t), WalkOut::default());
    // A failed step lands too, and you stand where you were.
    assert_eq!(w.line("Alas, you cannot go that way.", t), step('e'));
}

#[test]
fn a_stop_while_a_new_walk_waits_stops_that_one() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &[]), t);
    let _ = w.command(stop(), t);
    let _ = w.command(start("3e", &["look"]), t);
    assert_eq!(
        w.command(stop(), t),
        said(&["[walk] Stopped after 0 of 3 steps, so Vosh did not send the rest of the line."])
    );
    assert_eq!(arrive(&mut w, ROAD, t), WalkOut::default());
}

#[test]
fn the_backstop_gives_up_ten_seconds_after_the_step_left() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &[]), t);
    let deadline = t + Duration::from_secs(10);
    assert_eq!(w.deadline(), Some(deadline));
    assert_eq!(
        w.expire(deadline - Duration::from_millis(1)),
        WalkOut::default()
    );
    assert_eq!(
        w.expire(deadline),
        said(&["[walk] Stopped. Vosh lost track of the walk."])
    );
    assert_eq!(w.deadline(), None);
    assert_eq!(w.command(status(), deadline), said(&[NOT_WALKING]));

    // Each step gets its own ten seconds.
    let t2 = deadline + Duration::from_secs(1);
    let _ = w.command(start("2e", &["get all"]), t2);
    let t3 = t2 + Duration::from_secs(8);
    assert_eq!(w.room_info(&room_info(FOUNTAIN), t3), step('e'));
    assert_eq!(w.expire(t2 + Duration::from_secs(12)), WalkOut::default());
    assert_eq!(
        w.expire(t3 + BACKSTOP),
        said(&[
            "[walk] Stopped, so Vosh did not send the rest of the line. Vosh lost track of the walk."
        ])
    );

    // A step left over from a walk you stopped gives up quietly.
    let _ = w.command(start("w", &[]), t3);
    let _ = w.command(stop(), t3);
    assert_eq!(w.expire(t3 + BACKSTOP), WalkOut::default());
    assert_eq!(w.deadline(), None);
}

#[test]
fn a_route_from_a_click_expects_its_rooms_and_drops_from_another_room() {
    let t = Instant::now();
    let route = |start: i64| WalkCommand::Start {
        plan: WalkPlan {
            steps: parse_steps("2w").expect("the steps read"),
            route: Some(Route {
                start,
                rooms: vec![ROAD, ROAD_WEST],
            }),
        },
        rest: Vec::new(),
    };
    let mut w = standing_in(FOUNTAIN, t);
    // Planned from a room you have left, it drops.
    assert_eq!(w.command(route(ROAD), t), WalkOut::default());
    assert_eq!(w.command(status(), t), said(&[NOT_WALKING]));
    assert_eq!(w.command(route(FOUNTAIN), t), step('w'));
    assert_eq!(arrive(&mut w, ROAD, t), step('w'));
    // The route names each room, so one it does not name stops the walk
    // where no tiles would have.
    assert_eq!(
        w.room_info(&room_info(4403), t),
        said(&["[walk] Stopped after 1 of 2 steps."])
    );
}

#[test]
fn tiles_count_only_for_the_room_info_after_them() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    // The Common Road arrives without tiles, so the fountain's tiles
    // never apply there, and any new room will do for the next step.
    let _ = w.command(start("2w", &[]), t);
    assert_eq!(w.room_info(&room_info(ROAD), t), step('w'));
    let out = w.room_info(&json!({"num": 9999}), t);
    assert_eq!(out, WalkOut::default());
    assert_eq!(w.command(status(), t), said(&[NOT_WALKING]));
    // A packet with no number changes nothing.
    assert_eq!(w.room_info(&Value::Null, t), WalkOut::default());
}

fn walking(done: usize, total: usize, left: &str, route: bool) -> WalkProgress {
    WalkProgress::Walking {
        done,
        total,
        left: left.to_string(),
        route,
    }
}

fn stopped(done: usize, total: usize, why: Why) -> WalkProgress {
    WalkProgress::Stopped { done, total, why }
}

/// Something the walker hears.
type Event = fn(&mut Walker, Instant);

/// A click on the map from the fountain two rooms west.
fn route_from(start: i64) -> WalkCommand {
    WalkCommand::Start {
        plan: WalkPlan {
            steps: parse_steps("2w").expect("the steps read"),
            route: Some(Route {
                start,
                rooms: vec![ROAD, ROAD_WEST],
            }),
        },
        rest: Vec::new(),
    }
}

#[test]
fn progress_counts_each_landing_and_ends_idle_on_arrival() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    assert_eq!(w.progress(), WalkProgress::Idle);
    let _ = w.command(start("2w2e", &[]), t);
    assert_eq!(w.progress(), walking(0, 4, "2w2e", false));
    let _ = arrive(&mut w, ROAD, t);
    assert_eq!(w.progress(), walking(1, 4, "w2e", false));
    let _ = arrive(&mut w, ROAD_WEST, t);
    assert_eq!(w.progress(), walking(2, 4, "2e", false));
    let _ = arrive(&mut w, ROAD, t);
    assert_eq!(w.progress(), walking(3, 4, "e", false));
    let _ = arrive(&mut w, FOUNTAIN, t);
    assert_eq!(w.progress(), WalkProgress::Idle);

    // A click says so, and the steps left read back as the walk.
    let _ = w.command(route_from(FOUNTAIN), t);
    assert_eq!(w.progress(), walking(0, 2, "2w", true));
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("99n51n e", &[]), t);
    let WalkProgress::Walking { left, .. } = w.progress() else {
        panic!("the walk is under way");
    };
    assert_eq!(left, "99n51ne");
    assert_eq!(parse_steps(&left), parse_steps("99n51n e"));
}

#[test]
fn progress_names_every_reason_a_walk_stops() {
    let t = Instant::now();
    let esc = WalkCommand::Stop {
        key: true,
        rest: Vec::new(),
    };
    let cases: [(&str, Event, WalkProgress); 7] = [
        (
            "a failure line",
            |w, t| {
                let _ = w.line("Alas, you cannot go that way.", t);
            },
            stopped(1, 3, Why::Plain),
        ),
        (
            "another room",
            |w, t| {
                let _ = w.room_info(&room_info(4403), t);
            },
            stopped(1, 3, Why::Plain),
        ),
        (
            "a fight",
            |w, _| {
                let _ = w.combat(&json!({"target": "a Blackwatch guard"}));
            },
            stopped(1, 3, Why::Plain),
        ),
        (
            "sitting",
            |w, _| {
                let _ = w.state(&json!({"position": "sitting"}));
            },
            stopped(1, 3, Why::Plain),
        ),
        (
            "a command",
            |w, _| {
                let _ = w.typed(b"look\r\n");
            },
            stopped(1, 3, Why::Plain),
        ),
        (
            "the dark look",
            |w, t| {
                let _ = w.line("It is pitch black ... ", t);
            },
            stopped(2, 3, Why::LostSight),
        ),
        (
            "the backstop",
            |w, t| {
                let _ = w.expire(t + BACKSTOP);
            },
            stopped(1, 3, Why::LostTrack),
        ),
    ];
    for (what, event, progress) in cases {
        let mut w = standing_in(FOUNTAIN, t);
        let _ = w.command(start("3w", &[]), t);
        let _ = arrive(&mut w, ROAD, t);
        event(&mut w, t);
        assert_eq!(w.progress(), progress, "{what}");
    }

    // Esc and `#walk stop` stop it plainly, and Esc with no walk leaves
    // the last stop as it was.
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2w", &[]), t);
    let _ = w.command(esc.clone(), t);
    assert_eq!(w.progress(), stopped(0, 2, Why::Plain));
    let _ = w.command(esc, t);
    assert_eq!(w.progress(), stopped(0, 2, Why::Plain));
    let _ = w.line("Alas, you cannot go that way.", t);
    let _ = w.command(start("e", &[]), t);
    let _ = w.command(stop(), t);
    assert_eq!(w.progress(), stopped(0, 1, Why::Plain));
    // The next walk starts afresh.
    let _ = w.line("Alas, you cannot go that way.", t);
    let _ = w.command(start("w", &[]), t);
    assert_eq!(w.progress(), walking(0, 1, "w", false));
}

#[test]
fn progress_follows_a_click_that_takes_over() {
    let t = Instant::now();
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("n", &[]), t);
    // The walk under way shows until the step in flight lands.
    let _ = w.command(route_from(FOUNTAIN), t);
    assert_eq!(w.progress(), walking(0, 1, "n", false));
    // The step failed, so you stand where the click was planned, and it
    // takes over.
    let _ = w.line("Alas, you cannot go that way.", t);
    assert_eq!(w.progress(), walking(0, 2, "2w", true));
    let _ = arrive(&mut w, ROAD, t);
    assert_eq!(w.progress(), walking(1, 2, "w", true));

    // A click that waits behind a walk you stopped shows as it waits.
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("2e", &[]), t);
    let _ = w.command(stop(), t);
    let _ = w.command(route_from(FOUNTAIN), t);
    assert_eq!(w.progress(), walking(0, 2, "2w", true));
    // The stopped step led elsewhere, so the click, planned from the
    // fountain, drops, and nothing walks.
    let _ = w.room_info(&room_info(4446), t);
    assert_eq!(w.progress(), WalkProgress::Idle);

    // Esc while the click waits stops it before its first step.
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(start("n", &[]), t);
    let _ = w.command(route_from(FOUNTAIN), t);
    let _ = w.command(stop(), t);
    assert_eq!(w.progress(), stopped(0, 1, Why::Plain));
}

#[test]
fn a_click_planned_from_where_the_step_lands_takes_over_a_walk_going_well() {
    let t = Instant::now();
    // The click back east to the fountain, planned from the Common Road.
    let back = |start: i64| WalkCommand::Start {
        plan: WalkPlan {
            steps: parse_steps("e").expect("the steps read"),
            route: Some(Route {
                start,
                rooms: vec![FOUNTAIN],
            }),
        },
        rest: Vec::new(),
    };
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(route_from(FOUNTAIN), t);
    // The step west is on its way, so the page plans the click from the
    // road it reaches, and the click takes over once it lands.
    assert_eq!(w.command(back(ROAD), t), WalkOut::default());
    assert_eq!(
        arrive(&mut w, ROAD, t),
        WalkOut {
            send: b"e\r\n".to_vec(),
            lines: vec!["[walk] Stopped after 1 of 2 steps.".to_string()],
            ..WalkOut::default()
        }
    );
    assert_eq!(w.progress(), walking(0, 1, "e", true));
    assert_eq!(arrive(&mut w, FOUNTAIN, t), WalkOut::default());
    assert_eq!(w.progress(), WalkProgress::Idle);

    // Planned from the room the step leaves, the click drops where the
    // step lands, and the walk stops there.
    let mut w = standing_in(FOUNTAIN, t);
    let _ = w.command(route_from(FOUNTAIN), t);
    let _ = w.command(back(FOUNTAIN), t);
    assert_eq!(
        arrive(&mut w, ROAD, t),
        said(&["[walk] Stopped after 1 of 2 steps."])
    );
    assert_eq!(w.progress(), stopped(1, 2, Why::Plain));
}
