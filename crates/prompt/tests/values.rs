//! The resolver against the Aabahran packets in `fixtures/gmcp/aabahran`:
//! sources and their order, freshness, the fields the new build adds, and
//! the catalog. The hidden model has its own tests in `hidden.rs`.

mod common;

use std::collections::BTreeMap;

use common::{capture, client, draw, draw_with, feed, num, packet, resolve, text};
use serde_json::json;
use vosh_prompt::aabahran::codes::Position;
use vosh_prompt::testkit::designs::{DETAILED, JAMES};
use vosh_prompt::testkit::now;
use vosh_prompt::values::format::tank_bar_cells;
use vosh_prompt::values::{is_sourced, known, Tick, CATALOG};
use vosh_prompt::{
    render_str, ClientValues, FieldRef, MapValues, RenderOptions, Resolved, Value, Values, Vars,
};

// ---------------------------------------------------------------------
// Sources, their order and freshness
// ---------------------------------------------------------------------

#[test]
fn a_fresh_capture_wins_and_gmcp_fills_in_after_the_pulse() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    capture(&mut vars, &[("hp", "800"), ("maxhp", "900")]);
    assert_eq!(draw(&vars, "%hp/%{maxhp} %mana"), "800/900 760");
    // The next pulse starts with Char.Vitals, and no prompt followed.
    feed(&mut vars, "char-vitals.gmcp");
    assert_eq!(draw(&vars, "%hp/%{maxhp} %mana"), "850/900 760");
    assert!(vars.prompt_vars().is_empty());
}

#[test]
fn a_script_value_beats_the_capture_for_one_pulse() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    capture(&mut vars, &[("hp", "800")]);
    vars.set_script("hp", "700");
    assert_eq!(draw(&vars, "%hp"), "700");
    feed(&mut vars, "char-vitals.gmcp");
    assert_eq!(draw(&vars, "%hp"), "850");
}

#[test]
fn a_name_only_scripts_supply_keeps_its_value() {
    let mut vars = Vars::new(true);
    vars.set_script("mood", "grim");
    for _ in 0..3 {
        feed(&mut vars, "char-vitals.gmcp");
    }
    assert_eq!(draw(&vars, "%mood"), "grim");
    assert_eq!(
        vars.prompt_vars().get("mood").map(String::as_str),
        Some("grim")
    );
    // A script can still read a gauge the way the first renderer did.
    vars.set_script("sp", "40");
    vars.set_script("maxsp", "80");
    assert_eq!(draw(&vars, "%pct_sp%%"), "50%");
    vars.switch_profile(true);
    assert_eq!(resolve(&vars, "mood"), Resolved::Unknown);
    vars.set_script("mood", "grim");
    vars.disconnect();
    assert_eq!(resolve(&vars, "mood"), Resolved::Unknown);
}

#[test]
fn a_script_value_for_a_name_only_vosh_supplies_keeps_its_value() {
    // Neither the capture nor GMCP supplies these, so a script value
    // lasts until a script changes it.
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    for (name, value) in [
        ("target", "orc"),
        ("profile", "Healer"),
        ("tick", "14"),
        ("mood", "grim"),
    ] {
        vars.set_script(name, value);
    }
    for _ in 0..3 {
        feed(&mut vars, "char-vitals.gmcp");
    }
    assert_eq!(
        draw(&vars, "%target|%profile|%tick|%mood"),
        "orc|Healer|14|grim"
    );
    let pv = vars.prompt_vars();
    let keys: Vec<&str> = pv.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["mood", "profile", "target", "tick"]);
    // A name the capture or GMCP supplies lasts one pulse, the raw
    // prompt and the immortal prefix among them.
    vars.set_script("raw", "<1020hp>");
    vars.set_script("wizi", "60");
    vars.set_script("hp", "700");
    assert_eq!(draw(&vars, "%raw %wizi %hp"), "<1020hp> 60 700");
    feed(&mut vars, "char-vitals.gmcp");
    assert_eq!(resolve(&vars, "raw"), Resolved::Missing);
    assert_eq!(resolve(&vars, "wizi"), Resolved::Missing);
    assert_eq!(draw(&vars, "%hp"), "850");
    // A profile switch clears them all.
    vars.switch_profile(true);
    assert_eq!(resolve(&vars, "target"), Resolved::Absent);
    assert!(vars.prompt_vars().is_empty());

    for name in ["target", "tar", "profile", "tick", "time", "date", "mood"] {
        assert!(!is_sourced(name), "{name}");
    }
    for name in [
        "hp", "mhp", "raw", "wizi", "afk", "tank_pct", "exits", "gold",
    ] {
        assert!(is_sourced(name), "{name}");
    }
}

#[test]
fn vosh_knows_the_catalog_its_spellings_and_the_percents() {
    for name in [
        "hp", "maxhp", "mhp", "max_move", "hp_pct", "tank_bar", "wizi", "fight", "slot10", "moon2",
        "target", "tar", "tick",
    ] {
        assert!(known(name), "{name}");
    }
    // A name only a pattern or a script fills.
    for name in ["h", "mh", "health", "HP", "aff", ""] {
        assert!(!known(name), "{name}");
    }
}

