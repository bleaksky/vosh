//! The hidden model against the Aabahran
//! packets: the new build's flags, the derived terms for 243cac5c and the
//! older build, and one change per socket read.

mod common;

use common::{capture, draw, feed, lament_capture, packet, resolve, text};
use serde_json::json;
use vosh_prompt::testkit::designs::JAMES;
use vosh_prompt::values::Hidden;
use vosh_prompt::{Resolved, Value, Vars};

const ALL: Hidden = Hidden {
    hp: true,
    mana: true,
    moves: true,
    tank: true,
    opponent: true,
    affects: true,
    group: true,
};

/// Feed each step and check that no hidden field comes back between
/// them. Returns the hidden state after the last step.
fn feed_without_unhiding(vars: &mut Vars, steps: &[&dyn Fn(&mut Vars)]) -> Hidden {
    let mut seen = vars.hidden();
    for (i, step) in steps.iter().enumerate() {
        step(vars);
        let now = vars.hidden();
        for (name, was, is) in [
            ("hp", seen.hp, now.hp),
            ("mana", seen.mana, now.mana),
            ("moves", seen.moves, now.moves),
            ("tank", seen.tank, now.tank),
            ("opponent", seen.opponent, now.opponent),
            ("affects", seen.affects, now.affects),
            ("group", seen.group, now.group),
        ] {
            assert!(!was || is, "step {i} unhid {name}");
        }
        seen = now;
    }
    seen
}

/// Aabahran at a quiet prompt before the song, on any build.
fn before_the_song(vars: &mut Vars) {
    feed(vars, "char-vitals.gmcp");
    feed(vars, "char-affects.gmcp");
    feed(vars, "char-combat-tank.gmcp");
    feed(vars, "group-info.gmcp");
    capture(
        vars,
        &[
            ("hp", "850"),
            ("maxhp", "900"),
            ("mana", "760"),
            ("maxmana", "820"),
            ("move", "250"),
            ("maxmove", "250"),
        ],
    );
    assert!(vars.hidden().none());
}

#[test]
fn the_new_build_hides_by_its_flags_in_wire_order() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    assert!(vars.new_build());
    before_the_song(&mut vars);
    let hidden = feed_without_unhiding(
        &mut vars,
        &[
            // The song lands and Char.Affects goes out at once.
            &|v| feed(v, "char-affects-hidden.gmcp"),
            // Then the prompt time packages of the next pulse.
            &|v| feed(v, "char-vitals-hidden.gmcp"),
            &|v| packet(v, "Char.Worth", json!({"gold": 1250})),
            &|v| feed(v, "char-combat-tank-hidden.gmcp"),
            &|v| feed(v, "group-info-hidden.gmcp"),
            &|v| feed(v, "char-state.gmcp"),
            &|v| feed(v, "room-weather.gmcp"),
            // Then the prompt text, `Tester: ` and `[0/0hp 0/0mn 0/0mv]`.
            &|v| {
                capture(
                    v,
                    &[
                        ("tank", "Tester"),
                        ("tank_bar", ""),
                        ("hp", "0"),
                        ("maxhp", "0"),
                        ("mana", "0"),
                        ("maxmana", "0"),
                        ("move", "0"),
                        ("maxmove", "0"),
                    ],
                );
            },
        ],
    );
    assert_eq!(hidden, ALL);
    assert_eq!(draw(&vars, JAMES), "[?(?%)h ?(?%)m ?(?%)v] ");
    assert_eq!(resolve(&vars, "tank"), text("Tester"));
    assert_eq!(resolve(&vars, "opponent"), text("a Blackwatch guard"));
    for field in [
        "hp",
        "maxmana",
        "tank_hp",
        "opponent_hp",
        "opponent_cond",
        "missing",
        "aff:bless",
        "leader",
        "group_size",
        "group_low",
        "member_hp:Tester",
    ] {
        assert_eq!(resolve(&vars, field), Resolved::Hidden, "{field}");
    }
}

#[test]
fn the_new_build_unhides_each_field_with_its_own_package() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    for file in [
        "char-affects-hidden.gmcp",
        "char-vitals-hidden.gmcp",
        "char-combat-tank-hidden.gmcp",
        "group-info-hidden.gmcp",
    ] {
        feed(&mut vars, file);
    }
    assert_eq!(vars.hidden(), ALL);
    // The song ends, and Char.Affects without the flag comes at once.
    feed(&mut vars, "char-affects.gmcp");
    assert_eq!(
        vars.hidden(),
        Hidden {
            affects: false,
            ..ALL
        }
    );
    feed(&mut vars, "char-vitals.gmcp");
    assert!(!vars.hidden().vitals());
    assert!(vars.hidden().tank && vars.hidden().group);
    feed(&mut vars, "char-combat-tank.gmcp");
    feed(&mut vars, "group-info.gmcp");
    assert!(vars.hidden().none());
}

