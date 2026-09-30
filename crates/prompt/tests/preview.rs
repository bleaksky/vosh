//! Renders with preview values on top of the live ones, and the state of
//! each field `prompt_state_get` reports (sections 4, 6 and 7 step 8).

mod common;

use common::{capture, feed, now, vosh, DETAILED, JAMES};
use serde_json::json;
use vosh_prompt::overrides::{lament_hides, Overridden, Overrides};
use vosh_prompt::state::{catalog, State};
use vosh_prompt::vars::{Group, Samples, Source};
use vosh_prompt::{render_str, FieldRef, RenderOptions, Vars};

fn overrides(values: serde_json::Value, lament: bool) -> Overrides {
    serde_json::from_value(json!({"values": values, "lament": lament})).expect("overrides")
}

fn live() -> Vars {
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "char-state.gmcp");
    vars
}

fn draw(vars: &Vars, over: &Overrides, template: &str) -> String {
    let vosh = vosh();
    let live = vars.resolver(&vosh);
    render_str(
        template,
        &Overridden::new(&live, over, now()),
        RenderOptions::default(),
    )
    .plain
}

#[test]
fn low_health_keeps_the_live_max() {
    let vars = live();
    let low = overrides(json!({"hp": 180}), false);
    assert_eq!(
        draw(&vars, &low, "%hp/%{maxhp}hp %pct_hp%%"),
        "180/900hp 20%"
    );
    // Colored by how full takes the preview's share.
    let vosh = vosh();
    let resolver = vars.resolver(&vosh);
    let rendered = render_str(
        "%c_hp%hp",
        &Overridden::new(&resolver, &low, now()),
        RenderOptions::default(),
    );
    assert!(
        rendered.ansi.starts_with("\x1b[31m180"),
        "{:?}",
        rendered.ansi
    );
    // Nothing else changes.
    assert_eq!(draw(&vars, &low, "%mana/%{maxmana}mn"), "760/820mn");
}

#[test]
fn fight_draws_the_sample_opponent_and_makes_you_the_tank() {
    let vars = live();
    let fight = overrides(
        json!({
            "fight": true,
            "opponent": "Blackwatch Guard",
            "opponent_hp": 60,
            "opponent_cond": "quite a few wounds",
            "tank": "Tester",
            "tank_hp": 90,
        }),
        false,
    );
    assert_eq!(
        draw(&vars, &fight, DETAILED).lines().next(),
        Some("Blackwatch Guard ██████░░░░ 60% quite a few wounds")
    );
    assert_eq!(
        draw(&vars, &fight, "%{if:tank}%tank: %{tank_hp:game}%{end}"),
        "Tester: [===|===|===|==-]"
    );
    // Out of a fight preview the section draws nothing.
    let calm = overrides(json!({"fight": false}), false);
    assert_eq!(draw(&vars, &calm, "%{if:fight}x%{end}y"), "y");
}

#[test]
fn lament_hides_every_value_the_song_hides() {
    let vars = live();
    let lament = overrides(json!({}), true);
    assert_eq!(draw(&vars, &lament, JAMES), "[?(?%)h ?(?%)m ?(?%)v] ");
    assert_eq!(draw(&vars, &lament, "%pos"), "sit");
    for name in [
        "hp",
        "maxmana",
        "hp_pct",
        "tank_hp",
        "opponent_hp",
        "missing",
        "leader",
    ] {
        assert!(lament_hides(&FieldRef::new(name)), "{name}");
    }
    assert!(lament_hides(&FieldRef::with_param("aff", "sanctuary")));
    assert!(lament_hides(&FieldRef::with_param("member_hp", "Tarvik")));
    assert!(lament_hides(&FieldRef::with_param(
        "gmcp",
        "Char.Vitals.hp"
    )));
    assert!(!lament_hides(&FieldRef::with_param(
        "gmcp",
        "Char.Worth.gold"
    )));
    for name in ["gold", "pos", "opponent", "tank", "slot1", "exits"] {
        assert!(!lament_hides(&FieldRef::new(name)), "{name}");
    }
}

#[test]
fn an_override_can_hide_or_clear_one_value() {
    let vars = live();
    let over = overrides(json!({"hp": "?", "mana": null, "MOVE": "10/250"}), false);
    assert_eq!(
        draw(&vars, &over, "%hp|%mana|%move/%{maxmove}"),
        "?||10/250"
    );
    // An alias names its field, and a parameter matches in any case.
    let over = overrides(json!({"mhp": 2000, "aff:SANCTUARY": 5}), false);
    assert_eq!(draw(&vars, &over, "%{maxhp} %{aff:sanctuary}"), "2000 5");
}

#[test]
fn samples_take_overrides_too() {
    let samples = Samples { now: now() };
    let over = overrides(json!({"hp": 180}), false);
    let rendered = render_str(
        "%hp/%{maxhp}",
        &Overridden::new(&samples, &over, now()),
        RenderOptions::default(),
    );
    assert_eq!(rendered.plain, "180/1020");
}

#[test]
fn every_field_reports_its_state_source_and_value() {
    let mut vars = live();
    feed(&mut vars, "char-combat.gmcp");
    capture(&mut vars, &[("gold", "1250")]);
    vars.set_script("mood", "grim");
    let reads = vec!["hp".to_string(), "maxhp".to_string(), "hp_pct".to_string()];
    let fields = catalog(&vars, &vosh(), &reads);
    let find = |name: &str| {
        fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no field {name}"))
    };

    let hp = find("hp");
    assert_eq!(hp.state, State::Value);
    assert_eq!(hp.source, Some(Source::Gmcp));
    assert_eq!(hp.value.as_deref(), Some("850"));
    assert_eq!(hp.max.as_deref(), Some("900"));
    assert!(hp.in_prompt && hp.sent);
    assert_eq!(hp.label, "Health");

    let gold = find("gold");
    assert_eq!(gold.source, Some(Source::Capture));
    assert_eq!(gold.value.as_deref(), Some("1250"));
    assert!(!gold.in_prompt);
    assert!(!gold.sent, "Char.Worth has not come");

    // An enum shows its word.
    let pos = find("pos");
    assert_eq!(pos.value.as_deref(), Some("sitting"));

    let opponent = find("opponent");
    assert_eq!(opponent.value.as_deref(), Some("a Blackwatch guard"));

    // Vosh's own and fields nothing sent yet.
    let time = find("time");
    assert_eq!(time.source, Some(Source::Vosh));
    let room = find("room");
    assert_eq!((room.state, room.source), (State::Missing, None));

    // A field with a parameter is a form to fill in.
    let aff = find("aff");
    assert!(aff.param);
    assert_eq!(aff.state, State::Missing);

    // A name only scripts set lists under Your scripts.
    let mood = find("mood");
    assert_eq!(mood.group, Group::Scripts);
    assert_eq!(mood.value.as_deref(), Some("grim"));

    // Hidden fields report no value.
    let mut hidden = Vars::new(true);
    feed(&mut hidden, "char-prompt.gmcp");
    feed(&mut hidden, "char-vitals-hidden.gmcp");
    let fields = catalog(&hidden, &vosh(), &[]);
    let hp = fields.iter().find(|f| f.name == "hp").expect("hp");
    assert_eq!((hp.state, hp.value.clone()), (State::Hidden, None));
}