#[test]
fn a_script_value_comes_first_for_every_field() {
    // A game without Char.Status, where a trigger that gags the prompt
    // reads each of these from the prompt itself.
    let values = [
        ("name", "Tester"),
        ("race", "elf"),
        ("class", "warrior"),
        ("level", "50"),
        ("bank", "5000"),
        ("trains", "3"),
        ("pracs", "12"),
        ("cabal", "Nexus"),
        ("leader", "Ketterly"),
        ("group_size", "3"),
        ("group_low", "Iskra 45"),
        ("missing", "sanctuary"),
        ("terrain", "forest"),
        ("sector", "3"),
        ("region_num", "2"),
        ("people", "2"),
        ("things", "1"),
        ("day", "12"),
        ("month", "3"),
        ("year", "812"),
        ("sun", "dark"),
        ("sky", "cloudy"),
        ("tick", "14"),
        ("profile", "Healer"),
        ("opponent_hp", "60"),
        ("raw", "<1020hp>"),
    ];
    let mut vars = Vars::new(false);
    let mut first = BTreeMap::new();
    for (name, value) in values {
        vars.set_script(name, value);
        first.insert(name.to_string(), value.to_string());
    }
    // Each prints what the first renderer printed.
    let template = values
        .iter()
        .map(|(name, _)| format!("%{{{name}}}"))
        .collect::<Vec<_>>()
        .join("|");
    let legacy = render_str(
        &template,
        &MapValues::new(&first, now()),
        RenderOptions::default(),
    );
    assert_eq!(draw(&vars, &template), legacy.plain);
    // And each keeps the formats of its kind.
    assert_eq!(
        draw(
            &vars,
            "Lv %level t%tick %{tick:unit} %{bank:grouped} %{opponent_hp:pct}%%"
        ),
        "Lv 50 t14 14s 5,000 60%"
    );
    // Flags read as flags, and a zero count is Absent.
    for (name, value) in [
        ("near", "1"),
        ("eclipse", "yes"),
        ("triad", "0"),
        ("people", "0"),
    ] {
        vars.set_script(name, value);
    }
    assert_eq!(
        draw(
            &vars,
            "%{if:near}near%{end}%{if:eclipse} eclipse%{end}%{if:triad} triad%{end}%{if:people} people%{end}"
        ),
        "near eclipse"
    );
    assert_eq!(resolve(&vars, "people"), Resolved::Absent);

    // A script value beats the packet for its pulse, and Vosh's own
    // values after that.
    let mut vars = Vars::new(true);
    packet(
        &mut vars,
        "Char.Status",
        json!({"name":"Tester","level":50,"race":"elf","class":"warrior"}),
    );
    feed(&mut vars, "char-vitals.gmcp");
    vars.set_script("level", "51");
    vars.set_script("tick", "14");
    let mut timed = client();
    timed.tick = Some(Tick {
        remaining: 30,
        interval: Some(60),
        since: Some(30),
    });
    timed.profile = Some("Default".to_string());
    assert_eq!(
        draw_with(&vars, &timed, "%level %tick %profile"),
        "51 14 Default"
    );
    feed(&mut vars, "char-vitals.gmcp");
    assert_eq!(draw_with(&vars, &timed, "%level"), "50");
}

#[test]
fn a_stale_capture_name_is_missing_not_unknown() {
    // Another game, where each send starts a pulse.
    let mut vars = Vars::new(false);
    capture(&mut vars, &[("n1", "42")]);
    assert_eq!(resolve(&vars, "n1"), num(42));
    assert!(vars.on_send());
    assert_eq!(resolve(&vars, "n1"), Resolved::Missing);
    assert_eq!(resolve(&vars, "n2"), Resolved::Unknown);
    assert_eq!(draw(&vars, "[%n1] %n2"), "[] %n2");
}

#[test]
fn percent_only_prompts_feed_pct_with_no_max() {
    let mut vars = Vars::new(false);
    capture(&mut vars, &[("hp", "300"), ("hp_pct", "29")]);
    assert_eq!(draw(&vars, "%pct_hp%% %hp_bar:10"), "29% ███░░░░░░░");
    assert_eq!(
        resolve(&vars, "hp"),
        Resolved::Value(Value::Gauge {
            cur: 300,
            max: None,
            pct: Some(29),
        })
    );
    // A known max wins over the game's own percent.
    capture(
        &mut vars,
        &[("hp", "300"), ("maxhp", "1020"), ("hp_pct", "10")],
    );
    assert_eq!(draw(&vars, "%pct_hp"), "29");
}

