//! The Aabahran packets in `fixtures/gmcp/aabahran` parse into the
//! package and the fields the server documents. That covers the
//! lamented tears `hidden` flag, the Char.Combat `tank` field, the
//! Char.Prompt, Char.State and Room.Weather packages, Room.Info, the
//! packets two older server builds send under lamented tears, and Snoop.

use serde_json::{json, Value};
use vosh_protocol::gmcp::{parse, Message};

macro_rules! fixture {
    ($file:literal, $package:literal) => {
        (
            $file,
            $package,
            include_str!(concat!("../../../fixtures/gmcp/aabahran/", $file)),
        )
    };
}

/// Every fixture with the package it carries.
const FIXTURES: &[(&str, &str, &str)] = &[
    fixture!("char-vitals.gmcp", "Char.Vitals"),
    fixture!("char-vitals-hidden.gmcp", "Char.Vitals"),
    fixture!("char-affects.gmcp", "Char.Affects"),
    fixture!("char-affects-hidden.gmcp", "Char.Affects"),
    fixture!("char-combat.gmcp", "Char.Combat"),
    fixture!("char-combat-hidden.gmcp", "Char.Combat"),
    fixture!("char-combat-tank.gmcp", "Char.Combat"),
    fixture!("char-combat-tank-hidden.gmcp", "Char.Combat"),
    fixture!("char-combat-end.gmcp", "Char.Combat"),
    fixture!("char-prompt.gmcp", "Char.Prompt"),
    fixture!("char-prompt-off.gmcp", "Char.Prompt"),
    fixture!("char-prompt-fight.gmcp", "Char.Prompt"),
    fixture!("char-state.gmcp", "Char.State"),
    fixture!("room-weather.gmcp", "Room.Weather"),
    fixture!("room-weather-indoors.gmcp", "Room.Weather"),
    fixture!("group-info.gmcp", "Group.Info"),
    fixture!("group-info-solo.gmcp", "Group.Info"),
    fixture!("group-info-hidden.gmcp", "Group.Info"),
    fixture!("char-vitals-zero.gmcp", "Char.Vitals"),
    fixture!("char-affects-empty.gmcp", "Char.Affects"),
    fixture!("group-info-empty.gmcp", "Group.Info"),
    fixture!("char-combat-withheld.gmcp", "Char.Combat"),
    fixture!("char-affects-lament.gmcp", "Char.Affects"),
    fixture!("group-info-own-row.gmcp", "Group.Info"),
    fixture!("char-combat-lament-older.gmcp", "Char.Combat"),
    fixture!("room-info.gmcp", "Room.Info"),
    fixture!("room-info-rhapsody.gmcp", "Room.Info"),
    fixture!("snoop-start.gmcp", "Snoop.Start"),
    fixture!("snoop-output.gmcp", "Snoop.Output"),
    fixture!("snoop-stop.gmcp", "Snoop.Stop"),
];

fn packet(file: &str) -> Message {
    let (_, _, text) = FIXTURES
        .iter()
        .find(|(name, _, _)| *name == file)
        .unwrap_or_else(|| panic!("no fixture {file}"));
    parse(text.as_bytes()).unwrap_or_else(|e| panic!("{file}: {e}"))
}

fn data(file: &str) -> Value {
    packet(file).data
}

#[test]
fn every_fixture_parses_into_its_package() {
    for (file, package, text) in FIXTURES {
        let msg = parse(text.as_bytes()).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(msg.package, *package, "{file}");
        assert!(msg.data.is_object(), "{file} carries an object");
    }
}

#[test]
fn every_fixture_in_the_folder_has_a_test() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/gmcp/aabahran");
    let mut on_disk: Vec<String> = std::fs::read_dir(dir)
        .expect("fixture folder")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "gmcp" {
                return None;
            }
            path.file_name()?.to_str().map(str::to_string)
        })
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = FIXTURES
        .iter()
        .map(|(file, _, _)| (*file).to_string())
        .collect();
    listed.sort();
    assert_eq!(on_disk, listed);
}

