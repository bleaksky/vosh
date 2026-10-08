//! The engine's reading of each Aabahran packet against
//! `fixtures/gmcp/aabahran/views.json`, which the webview's store tests
//! read too. The engine keeps its own copy of the packages, since
//! the session draws the prompt without the webview, so this file and
//! `src/test/aabahranViews.test.ts` hold both readings to one record.
//! No webview store reads Char.State or Room.Weather, so their records
//! are the engine's alone (`ENGINE_ONLY` in that test) until a pane
//! brings their stores back.
//!
//! Each view is what a pane or a piece shows from the packet alone, with
//! no hidden model: the vitals, the affects one row per name, the fight
//! with a flagged opponent's health and condition left out, the group
//! with a flagged roster left out, position and language, the weather,
//! the prompt settings, and the room. A view's `map` holds what the Map
//! pane's room strip reads where it differs from the engine on purpose,
//! and only the webview test reads it.

mod common;

use common::FIXTURES;
use serde_json::{json, Map, Value as Json};
use vosh_prompt::testkit::at;
use vosh_prompt::values::gmcp::Snapshot;
use vosh_prompt::{ClientValues, FieldRef, Resolved, Value, Values, Vars};

const VIEWS: &str = include_str!("../../../fixtures/gmcp/aabahran/views.json");

/// What a resolved field reads as in the record.
fn field_json(resolved: Resolved) -> Json {
    match resolved {
        Resolved::Value(Value::Num(n)) => json!(n),
        Resolved::Value(Value::Text(t)) => json!(t),
        Resolved::Absent | Resolved::Missing => Json::Null,
        other => panic!("no record reading for {other:?}"),
    }
}

/// The room as the prompt engine reads it on the new build, the only
/// build whose Room.Info feeds Exits. Exits read as direction
/// words in the game's door order.
fn room_view(msg: &vosh_protocol::gmcp::Message) -> Json {
    let mut vars = Vars::new(true);
    vars.observe(
        "Char.Prompt",
        json!({"enabled": true, "prompt": "", "fprompt": ""}),
        at(),
    );
    vars.observe(&msg.package, msg.data.clone(), at());
    let client = ClientValues::default();
    let resolver = vars.resolver(&client);
    let get = |name: &str| resolver.resolve(&FieldRef::new(name));
    let exits: Vec<&str> = match get("exits") {
        Resolved::Value(Value::Exits { letters, .. }) if letters != "none" => letters
            .split(' ')
            .map(|letter| match letter {
                "N" => "north",
                "E" => "east",
                "S" => "south",
                "W" => "west",
                "U" => "up",
                "D" => "down",
                other => panic!("no door {other}"),
            })
            .collect(),
        Resolved::Value(Value::Exits { .. }) => Vec::new(),
        other => panic!("exits read {other:?}"),
    };
    json!({
        "name": field_json(get("room")),
        "num": field_json(get("room_num")),
        "area": field_json(get("area")),
        "terrain": field_json(get("terrain")),
        "sector": field_json(get("sector")),
        "region": field_json(get("region")),
        "exits": exits,
    })
}

/// The engine's view of one packet.
fn view(file: &str, text: &str) -> Json {
    let msg = vosh_protocol::gmcp::parse(text.as_bytes()).unwrap_or_else(|e| panic!("{file}: {e}"));
    if msg.package == "Room.Info" {
        return room_view(&msg);
    }
    let mut snapshot = Snapshot::new();
    let observed = snapshot.observe(&msg.package, msg.data.clone(), at());
    // Off the Forsaken Lands rules nothing is hidden, so the resolver
    // reads each field as the packet gives it.
    let mut vars = Vars::new(false);
    vars.observe(&msg.package, msg.data.clone(), at());
    let client = ClientValues::default();
    let resolver = vars.resolver(&client);
    match msg.package.as_str() {
        "Char.Vitals" => {
            let v = snapshot.vitals().expect("a vitals packet");
            json!({
                "hp": v.hp, "maxhp": v.maxhp, "mana": v.mana, "maxmana": v.maxmana,
                "move": v.moves, "maxmove": v.maxmove, "hidden": v.hidden,
            })
        }
        "Char.Affects" => {
            let a = snapshot.affects().expect("an affects packet");
            let mut names: Vec<&str> = Vec::new();
            for row in &a.list {
                if !names.contains(&row.name.as_str()) {
                    names.push(&row.name);
                }
            }
            let rows: Vec<Json> = names
                .iter()
                .map(|name| {
                    let kind = a
                        .list
                        .iter()
                        .find(|r| r.name == *name)
                        .and_then(|r| r.kind.clone());
                    let duration = match resolver.resolve(&FieldRef::with_param("aff", *name)) {
                        Resolved::Value(Value::Ticks(n)) => json!(n),
                        Resolved::Value(Value::Flag) => Json::Null,
                        other => panic!("{file}: aff:{name} reads {other:?}"),
                    };
                    json!({"name": name, "kind": kind, "duration": duration})
                })
                .collect();
            json!({"affects": rows, "hidden": a.hidden})
        }
        "Char.Combat" => {
            let k = snapshot.combat().expect("a combat packet");
            match &k.target {
                None => Json::Null,
                Some(target) => json!({
                    "opponent": target,
                    "hp_pct": if k.hidden { None } else { k.hp_pct },
                    "condition": if k.hidden { None } else { k.condition.clone() },
                    "hidden": k.hidden,
                    "tank": k.tank.as_ref().map(|t| json!({"name": t.name, "hp_pct": t.hp_pct})),
                }),
            }
        }
        "Group.Info" => {
            let g = snapshot.group().expect("a group packet");
            if g.hidden {
                return json!({"leader": null, "members": [], "hidden": true});
            }
            let members: Vec<Json> = g
                .members
                .iter()
                .map(|m| {
                    json!({
                        "id": m.id, "name": m.name, "level": m.level, "class": m.class,
                        "hp_pct": m.hp_pct, "mana_pct": m.mana_pct, "move_pct": m.move_pct,
                        "tnl": m.tnl,
                    })
                })
                .collect();
            json!({"leader": g.leader, "members": members, "hidden": false})
        }
        "Char.State" => {
            let s = snapshot.state().expect("a state packet");
            json!({"position": s.position, "language": s.language})
        }
        "Room.Weather" => {
            let w = snapshot.weather().expect("a weather packet");
            json!({
                "sky": w.sky, "temp": w.temp,
                "unit": w.unit.map(String::from), "region": w.region,
            })
        }
        "Char.Prompt" => {
            let p = observed.prompt.expect("a prompt packet");
            json!({"enabled": p.enabled, "prompt": p.prompt, "fprompt": p.fprompt})
        }
        other => panic!("{file}: no view for {other}"),
    }
}

#[test]
fn the_engine_reads_every_packet_as_the_record_says() {
    let views: Map<String, Json> = serde_json::from_str(VIEWS).expect("views.json reads");
    let mut seen = Vec::new();
    for (file, text) in FIXTURES {
        let mut want = views
            .get(*file)
            .unwrap_or_else(|| panic!("views.json has no view of {file}"))
            .clone();
        // The Map pane's own reading is for the webview test.
        if let Some(view) = want.as_object_mut() {
            view.remove("map");
        }
        let want = &want;
        let got = view(file, text);
        assert_eq!(
            &got,
            want,
            "{file}\n{}",
            serde_json::to_string(&got).unwrap_or_default()
        );
        seen.push(*file);
    }
    // Every view names a fixture.
    for file in views.keys() {
        assert!(seen.contains(&file.as_str()), "{file} is no fixture");
    }
}