#[test]
fn numbers_with_a_decimal_point_draw_as_gauges() {
    // Some games write health with a decimal point, and the first
    // renderer drew a percent, a bar and a color by how full from it.
    let mut vars = Vars::new(false);
    vars.set_script("hp", "1.5");
    vars.set_script("maxhp", "3");
    let client = client();
    let template = "%pct_hp %c_hp%hp%c_default %hp_bar:4 %hp/%{maxhp}";
    let drawn = render_str(template, &vars.resolver(&client), RenderOptions::default());
    assert_eq!(drawn.plain, "50 1.5 ██░░ 1.5/3");
    // Half full is yellow, in the text and in the bar.
    assert!(
        drawn.ansi.contains("\x1b[33m1.5\x1b[39m"),
        "{:?}",
        drawn.ansi
    );
    assert!(drawn.ansi.contains("\x1b[33m██"), "{:?}", drawn.ansi);
    // The first renderer's reading of the same values agrees.
    let first: BTreeMap<String, String> = [("hp", "1.5"), ("maxhp", "3")]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let legacy = render_str(
        template,
        &MapValues::new(&first, now()),
        RenderOptions::default(),
    );
    assert_eq!(drawn.ansi, legacy.ansi);
    // A decimal max, the game's bands, and a name no catalog entry has.
    vars.set_script("maxhp", "2.5");
    vars.set_script("sp", "12.5");
    vars.set_script("maxsp", "50");
    assert_eq!(
        draw(&vars, "%pct_hp %{hp:max} %{maxhp} %pct_sp%% %sp_bar:4"),
        "60 2.5 2.5 25% █░░░"
    );
    let banded = render_str(
        "%{c:hp:game}%hp",
        &vars.resolver(&client),
        RenderOptions::default(),
    );
    assert_eq!(banded.ansi, "\x1b[39m1.5\x1b[0m");
    // Formats for whole numbers leave a decimal alone.
    assert_eq!(draw(&vars, "%{hp:grouped}"), "%{hp:grouped}");
}

#[test]
fn the_game_bands_divide_as_the_server_does() {
    let mut vars = Vars::new(true);
    packet(
        &mut vars,
        "Char.Vitals",
        json!({"hp":414,"maxhp":1020,"mana":1,"maxmana":1,"move":1,"maxmove":1}),
    );
    let client = client();
    let drawn = render_str(
        "%{c:hp:game}%hp%c_reset %pct_hp",
        &vars.resolver(&client),
        RenderOptions::default(),
    );
    // 414 of 1020 is 40.6 percent. The game divides to 40, which is bold
    // yellow, where the pct format rounds to 41.
    assert_eq!(drawn.ansi, "\x1b[1;33m414\x1b[0m 41\x1b[0m");
    // With only %K, the band reads the game's own percent.
    let mut other = Vars::new(false);
    capture(&mut other, &[("hp", "300"), ("hp_pct", "20")]);
    let drawn = render_str(
        "%{c:hp:game}%hp",
        &other.resolver(&client),
        RenderOptions::default(),
    );
    assert_eq!(drawn.ansi, "\x1b[31m300\x1b[0m");
}

#[test]
fn prompt_vars_carry_fresh_values_and_drop_stale_ones() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    capture(&mut vars, &[("hp", "800"), ("maxhp", "900")]);
    vars.set_script("hp", "700");
    vars.set_script("mood", "grim");
    let pv = vars.prompt_vars();
    assert_eq!(pv.get("hp").map(String::as_str), Some("700"));
    assert_eq!(pv.get("maxhp").map(String::as_str), Some("900"));
    assert_eq!(pv.get("mood").map(String::as_str), Some("grim"));
    feed(&mut vars, "char-vitals.gmcp");
    let left = vars.prompt_vars();
    let keys: Vec<&str> = left.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["mood"]);
}

#[test]
fn the_legacy_template_draws_as_the_first_renderer_did() {
    let full: BTreeMap<String, String> = [
        ("hp", "300"),
        ("maxhp", "1020"),
        ("mana", "400"),
        ("maxmana", "800"),
        ("move", "100"),
        ("maxmove", "930"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
    .collect();
    let first = render_str(
        JAMES,
        &MapValues::new(&full, now()),
        RenderOptions::default(),
    );
    for forsaken in [true, false] {
        let mut vars = Vars::new(forsaken);
        for (k, v) in &full {
            vars.set_script(k, v);
        }
        let client = client();
        let drawn = render_str(JAMES, &vars.resolver(&client), RenderOptions::default());
        assert_eq!(drawn.ansi, first.ansi, "forsaken {forsaken}");
    }
}

#[test]
fn disagreements_are_counted_after_the_mappings() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "char-state.gmcp");
    packet(
        &mut vars,
        "Char.State",
        json!({"position":"meditate","language":"Thsu'ul"}),
    );
    feed(&mut vars, "room-weather.gmcp");
    let agree = capture(
        &mut vars,
        &[
            ("hp", "850"),
            ("maxhp", "900"),
            ("pos", ""),
            ("lang", "thsu'ul"),
            ("weather", "rainy"),
            ("temp", "60"),
            ("region", "Coastal North"),
        ],
    );
    assert!(agree.is_empty(), "{agree:?}");
    let disagree = capture(&mut vars, &[("hp", "800"), ("pos", "std")]);
    assert_eq!(disagree, vec!["hp", "pos"]);
    assert_eq!(vars.disagreements(), 2);
}

