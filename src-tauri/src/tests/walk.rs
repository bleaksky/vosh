//! The rooms `#walk` is tested in: four rooms of Caranduin from the
//! Map.Tiles fixtures in `fixtures/gmcp/aabahran/map`, with the Room.Info
//! the game sends in each, carrying the `num` the walker reads and the
//! exits the tiles list for that room, and the Map.Tiles of the two rooms
//! the fixtures were taken in.

use serde_json::{json, Value};

/// West of the City Fountain in Caranduin, where every test starts.
pub(crate) const FOUNTAIN: i64 = 4406;
/// The Common Road, one step west of the fountain.
pub(crate) const ROAD: i64 = 4405;
/// The room west of the Common Road.
const ROAD_WEST: i64 = 4404;
/// The room north of the fountain.
const NORTH_OF_FOUNTAIN: i64 = 4631;

/// The rooms, each with the rooms its exits lead to in the game's door
/// order, as the Map.Tiles fixtures list them in `ex`.
const ROOMS: &[(i64, &[(&str, i64)])] = &[
    (
        FOUNTAIN,
        &[("north", 4631), ("south", 4633), ("west", ROAD)],
    ),
    (
        ROAD,
        &[
            ("north", 4508),
            ("east", FOUNTAIN),
            ("south", 4514),
            ("west", ROAD_WEST),
        ],
    ),
    (
        ROAD_WEST,
        &[
            ("north", 4506),
            ("east", ROAD),
            ("south", 4512),
            ("west", 4403),
        ],
    ),
    (
        NORTH_OF_FOUNTAIN,
        &[
            ("north", 4499),
            ("east", 4446),
            ("south", FOUNTAIN),
            ("west", 4510),
        ],
    ),
];

/// The exits of room `num`.
fn exits(num: i64) -> &'static [(&'static str, i64)] {
    ROOMS
        .iter()
        .find(|(room, _)| *room == num)
        .map_or(&[], |(_, exits)| exits)
}

/// The Room.Info data for room `num`: its `num` and its exits, as
/// `gmcp_send_room` writes them.
pub(crate) fn room_info(num: i64) -> Value {
    let exits: serde_json::Map<String, Value> = exits(num)
        .iter()
        .map(|(dir, to)| ((*dir).to_string(), json!(to)))
        .collect();
    json!({ "num": num, "exits": exits })
}

/// The Map.Tiles data the game sends for room `num`, for the two rooms
/// the fixtures were taken in.
pub(crate) fn map_tiles(num: i64) -> Option<Value> {
    let name = match num {
        FOUNTAIN => "caranduin-west-of-the-fountain",
        ROAD => "caranduin-the-common-road",
        _ => return None,
    };
    let path = format!(
        "{}/../fixtures/gmcp/aabahran/map/{name}.gmcp",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let msg = vosh_protocol::gmcp::parse(&raw).expect("the fixture parses");
    assert_eq!(msg.package, "Map.Tiles");
    Some(msg.data)
}
