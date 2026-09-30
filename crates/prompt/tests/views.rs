//! The engine's reading of each Aabahran packet against
//! `fixtures/gmcp/aabahran/views.json`, which the webview's store tests
//! read too (D29). The engine keeps its own copy of the packages, since
//! the session draws the prompt without the webview, so this file and
//! `src/test/aabahranViews.test.ts` hold both readings to one record.
//!
//! Each view is what a pane or a piece shows from the packet alone, with
//! no hidden model: the vitals, the affects one row per name, the fight
//! with a flagged opponent's health and condition left out, the group
//! with a flagged roster left out, position and language, the weather,
//! and the prompt settings. Room.Info has no view here, since the engine
//! reads its fields through the resolver.

mod common;

use common::{at, FIXTURES};
use serde_json::{json, Map, Value as Json};
use vosh_prompt::gmcp::Snapshot;
use vosh_prompt::{FieldRef, Resolved, Value, Values, Vars, Vosh};

const VIEWS: &str = include_str!("../../../fixtures/gmcp/aabahran/views.json");

/// The fixture files with no view, since both sides read them through
/// other paths.
const NO_VIEW: &[&str] = &["room-info.gmcp", "room-info-rhapsody.gmcp"];

/// The engine's view of one packet.
fn view(file: &str, text: &str) -> Json {
    let msg = vosh_gmcp::parse(text.as_bytes()).unwrap_or_else(|e| panic!("{file}: {e}"));
    let mut snapshot = Snapshot::new();
    let observed = snapshot.observe(&msg.package, msg.data.clone(), at());
    // Off the Forsaken Lands rules nothing is hidden, so the resolver
    // reads each field as the packet gives it.
    let mut vars = Vars::new(false);
    vars.observe(&msg.package, msg.data.clone(), at());
    let vosh = Vosh::default();
    let resolver = vars.resolver(&vosh);
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
        if NO_VIEW.contains(file) {
            assert!(!views.contains_key(*file), "{file} has a view");
            continue;
        }
        let want = views
            .get(*file)
            .unwrap_or_else(|| panic!("views.json has no view of {file}"));
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