// ---------------------------------------------------------------------
// Other games
// ---------------------------------------------------------------------

#[test]
fn other_games_read_a_max_of_zero_as_absent() {
    let mut vars = Vars::new(false);
    packet(
        &mut vars,
        "Char.Vitals",
        json!({"hp":120,"maxhp":150,"mana":0,"maxmana":0,"move":80,"maxmove":100}),
    );
    assert_eq!(resolve(&vars, "mana"), Resolved::Absent);
    assert_eq!(resolve(&vars, "maxmana"), Resolved::Absent);
    assert_eq!(
        draw(&vars, "%hp/%{maxhp} [%mana/%{maxmana}] %pct_mana%%"),
        "120/150 [] "
    );
    // The same from a capture.
    capture(&mut vars, &[("mana", "0"), ("maxmana", "0")]);
    assert_eq!(resolve(&vars, "mana"), Resolved::Absent);
    assert!(!vars.new_build());
    feed(&mut vars, "char-prompt.gmcp");
    assert!(!vars.new_build());
}

#[test]
fn pulses_follow_sends_until_the_game_sends_char_vitals() {
    let mut vars = Vars::new(false);
    capture(&mut vars, &[("hp", "90")]);
    assert!(vars.on_send());
    assert_eq!(resolve(&vars, "hp"), Resolved::Missing);
    capture(&mut vars, &[("hp", "91")]);
    feed(&mut vars, "char-vitals.gmcp");
    capture(&mut vars, &[("hp", "92")]);
    // Once Char.Vitals has come, a send no longer ends the capture.
    assert!(!vars.on_send());
    assert_eq!(draw(&vars, "%hp"), "92");
}

// ---------------------------------------------------------------------
// Char.Prompt and the new build sign
// ---------------------------------------------------------------------

#[test]
fn the_first_char_prompt_sets_the_new_build() {
    let mut vars = Vars::new(true);
    assert!(!vars.new_build());
    feed(&mut vars, "char-prompt.gmcp");
    assert!(vars.new_build());
    let first = vars.gmcp().char_prompt().cloned().expect("a prompt");
    assert!(first.at_login && first.enabled);
    assert_eq!(first.prompt, "%n%P%C<%hhp %mm %vmv> ");
    feed(&mut vars, "char-prompt-off.gmcp");
    let off = vars.gmcp().char_prompt().cloned().expect("a prompt");
    assert!(!off.at_login && !off.enabled);
    feed(&mut vars, "char-prompt-fight.gmcp");
    assert_eq!(
        vars.gmcp().char_prompt().map(|p| p.fprompt.as_str()),
        Some("`1%h``hp [%p] > ")
    );
}

#[test]
fn a_profile_switch_keeps_the_snapshot_and_a_disconnect_clears_it() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "room-info.gmcp");
    capture(&mut vars, &[("hp", "800")]);
    vars.set_script("mood", "grim");
    vars.switch_profile(true);
    assert!(vars.new_build());
    assert_eq!(draw(&vars, "%hp %exits"), "850 S");
    assert_eq!(resolve(&vars, "mood"), Resolved::Unknown);
    vars.disconnect();
    assert!(!vars.new_build());
    assert_eq!(resolve(&vars, "hp"), Resolved::Missing);
    assert_eq!(vars.gmcp().pulse(), 0);
}

// ---------------------------------------------------------------------
// The new build's sources
// ---------------------------------------------------------------------

#[test]
fn position_from_char_state_and_the_prompt_agree_through_the_table() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    feed(&mut vars, "char-state.gmcp");
    assert_eq!(
        draw(&vars, "%pos %{pos:game} %{pos:word}"),
        "sit sit sitting"
    );
    packet(
        &mut vars,
        "Char.State",
        json!({"position":"meditate","language":"common"}),
    );
    assert_eq!(draw(&vars, "[%{pos:game}] %{pos:word}"), "[] meditate");
    feed(&mut vars, "char-vitals.gmcp");
    // `%S` printed nothing, which is meditating.
    capture(&mut vars, &[("pos", "")]);
    assert_eq!(
        resolve(&vars, "pos"),
        Resolved::Value(Value::Position(Position::Meditate))
    );
    capture(&mut vars, &[("pos", "fgt")]);
    assert_eq!(draw(&vars, "%pos %{pos:word}"), "fgt fighting");
}