#[test]
fn the_new_build_decides_by_its_flags_alone() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    // Zeros, a named song, a lone target and an empty group, all with no
    // flag, hide nothing once the game sends flags.
    for file in [
        "char-vitals-zero.gmcp",
        "char-affects-lament.gmcp",
        "char-combat-withheld.gmcp",
        "group-info-empty.gmcp",
    ] {
        feed(&mut vars, file);
    }
    lament_capture(&mut vars);
    assert!(vars.hidden().none());
    // A max of 0 with no flag reads as nothing to show.
    assert_eq!(resolve(&vars, "hp"), Resolved::Absent);
    assert_eq!(resolve(&vars, "opponent_hp"), Resolved::Absent);
}

#[test]
fn build_243cac5c_hides_by_the_derived_terms() {
    let mut vars = Vars::new(true);
    before_the_song(&mut vars);
    let hidden = feed_without_unhiding(
        &mut vars,
        &[
            &|v| feed(v, "char-affects-empty.gmcp"),
            &|v| feed(v, "char-vitals-zero.gmcp"),
            &|v| feed(v, "char-combat-withheld.gmcp"),
            &|v| feed(v, "group-info-empty.gmcp"),
            &|v| lament_capture(v),
        ],
    );
    // No one tanks in this fight, so there is no tank health to hide.
    assert_eq!(hidden, Hidden { tank: false, ..ALL });
    assert_eq!(draw(&vars, JAMES), "[?(?%)h ?(?%)m ?(?%)v] ");
    assert_eq!(resolve(&vars, "opponent_hp"), Resolved::Hidden);
    assert_eq!(resolve(&vars, "group_size"), Resolved::Hidden);
}

#[test]
fn the_older_build_leaks_nothing_under_the_song() {
    let mut vars = Vars::new(true);
    before_the_song(&mut vars);
    let hidden = feed_without_unhiding(
        &mut vars,
        &[
            &|v| feed(v, "char-affects-lament.gmcp"),
            // True values keep coming, the roster with your own row, and the
            // opponent's health.
            &|v| feed(v, "char-vitals.gmcp"),
            &|v| feed(v, "char-combat-lament-older.gmcp"),
            &|v| feed(v, "group-info-own-row.gmcp"),
            &|v| lament_capture(v),
        ],
    );
    assert_eq!(hidden, ALL);
    for field in [
        "hp",
        "maxhp",
        "mhp",
        "hp_pct",
        "leader",
        "group_size",
        "group_low",
        "member_hp:Tester",
        "member_mana:id=1769388810",
        "opponent_hp",
        "opponent_cond",
        "aff:lamented_tears",
        "missing",
        "gmcp:Char.Vitals.hp",
        "gmcp:Group.Info.members[name=Tester].hp_pct",
        "gmcp:Group.Info",
        "gmcp:Char.Combat.hp_pct",
        "gmcp:Char.Combat.condition",
        "gmcp:Char.Affects.affects[0].name",
    ] {
        assert_eq!(resolve(&vars, field), Resolved::Hidden, "{field}");
    }
    // Who you fight stays shown.
    assert_eq!(resolve(&vars, "opponent"), text("a Blackwatch guard"));
    assert_eq!(
        resolve(&vars, "gmcp:Char.Combat.target"),
        text("a Blackwatch guard")
    );
    assert_eq!(draw(&vars, JAMES), "[?(?%)h ?(?%)m ?(?%)v] ");
    assert_eq!(draw(&vars, "%hp/%{maxhp}"), "?/?");
    let pv = vars.prompt_vars();
    assert_eq!(pv.get("hp").map(String::as_str), Some("?"));
}

#[test]
fn a_session_without_char_prompt_ends_each_pulse_hiding_what_the_new_build_hides() {
    let lament: [&str; 4] = [
        "char-affects-hidden.gmcp",
        "char-vitals-hidden.gmcp",
        "char-combat-tank-hidden.gmcp",
        "group-info-hidden.gmcp",
    ];
    let ended: [&str; 4] = [
        "char-affects.gmcp",
        "char-vitals.gmcp",
        "char-combat-tank.gmcp",
        "group-info.gmcp",
    ];
    let mut new_build = Vars::new(true);
    feed(&mut new_build, "char-prompt.gmcp");
    // A reconnect on the new build brings no Char.Prompt.
    let mut reconnected = Vars::new(true);
    for vars in [&mut new_build, &mut reconnected] {
        before_the_song(vars);
        let hidden = feed_without_unhiding(
            vars,
            &[
                &|v| feed(v, lament[0]),
                &|v| feed(v, lament[1]),
                &|v| feed(v, lament[2]),
                &|v| feed(v, lament[3]),
                &|v| lament_capture(v),
            ],
        );
        assert_eq!(hidden, ALL);
        for file in ended {
            feed(vars, file);
        }
        assert!(vars.hidden().none());
    }
    assert!(new_build.new_build());
    assert!(!reconnected.new_build());
}