#[test]
fn hidden_packets_carry_the_flag_and_no_values() {
    assert_eq!(
        data("char-vitals-hidden.gmcp"),
        json!({"hp":0,"maxhp":0,"mana":0,"maxmana":0,"move":0,"maxmove":0,"hidden":true})
    );
    assert_eq!(
        data("char-affects-hidden.gmcp"),
        json!({"affects":[],"hidden":true})
    );
    assert_eq!(data("group-info-hidden.gmcp"), json!({"hidden":true}));
    let combat = data("char-combat-hidden.gmcp");
    assert_eq!(combat["target"], "a Blackwatch guard");
    assert_eq!(combat["hidden"], true);
    assert!(combat.get("condition").is_none());
    assert!(combat.get("hp_pct").is_none());
}

#[test]
fn shown_packets_carry_no_flag() {
    for file in [
        "char-vitals.gmcp",
        "char-affects.gmcp",
        "char-combat.gmcp",
        "char-combat-tank.gmcp",
        "group-info.gmcp",
        "group-info-solo.gmcp",
    ] {
        assert!(data(file).get("hidden").is_none(), "{file}");
    }
    assert_eq!(data("char-combat.gmcp")["hp_pct"], 54);
    assert_eq!(data("group-info.gmcp")["members"][0]["hp_pct"], 78);
}

#[test]
fn combat_names_the_groupmate_your_target_hits() {
    assert_eq!(
        data("char-combat-tank.gmcp")["tank"],
        json!({"name":"Tester","hp_pct":78})
    );
    // Under lamented tears the tank keeps its name and loses its health.
    let lament = data("char-combat-tank-hidden.gmcp");
    assert_eq!(lament["tank"], json!({"name":"Tester"}));
    assert_eq!(lament["hidden"], true);
    assert_eq!(data("char-combat-end.gmcp"), json!({}));
}

#[test]
fn prompt_packages_keep_their_text_raw() {
    assert_eq!(
        data("char-prompt.gmcp"),
        json!({"enabled":true,"prompt":"%n%P%C<%hhp %mm %vmv> ","fprompt":""})
    );
    assert_eq!(data("char-prompt-off.gmcp")["enabled"], false);
    // Colour codes and the trailing space stay as the player typed them.
    assert_eq!(
        data("char-prompt-fight.gmcp")["fprompt"],
        "`1%h``hp [%p] > "
    );
    assert_eq!(
        data("char-state.gmcp"),
        json!({"position":"sitting","language":"common"})
    );
    assert_eq!(
        data("room-weather.gmcp"),
        json!({"sky":"rainy","temp":60,"unit":"F","region":"Coastal North"})
    );
    assert_eq!(data("room-weather-indoors.gmcp")["sky"], "indoors");
}

#[test]
fn the_build_before_the_flag_sends_the_same_packets_without_it() {
    assert_eq!(
        data("char-vitals-zero.gmcp"),
        json!({"hp":0,"maxhp":0,"mana":0,"maxmana":0,"move":0,"maxmove":0})
    );
    assert_eq!(data("char-affects-empty.gmcp"), json!({"affects":[]}));
    // Under the song the group reads the same as no group at all.
    assert_eq!(data("group-info-empty.gmcp"), data("group-info-solo.gmcp"));
    assert_eq!(
        data("char-combat-withheld.gmcp"),
        json!({"target":"a Blackwatch guard"})
    );
}

#[test]
fn the_older_build_sends_true_values_and_names_the_song() {
    let affects = data("char-affects-lament.gmcp");
    assert!(affects.get("hidden").is_none());
    let song = &affects["affects"][1];
    assert_eq!(song["kind"], "song");
    assert_eq!(song["name"], "lamented tears");
    // Your own row stays in the roster, with its true health.
    let roster = data("group-info-own-row.gmcp");
    assert!(roster.get("hidden").is_none());
    assert_eq!(roster["members"][0]["name"], "Tester");
    assert_eq!(roster["members"][0]["hp_pct"], 64);
    let combat = data("char-combat-lament-older.gmcp");
    assert!(combat.get("hidden").is_none());
    assert_eq!(combat["hp_pct"], 41);
    assert_eq!(combat["condition"], "big nasty wounds");
}

#[test]
fn room_info_lists_exits_by_direction() {
    let bank = data("room-info.gmcp");
    assert_eq!(bank["name"], "The Bank of Aabahran");
    assert_eq!(bank["climate"], "Temperate");
    assert_eq!(bank["exits"], json!({"south":5233}));
    // Rhapsody of delusion sends a room with every exit and no climate.
    let fake = data("room-info-rhapsody.gmcp");
    assert_eq!(fake["exits"].as_object().map(serde_json::Map::len), Some(6));
    assert!(fake.get("climate").is_none());
}