#[test]
fn language_takes_char_state_spelling_when_the_prompt_agrees() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    packet(
        &mut vars,
        "Char.State",
        json!({"position":"standing","language":"Thsu'ul"}),
    );
    assert_eq!(draw(&vars, "%lang %{lang:game}"), "Thsu'ul thsu'ul");
    feed(&mut vars, "char-vitals.gmcp");
    capture(&mut vars, &[("lang", "thsu'ul")]);
    assert_eq!(
        resolve(&vars, "lang"),
        Resolved::Value(Value::Lang("Thsu'ul".to_string()))
    );
    capture(&mut vars, &[("lang", "orcish")]);
    assert_eq!(draw(&vars, "%lang"), "orcish");
    // A switched immortal sends no language.
    packet(
        &mut vars,
        "Char.State",
        json!({"position":"standing","language":""}),
    );
    feed(&mut vars, "char-vitals.gmcp");
    assert_eq!(resolve(&vars, "lang"), Resolved::Absent);
}

#[test]
fn weather_temperature_and_region_come_from_room_weather_first() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    feed(&mut vars, "room-info.gmcp");
    // Before Room.Weather, the region is Room.Info's climate.
    assert_eq!(
        draw(&vars, "%region %climate %region_num"),
        "Temperate Temperate 0"
    );
    assert_eq!(resolve(&vars, "weather"), Resolved::Missing);
    feed(&mut vars, "room-weather.gmcp");
    assert_eq!(
        draw(&vars, "%weather %{temp:unit} %temp %region"),
        "rainy 60°F 60 Coastal North"
    );
    feed(&mut vars, "room-weather-indoors.gmcp");
    assert_eq!(
        draw(&vars, "%weather %{temp:unit} %region"),
        "indoors 18°C Temperate"
    );
    // A fresh %w, %W and %G win, and the unit still comes from Room.Weather.
    feed(&mut vars, "char-vitals.gmcp");
    capture(
        &mut vars,
        &[("temp", "19"), ("weather", "cloudy"), ("region", "Desert")],
    );
    assert_eq!(
        draw(&vars, "%weather %{temp:unit} %region"),
        "cloudy 19°C Desert"
    );
    // The rhapsody room sends no climate.
    let mut fake = Vars::new(true);
    feed(&mut fake, "room-info-rhapsody.gmcp");
    assert_eq!(resolve(&fake, "region"), Resolved::Absent);
}

#[test]
fn tank_and_tank_health_take_a_fresh_capture_first() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "char-combat-tank.gmcp");
    assert_eq!(
        draw(&vars, "%tank %tank_hp %{tank_hp:game}"),
        "Tester 78 [===|===|===|=--]"
    );
    // The prompt names who the tank is when Char.Combat reads someone.
    capture(&mut vars, &[("tank", "Brask"), ("tank_pct", "40")]);
    assert_eq!(draw(&vars, "%tank %tank_hp"), "Brask 40");
    // A %P bar that Char.Combat's percent does not fill reads back to the
    // highest percent that fills as many cells, since a fresh capture wins.
    capture(
        &mut vars,
        &[("tank", "Brask"), ("tank_bar", "===|=--|---|---")],
    );
    assert_eq!(
        draw(&vars, "%tank_hp %{tank_hp:game}"),
        "33 [===|=--|---|---]"
    );
    // Out of the fight there is no tank on the new build.
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "char-combat.gmcp");
    assert_eq!(resolve(&vars, "tank"), Resolved::Absent);
    feed(&mut vars, "char-combat-end.gmcp");
    assert_eq!(resolve(&vars, "tank_hp"), Resolved::Absent);
    assert_eq!(resolve(&vars, "fight"), Resolved::Absent);
    // An older build sends no tank object, so Vosh cannot tell.
    let mut older = Vars::new(true);
    feed(&mut older, "char-combat.gmcp");
    assert_eq!(resolve(&older, "tank"), Resolved::Missing);
    assert_eq!(resolve(&older, "fight"), Resolved::Value(Value::Flag));
}

#[test]
fn a_p_bar_takes_char_combats_exact_percent_when_it_fills_the_same_cells() {
    // Char.Combat goes out on the same pulse as the prompt, with the
    // percent %P was drawn from, so within the bar's twelfth the tank's
    // health is that exact percent.
    let bar = |pct: i64| {
        let cells = tank_bar_cells(pct).map(|full| if full { '=' } else { '-' });
        cells
            .chunks(3)
            .map(|c| c.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("|")
    };
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "char-combat-tank.gmcp");
    capture(&mut vars, &[("tank", "Tester"), ("tank_bar", &bar(78))]);
    assert_eq!(
        draw(&vars, "%tank_hp %{tank_hp:game} %{tank_hp:bar:10}"),
        "78 [===|===|===|=--] ████████░░"
    );
    for pct in 0..=100 {
        packet(
            &mut vars,
            "Char.Combat",
            json!({"target":"a Blackwatch guard","condition":"awful","hp_pct":10,
                   "tank":{"name":"Tester","hp_pct":pct}}),
        );
        capture(&mut vars, &[("tank", "Tester"), ("tank_bar", &bar(pct))]);
        assert_eq!(
            resolve(&vars, "tank_hp"),
            Resolved::Value(Value::TankHp(pct)),
            "{pct}"
        );
    }
    // With no percent from Char.Combat the bar reads back alone, as on
    // an older build, whose Char.Combat names no tank.
    let mut older = Vars::new(true);
    feed(&mut older, "char-combat.gmcp");
    capture(&mut older, &[("tank", "Tester"), ("tank_bar", &bar(78))]);
    assert_eq!(
        draw(&older, "%tank_hp %{tank_hp:game}"),
        "83 [===|===|===|=--]"
    );
}