#[test]
fn tank_health_hides_alone_while_the_tank_keeps_its_name() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    // Mirror image withholds your opponent's health and keeps the tank's.
    packet(
        &mut vars,
        "Char.Combat",
        json!({"target":"a Blackwatch guard","hidden":true,"tank":{"name":"Tester","hp_pct":78}}),
    );
    assert_eq!(resolve(&vars, "opponent_hp"), Resolved::Hidden);
    assert_eq!(
        resolve(&vars, "tank_hp"),
        Resolved::Value(Value::TankHp(78))
    );
    assert_eq!(
        draw(&vars, "%tank: %{tank_hp:game}"),
        "Tester: [===|===|===|=--]"
    );
    // Under the song the tank loses its health too.
    feed(&mut vars, "char-combat-tank-hidden.gmcp");
    assert_eq!(resolve(&vars, "tank_hp"), Resolved::Hidden);
    assert_eq!(resolve(&vars, "tank"), text("Tester"));
    assert_eq!(
        draw(&vars, "%tank: %{tank_hp:game}"),
        "Tester: [···|···|···|···]"
    );
}

#[test]
fn a_blank_tank_health_under_a_named_tank_is_hidden_by_the_capture() {
    // H2 on a build with no tank object.
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    capture(&mut vars, &[("tank", "Tester"), ("tank_pct", "")]);
    assert!(vars.hidden().tank);
    // The same prompt with no %p or %P code hides nothing.
    capture(&mut vars, &[("tank", "Tester")]);
    assert!(!vars.hidden().tank);
}

/// The values a prompt trigger hands to `mud.set_prompt_var`, which is
/// how the capture reaches the engine until it moves into the profile.
fn script_capture(vars: &mut Vars, pairs: &[(&str, &str)]) {
    for (name, value) in pairs {
        vars.set_script(name, value);
    }
}

#[test]
fn a_prompt_trigger_that_reads_zero_maxes_hides_the_vitals() {
    // The older build after a link dead reconnect in the middle of the
    // song. No Char.Affects comes until the next tick, Char.Vitals sends
    // the true values, and the prompt reads `[0/0hp 0/0mn 0/0mv]`.
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    let zeros = [
        ("hp", "0"),
        ("maxhp", "0"),
        ("mana", "0"),
        ("maxmana", "0"),
        ("move", "0"),
        ("maxmove", "0"),
    ];
    script_capture(&mut vars, &zeros);
    let hidden = vars.hidden();
    assert!(hidden.hp && hidden.mana && hidden.moves, "{hidden:?}");
    assert!(!hidden.affects && !hidden.group && !hidden.tank);
    assert_eq!(draw(&vars, JAMES), "[?(?%)h ?(?%)m ?(?%)v] ");
    assert_eq!(resolve(&vars, "maxmana"), Resolved::Hidden);
    let pv = vars.prompt_vars();
    assert_eq!(pv.len(), 6);
    assert!(pv.values().all(|v| v == "?"), "{pv:?}");
    // The values last one pulse, as a capture does.
    feed(&mut vars, "char-vitals.gmcp");
    assert!(vars.hidden().none());
    script_capture(&mut vars, &zeros);
    assert!(vars.hidden().vitals());
    // Clearing one max takes its pair out of the rule.
    assert!(vars.remove_script("maxhp"));
    assert!(!vars.hidden().hp);
    assert!(vars.hidden().mana);
}

#[test]
fn a_prompt_trigger_that_reads_a_blank_tank_health_hides_it() {
    // H2 from a prompt trigger, on a build with no tank object.
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    script_capture(&mut vars, &[("tank", "Tester"), ("tank_pct", "")]);
    assert!(vars.hidden().tank);
    assert_eq!(draw(&vars, "%tank: %tank_hp"), "Tester: ?");
}

