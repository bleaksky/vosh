//! The Room, time, and weather colors preset through the real session
//! against the fake game, after a walk, a look and an immortal's goto,
//! with the room packets before the text, as Aabahran sent them before
//! d50e4a24, and after the people, as it sends them since, in the same
//! read as the people or in the next one. You target the werebeast, so
//! its line shows in your target's color on each look.

use alacritty_terminal::vte::ansi::{Color, NamedColor};
use vosh_prompt::testkit::{Build, Options, TickOrder};

use super::harness::Harness;

/// The things and the person of The Crossroads, room 6909, in the order
/// the look lists them, after its army.
const CROSSROADS: [&str; 3] = [
    "A mighty Fortress looms over the area.",
    "A stand of blue dried leaves grows in the wild here.",
    "A young werebeast stands here, leaning on his spear.",
];

/// The emote the werebeast's GREET program prints as you come in.
const GREET: &str = "A werebeast looks into the sky.";

const YELLOW: Option<Color> = Some(Color::Named(NamedColor::Yellow));
const BRIGHT_RED: Option<Color> = Some(Color::Named(NamedColor::BrightRed));
const GREEN: Option<Color> = Some(Color::Named(NamedColor::Green));

/// The triggers of the preset, from fixtures/room-colors/preset.json.
fn preset() -> Vec<vosh_automation::trigger::Trigger> {
    #[derive(serde::Deserialize)]
    struct PresetFile {
        triggers: Vec<vosh_automation::trigger::Trigger>,
    }
    serde_json::from_str::<PresetFile>(include_str!("../../../../fixtures/room-colors/preset.json"))
        .expect("preset.json reads")
        .triggers
}

/// How many looks at The Crossroads the screen shows.
fn crossroads_looks(h: &Harness) -> usize {
    h.screen()
        .iter()
        .filter(|r| r.trim() == CROSSROADS[2])
        .count()
}

/// Type `line` and wait for the look at The Crossroads it brings, and
/// for the prompt that ends its pulse.
async fn look_after(h: &Harness, line: &str) {
    let before = crossroads_looks(h);
    h.type_line(line).await;
    h.until(&format!("the crossroads after {line}"), |h| {
        crossroads_looks(h) > before && h.last_row().starts_with('[')
    })
    .await;
}

/// The latest look at The Crossroads shows its exits green, its army and
/// thing yellow, the werebeast you target bright red, and what follows it
/// in the pulse plain.
fn assert_colored(h: &Harness, after: &str, greeted: bool) {
    let rows = h.screen_colors();
    let at = rows
        .iter()
        .rposition(|(row, _)| row.trim() == CROSSROADS[2])
        .expect("a look at the crossroads");
    let exits = &rows[at - 3];
    assert_eq!(
        (exits.0.as_str(), exits.1),
        ("[Exits: north east south west]", GREEN),
        "the exits after {after}"
    );
    for (i, want) in CROSSROADS.iter().enumerate() {
        let (row, color) = &rows[at - 2 + i];
        assert_eq!(row.trim(), *want, "after {after}");
        let want_color = if i == 2 { BRIGHT_RED } else { YELLOW };
        assert_eq!(*color, want_color, "{want} after {after}");
    }
    let next = &rows[at + 1];
    if greeted {
        assert_eq!(next.0, GREET, "after {after}");
        assert_ne!(next.1, YELLOW, "the greeting after {after}");
    } else {
        assert!(next.0.is_empty(), "the row after the look after {after}");
    }
}

/// Walk, look and goto into The Crossroads with the game sending its room
/// packets in `order`, and with `split` each look whose packets follow
/// its people in two writes, the packets in the second. The guard keeps
/// other tests off the shared native grid, which every session output
/// also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
async fn walk_look_and_goto(order: TickOrder, split: bool) {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options {
        // An immortal, who can goto and sees each room's vnum.
        wizi: 1,
        order,
        ..Options::new(Build::New)
    })
    .await;
    {
        let mut p = h.state.selected_profile().await;
        for trigger in preset() {
            p.triggers
                .set(trigger)
                .expect("every preset pattern compiles");
        }
    }
    h.connect().await;
    h.until_shown("[Exits: south]").await;
    h.type_line("tar werebeast").await;
    h.until_shown("target: werebeast").await;
    let look_after = |line: &'static str| {
        let h = &h;
        async move {
            if split {
                h.type_line("splitroom").await;
                h.until_shown("The next look comes in two writes.").await;
            }
            look_after(h, line).await;
        }
    };

    // From the bank to the empty room east of the crossroads, then a walk
    // west into the crossroads, which holds an army, a thing and a mob.
    h.type_line("goto 6910").await;
    h.until_shown("Nearing the Crossroads [Room 6910]").await;
    look_after("west").await;
    assert_colored(&h, "west", true);

    // A look in the same room.
    look_after("look").await;
    assert_colored(&h, "look", false);

    // Out east, and an immortal's goto back.
    h.type_line("east").await;
    h.until_shown("You have explored").await;
    look_after("goto 6909").await;
    assert_colored(&h, "goto 6909", false);
    h.finish(grid).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_room_colors_after_a_walk_a_look_and_a_goto_with_the_packets_after_the_text() {
    walk_look_and_goto(TickOrder::Middle, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_room_colors_after_a_walk_a_look_and_a_goto_with_the_packets_a_read_later() {
    walk_look_and_goto(TickOrder::Middle, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_room_colors_after_a_walk_a_look_and_a_goto_with_the_packets_first() {
    walk_look_and_goto(TickOrder::First, false).await;
}