#[test]
fn exits_come_from_room_info_only_on_the_new_build() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "room-info.gmcp");
    assert_eq!(resolve(&vars, "exits"), Resolved::Missing);
    assert_eq!(draw(&vars, "%{if:exits}[%exits]%{end}"), "");
    feed(&mut vars, "char-prompt.gmcp");
    assert_eq!(draw(&vars, "%exits %{exits:game}"), "S [Exits: S]");
    // A fresh %e wins, since only it shows doors, traps, blindness and mist.
    feed(&mut vars, "char-vitals.gmcp");
    capture(&mut vars, &[("exits", "[Exits: N E (S) W]")]);
    assert_eq!(
        draw(&vars, "%exits %{exits:game}"),
        "N E (S) W [Exits: N E (S) W]"
    );
    capture(&mut vars, &[("exits", "[Exits: --- ]")]);
    assert_eq!(draw(&vars, "%exits"), "---");
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "room-info-rhapsody.gmcp");
    assert_eq!(draw(&vars, "%exits"), "N E S W U D");
    packet(&mut vars, "Room.Info", json!({"num": 1, "exits": {}}));
    assert_eq!(draw(&vars, "%{exits:game}"), "[Exits: none]");
}

#[test]
fn the_gate_pieces_draw_from_the_new_build_packets() {
    let mut vars = Vars::new(true);
    for file in [
        "char-prompt.gmcp",
        "char-vitals.gmcp",
        "char-combat-tank.gmcp",
        "char-state.gmcp",
        "room-weather.gmcp",
        "room-info.gmcp",
    ] {
        feed(&mut vars, file);
    }
    packet(
        &mut vars,
        "Char.Worth",
        json!({"gold":1250,"bank":5000,"exp":125_000,"tnl":1250,"trains":3,"practices":12,"cps":40,"rps":7,"cabal":"none"}),
    );
    packet(
        &mut vars,
        "World.Moons",
        json!({"moons":[
            {"name":"Lysenties","active":true,"phase":4,"phase_name":"full and whole"},
            {"name":"Nercuros","active":false,"phase":2,"phase_name":"half-lit and growing"},
            {"name":"Dyphrities","active":true,"phase":7,"phase_name":"a thin crescent, fading"}
        ],"eclipse":false,"triad":false,"near_alignment":true}),
    );
    let client = ClientValues {
        tick: Some(Tick {
            remaining: 14,
            interval: Some(60),
            since: Some(46),
        }),
        ..client()
    };
    assert_eq!(
        draw_with(
            &vars,
            &client,
            "%gold %opponent|%{moon1:game} %{moon2:game} %{moon3:word}|%tick %pos %lang %weather %{temp:unit} %region|%tank %{tank_hp:game}|%exits"
        ),
        "1250 a Blackwatch guard|FUL - waning crescent|14 sit common rainy 60°F Coastal North|Tester [===|===|===|=--]|S"
    );
    assert_eq!(
        draw_with(
            &vars,
            &client,
            "%{moon1:name}%{if:near} near%{end}%{if:eclipse} eclipse%{end}[%cabal]"
        ),
        "full and whole near[]"
    );
    assert_eq!(
        draw_with(
            &vars,
            &client,
            "%{gold:grouped} %{exp:short} %{cp} %{rp} %pracs"
        ),
        "1,250 125k 40 7 12"
    );
    // The forms the old TinTin prompt wrote: the tick counting up, the
    // hour as 3PM and gold in thousands.
    assert_eq!(
        draw_with(
            &vars,
            &client,
            "%{tick:since} %{tick:unit} %{gold:thousands} %{exp:thousands}"
        ),
        "46s 14s 1.2K 125.0K"
    );
    // Labels for the moons come from the packet.
    assert_eq!(
        vars.resolver(&client).label(&FieldRef::new("moon2")),
        "Nercuros"
    );
}