#[test]
fn one_hidden_change_per_read() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    before_the_song(&mut vars);
    assert_eq!(vars.take_hidden_change(), None);
    // One socket read holds the whole pulse.
    for file in [
        "char-affects-hidden.gmcp",
        "char-vitals-hidden.gmcp",
        "char-combat-tank-hidden.gmcp",
        "group-info-hidden.gmcp",
    ] {
        feed(&mut vars, file);
    }
    assert_eq!(vars.take_hidden_change(), Some(ALL));
    assert_eq!(vars.take_hidden_change(), None);
    // The next pulse repeats the same packets and changes nothing.
    feed(&mut vars, "char-vitals-hidden.gmcp");
    feed(&mut vars, "group-info-hidden.gmcp");
    assert_eq!(vars.take_hidden_change(), None);
    // A disconnect clears the panes once.
    vars.disconnect();
    assert_eq!(vars.take_hidden_change(), Some(Hidden::default()));
    let json = serde_json::to_value(ALL).expect("json");
    assert_eq!(
        json,
        json!({"vitals": true, "tank": true, "opponent": true, "affects": true, "group": true})
    );
}

#[test]
fn the_reported_state_is_what_the_last_report_said() {
    // A window that opens late reads this, so it must match the report
    // every other window heard, not a state no report carried.
    let mut vars = Vars::new(true);
    assert_eq!(vars.reported(), Hidden::default());
    for file in [
        "char-affects-lament.gmcp",
        "char-vitals.gmcp",
        "char-combat-lament-older.gmcp",
        "group-info-own-row.gmcp",
    ] {
        feed(&mut vars, file);
    }
    assert_eq!(vars.hidden(), ALL);
    assert_eq!(vars.reported(), Hidden::default());
    assert_eq!(vars.take_hidden_change(), Some(ALL));
    assert_eq!(vars.reported(), ALL);
    // The song ends and comes back within one read, so nothing new is
    // reported and the reported state stays.
    feed(&mut vars, "char-affects.gmcp");
    feed(&mut vars, "char-affects-lament.gmcp");
    assert_eq!(vars.take_hidden_change(), None);
    assert_eq!(vars.reported(), ALL);
    vars.disconnect();
    assert_eq!(vars.take_hidden_change(), Some(Hidden::default()));
    assert_eq!(vars.reported(), Hidden::default());
}

#[test]
fn being_solo_is_not_hidden() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "group-info-solo.gmcp");
    assert!(!vars.hidden().group);
    assert_eq!(resolve(&vars, "group_size"), Resolved::Absent);
    assert_eq!(resolve(&vars, "leader"), Resolved::Absent);
    feed(&mut vars, "char-prompt.gmcp");
    assert!(!vars.hidden().group);
}

#[test]
fn prompt_vars_mark_hidden_values() {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-prompt.gmcp");
    feed(&mut vars, "char-vitals-hidden.gmcp");
    lament_capture(&mut vars);
    vars.set_script("mood", "grim");
    let pv = vars.prompt_vars();
    assert_eq!(pv.get("hp").map(String::as_str), Some("?"));
    assert_eq!(pv.get("maxmove").map(String::as_str), Some("?"));
    assert_eq!(pv.get("mood").map(String::as_str), Some("grim"));
}

#[test]
fn other_games_hide_a_field_only_when_its_packet_says_so() {
    let mut vars = Vars::new(false);
    for file in [
        "char-vitals-hidden.gmcp",
        "char-affects-hidden.gmcp",
        "char-combat-hidden.gmcp",
        "group-info-hidden.gmcp",
    ] {
        feed(&mut vars, file);
    }
    lament_capture(&mut vars);
    let hidden = vars.hidden();
    assert!(hidden.hp && hidden.mana && hidden.moves);
    assert!(hidden.affects && hidden.group && hidden.opponent);
    assert!(!hidden.tank);
    assert_eq!(resolve(&vars, "hp"), Resolved::Hidden);
}

#[test]
fn other_games_use_none_of_the_derived_rules() {
    let mut vars = Vars::new(false);
    for file in [
        "char-vitals-zero.gmcp",
        "char-affects-lament.gmcp",
        "char-combat-withheld.gmcp",
        "group-info-empty.gmcp",
    ] {
        feed(&mut vars, file);
    }
    lament_capture(&mut vars);
    assert!(vars.hidden().none());
    assert_eq!(resolve(&vars, "hp"), Resolved::Absent);
    assert_eq!(resolve(&vars, "opponent_hp"), Resolved::Absent);
    // A prompt trigger that reads a max of 0 there means no such pool.
    let mut other = Vars::new(false);
    script_capture(&mut other, &[("mana", "0"), ("maxmana", "0")]);
    assert!(other.hidden().none());
    assert_eq!(resolve(&other, "mana"), Resolved::Absent);
}