#[test]
fn detailed_out_of_a_fight_draws_one_line() {
    let mut vars = Vars::new(true);
    for file in [
        "char-prompt.gmcp",
        "char-vitals.gmcp",
        "char-combat-end.gmcp",
        "room-info.gmcp",
        "char-affects.gmcp",
    ] {
        feed(&mut vars, file);
    }
    packet(&mut vars, "Char.Worth", json!({"gold": 1250}));
    let client = ClientValues {
        tick: Some(Tick {
            remaining: 14,
            interval: Some(60),
            since: Some(46),
        }),
        tracked: vec!["bless".to_string(), "armor".to_string()],
        ..client()
    };
    assert_eq!(
        draw_with(&vars, &client, DETAILED),
        "850/900hp 760/820mn 250/250mv tick 14 [S] 1250g"
    );
    // Tracked affects that are off show, in a fight too.
    let client = ClientValues {
        tracked: vec![
            "bless".to_string(),
            "sanctuary".to_string(),
            "haste".to_string(),
        ],
        ..client
    };
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "char-combat.gmcp");
    assert_eq!(
        draw_with(&vars, &client, DETAILED),
        "a Blackwatch guard █████░░░░░ 54% quite a few wounds\n850/900hp 760/820mn 250/250mv tick 14 [S] 1250g 2 missing"
    );
    assert_eq!(
        draw_with(&vars, &client, "%{missing:names}"),
        "sanctuary, haste"
    );
    // Without the new build no exits show unless the prompt has %e.
    let mut older = Vars::new(true);
    for file in ["char-vitals.gmcp", "char-combat-end.gmcp", "room-info.gmcp"] {
        feed(&mut older, file);
    }
    packet(&mut older, "Char.Worth", json!({"gold": 1250}));
    let client = ClientValues {
        tracked: Vec::new(),
        ..client
    };
    assert_eq!(
        draw_with(&older, &client, DETAILED),
        "850/900hp 760/820mn 250/250mv tick 14 1250g"
    );
}

// ---------------------------------------------------------------------
// Group members and paths
// ---------------------------------------------------------------------

#[test]
fn members_are_found_by_name_or_id() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "group-info.gmcp");
    assert_eq!(
        resolve(&vars, "member_hp:Tester"),
        Resolved::Value(Value::Pct(78))
    );
    assert_eq!(
        resolve(&vars, "member_hp:a_loyal_wolf"),
        Resolved::Value(Value::Pct(91))
    );
    assert_eq!(
        resolve(&vars, "member_mana:id=1769401002"),
        Resolved::Value(Value::Pct(100))
    );
    assert_eq!(resolve(&vars, "member_class:A_Loyal_Wolf"), text("mob"));
    assert_eq!(resolve(&vars, "member_tnl:tester"), num(1250));
    assert_eq!(resolve(&vars, "member_level:id=1769388810"), num(50));
    assert_eq!(resolve(&vars, "member_hp:nobody"), Resolved::Absent);
    assert_eq!(resolve(&vars, "member_hp:id=5"), Resolved::Absent);
    assert_eq!(
        draw(&vars, "%leader %group_size %{group_size:names}"),
        "Tester 2 Tester, a loyal wolf"
    );
    // The lowest member leaves you out once Char.Status names you.
    assert_eq!(draw(&vars, "%group_low"), "Tester 78%");
    packet(
        &mut vars,
        "Char.Status",
        json!({"name":"Tester","level":50,"race":"human","class":"warrior"}),
    );
    assert_eq!(
        draw(&vars, "%group_low %{group_low:name} %{group_low:pct}"),
        "a loyal wolf 91% a loyal wolf 91"
    );
    assert_eq!(
        draw(&vars, "%name %level %race %class"),
        "Tester 50 human warrior"
    );
    assert_eq!(
        vars.resolver(&client())
            .label(&FieldRef::with_param("member_hp", "a_loyal_wolf")),
        "a loyal wolf health"
    );
}

#[test]
fn paths_and_affects_reach_any_packet() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-affects.gmcp");
    feed(&mut vars, "group-info.gmcp");
    packet(
        &mut vars,
        "Imm.Queues",
        json!({"bugs": 3, "journals": {"unread": 2}}),
    );
    assert_eq!(
        resolve(
            &vars,
            "gmcp:Char.Affects.affects[name=bagatelle_of_bravado].modifier"
        ),
        num(3)
    );
    assert_eq!(
        resolve(&vars, "gmcp:char.affects.affects[1].location"),
        text("ac")
    );
    assert_eq!(
        resolve(&vars, "gmcp:Group.Info.members[id=1769388810].tnl"),
        num(1250)
    );
    assert_eq!(resolve(&vars, "gmcp:Imm.Queues.journals.unread"), num(2));
    assert_eq!(
        resolve(&vars, "gmcp:Char.Affects.affects[name=haste].level"),
        Resolved::Absent
    );
    assert_eq!(resolve(&vars, "gmcp:World.Time.hour"), Resolved::Missing);
    assert_eq!(resolve(&vars, "queue:bugs"), num(3));
    assert_eq!(resolve(&vars, "queue:journals"), Resolved::Absent);
    assert_eq!(
        resolve(&vars, "aff:Bless"),
        Resolved::Value(Value::Ticks(6))
    );
    assert_eq!(resolve(&vars, "aff:sanctuary"), Resolved::Absent);
    assert_eq!(draw(&vars, "%{aff:armor:on}%{aff:haste:off}"), "armorhaste");
    // One row per thing an affect modifies. The longest row wins, and a
    // permanent one outlasts them all, as in the Affects pane.
    packet(
        &mut vars,
        "Char.Affects",
        json!({"affects":[
            {"name":"giant strength","duration":3,"location":"strength","modifier":2},
            {"name":"giant strength","duration":9,"location":"hitroll","modifier":1},
            {"name":"giant strength","location":"damroll","modifier":1},
            {"name":"fly","duration":4},
            {"name":"fly","duration":-1},
            {"name":"fly","duration":12}
        ]}),
    );
    assert_eq!(
        resolve(&vars, "aff:giant_strength"),
        Resolved::Value(Value::Ticks(9))
    );
    assert_eq!(resolve(&vars, "aff:fly"), Resolved::Value(Value::Ticks(-1)));
    assert_eq!(
        vars.resolver(&client()).label(&FieldRef::with_param(
            "gmcp",
            "Char.Affects.affects[name=bless].level"
        )),
        "level"
    );
}

#[test]
fn vosh_supplies_the_tick_target_clock_and_profile() {
    let vars = Vars::new(true);
    let full = ClientValues {
        tick: Some(Tick {
            remaining: 14,
            interval: Some(60),
            since: Some(46),
        }),
        target: Some("a Blackwatch guard".to_string()),
        profile: Some("Default".to_string()),
        ..client()
    };
    assert_eq!(
        draw_with(
            &vars,
            &full,
            "%{tick:unit} %tar|%target %profile %{time:hm} %{date:md}"
        ),
        "14s a Blackwatch guard|a Blackwatch guard Default 08:42 Sep 29"
    );
    let none = client();
    assert_eq!(
        vars.resolver(&none).resolve(&FieldRef::new("target")),
        Resolved::Absent
    );
    assert_eq!(
        vars.resolver(&none).resolve(&FieldRef::new("tick")),
        Resolved::Missing
    );
}

#[test]
fn prompt_only_fields_read_the_capture() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    capture(
        &mut vars,
        &[
            ("wizi", "60"),
            ("incog", ""),
            ("stallion", "M"),
            ("slot1", "12"),
            ("slot10", "~"),
            ("moon1", "Gwx"),
            ("moon2", "-"),
            ("hour", "14"),
            ("afk", ""),
        ],
    );
    assert_eq!(
        draw(
            &vars,
            "%{wizi:game}|%incog|%stallion %slot1 %slot10|%{moon1:word} %{moon2:game}|%{hour:word}"
        ),
        "(Wizi 60)||M 12 ~|waxing gibbous -|2 pm"
    );
    assert_eq!(resolve(&vars, "incog"), Resolved::Absent);
    assert_eq!(resolve(&vars, "afk"), Resolved::Absent);
    assert_eq!(resolve(&vars, "slot2"), Resolved::Missing);
    capture(&mut vars, &[("afk", "1")]);
    assert_eq!(resolve(&vars, "afk"), Resolved::Value(Value::Flag));
}

// ---------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------

#[test]
fn catalog_names_follow_the_naming_rules() {
    let mut seen = std::collections::BTreeSet::new();
    for e in CATALOG {
        for name in std::iter::once(e.name).chain(e.aliases.iter().copied()) {
            assert!(seen.insert(name), "{name} twice");
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{name}"
            );
            for prefix in ["c_", "bg_", "s_", "pct_", "bar_"] {
                assert!(!name.starts_with(prefix), "{name}");
            }
            assert!(!name.ends_with("_bar"), "{name}");
            assert!(!["nl", "if", "ifnot", "end"].contains(&name), "{name}");
        }
        assert!(!e.label.is_empty(), "{}", e.name);
        if e.new_build {
            assert!(e.package.is_some(), "{}", e.name);
        }
    }
}

#[test]
fn every_catalog_entry_resolves_and_has_a_sample() {
    let vars = Vars::new(true);
    let client = client();
    let samples = vosh_prompt::values::Samples { now: now() };
    for e in CATALOG {
        let f = if e.param {
            FieldRef::with_param(e.name, "x")
        } else {
            FieldRef::new(e.name)
        };
        assert_ne!(
            vars.resolver(&client).resolve(&f),
            Resolved::Unknown,
            "{}",
            e.name
        );
        if e.name != "gmcp" {
            assert!(
                matches!(samples.resolve(&f), Resolved::Value(_)),
                "{} sample",
                e.name
            );
        }
        assert!(!e.kind.formats().is_empty(), "{}", e.name);
    }
    // The Detailed preset draws whole from the samples.
    let drawn = render_str(DETAILED, &samples, RenderOptions::default()).plain;
    assert_eq!(
        drawn,
        "Blackwatch Guard ██████░░░░ 60% quite a few wounds\n1020/1020hp 800/800mn 930/930mv tick 14 [S] 1250g 2 missing"
    );
}
