//! Renders with preview values on top of the live ones, and the state of
//! each field `prompt_state_get` reports (sections 4, 6 and 7 step 8).

mod common;

use common::{capture, client, feed, packet};
use serde_json::json;
use vosh_prompt::card::state::{catalog, State};
use vosh_prompt::testkit::designs::{DETAILED, JAMES};
use vosh_prompt::testkit::now;
use vosh_prompt::values::overrides::{lament_hides, Overridden, Overrides, Preview, PromptPreview};
use vosh_prompt::values::{Group, Samples, Source};
use vosh_prompt::{render_str, FieldRef, PromptEngine, RenderOptions, Vars};

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
    let client = client();
    let live = vars.resolver(&client);
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
    let client = client();
    let resolver = vars.resolver(&client);
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
    assert!(lament_hides(&FieldRef::with_param("member_hp", "Quenby")));
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
    let fields = catalog(&vars, &client(), &reads);
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
    let fields = catalog(&hidden, &client(), &[]);
    let hp = fields.iter().find(|f| f.name == "hp").expect("hp");
    assert_eq!((hp.state, hp.value.clone()), (State::Hidden, None));
}

/// `template` drawn with the card's `preview` over `vars`.
fn draw_preview(vars: &Vars, preview: &PromptPreview, template: &str) -> String {
    let client = client();
    let live = vars.resolver(&client);
    let over = preview.overrides(&live);
    render_str(
        template,
        &Overridden::new(&live, &over, now()),
        RenderOptions::default(),
    )
    .plain
}

fn named(preview: Preview) -> PromptPreview {
    PromptPreview {
        preview: Some(preview),
        ..PromptPreview::default()
    }
}

#[test]
fn the_card_previews_draw_now_low_health_a_fight_and_lament() {
    let mut vars = live();
    packet(
        &mut vars,
        "Char.Status",
        json!({"name": "Tester", "level": 30}),
    );
    let vitals = "%hp/%{maxhp}hp %mana/%{maxmana}mn";
    assert_eq!(
        draw_preview(&vars, &named(Preview::Now), vitals),
        "850/900hp 760/820mn"
    );
    // Low health is 180 with the maxes kept.
    assert_eq!(
        draw_preview(&vars, &named(Preview::LowHealth), vitals),
        "180/900hp 760/820mn"
    );
    // Fight draws the sample opponent and makes you the tank, at your
    // own health as the game counts it.
    let fight = named(Preview::Fight);
    assert_eq!(
        draw_preview(&vars, &fight, DETAILED).lines().next(),
        Some("Blackwatch Guard ██████░░░░ 60% quite a few wounds")
    );
    assert_eq!(
        draw_preview(&vars, &fight, "%{if:fight}%tank %{tank_hp:pct}%%%{end}"),
        "Tester 94%"
    );
    assert_eq!(draw_preview(&vars, &fight, vitals), "850/900hp 760/820mn");
    // Lament hides every value the song hides.
    assert_eq!(
        draw_preview(&vars, &named(Preview::Lament), JAMES),
        "[?(?%)h ?(?%)m ?(?%)v] "
    );
}

#[test]
fn a_fight_preview_with_nothing_live_takes_the_samples() {
    let vars = Vars::new(true);
    assert_eq!(
        draw_preview(
            &vars,
            &named(Preview::Fight),
            "%tank %{tank_hp:pct}%% %opponent"
        ),
        "Wystan 78% Blackwatch Guard"
    );
    // While the game hides your health, the tank health is hidden too.
    let mut hidden = Vars::new(true);
    feed(&mut hidden, "char-prompt.gmcp");
    feed(&mut hidden, "char-vitals-hidden.gmcp");
    assert_eq!(
        draw_preview(&hidden, &named(Preview::Fight), "%{tank_hp:pct}"),
        "?"
    );
}

#[test]
fn values_on_top_of_a_preview_and_the_card_flags() {
    let vars = live();
    let preview: PromptPreview = serde_json::from_value(json!({
        "preview": "low_health",
        "overrides": {"values": {"mana": 5}},
        "placeholders": true,
    }))
    .expect("a preview");
    assert_eq!(preview.preview, Some(Preview::LowHealth));
    assert!(preview.placeholders && !preview.raw);
    assert_eq!(draw_preview(&vars, &preview, "%hp %mana"), "180 5");
    // Values on top win over the named preview's.
    let preview: PromptPreview = serde_json::from_value(json!({
        "preview": "low_health",
        "overrides": {"values": {"hp": 20}, "lament": false},
    }))
    .expect("a preview");
    assert_eq!(draw_preview(&vars, &preview, "%hp"), "20");
    // What draws the live prompt as it is.
    assert!(PromptPreview::default().is_live());
    assert!(named(Preview::Now).is_live());
    assert!(!named(Preview::LowHealth).is_live());
    for shows_more in [
        json!({"placeholders": true}),
        json!({"raw": true}),
        json!({"overrides": {"lament": true}}),
    ] {
        let preview: PromptPreview = serde_json::from_value(shows_more).expect("a preview");
        assert!(!preview.is_live(), "{preview:?}");
    }
    let empty: PromptPreview =
        serde_json::from_value(json!({"overrides": {"values": {}}})).expect("a preview");
    assert!(empty.is_live());
    for (name, preview) in [
        ("now", Preview::Now),
        ("low_health", Preview::LowHealth),
        ("fight", Preview::Fight),
        ("lament", Preview::Lament),
    ] {
        assert_eq!(serde_json::to_value(preview).expect("json"), json!(name));
    }
}

#[test]
fn the_engine_keeps_the_preview_until_the_connection_goes() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_preview(Some(named(Preview::Fight)));
    assert_eq!(engine.preview(), Some(&named(Preview::Fight)));
    // A preview that draws the live prompt is no preview.
    engine.set_preview(Some(named(Preview::Now)));
    assert_eq!(engine.preview(), None);
    engine.set_preview(Some(named(Preview::Lament)));
    engine.set_preview(None);
    assert_eq!(engine.preview(), None);
    // Another profile keeps it, since the card still shows it.
    engine.set_preview(Some(named(Preview::LowHealth)));
    engine.switch_profile();
    assert!(engine.preview().is_some());
    // The connection takes it.
    engine.disconnect();
    assert_eq!(engine.preview(), None);
    // One the card sets while you are offline lasts as you connect, since
    // the card is still open.
    engine.set_preview(Some(named(Preview::LowHealth)));
    engine.connect(false);
    assert_eq!(engine.preview(), Some(&named(Preview::LowHealth)));
}

#[test]
fn a_package_older_builds_send_feeds_a_new_build_field_only_on_the_new_build() {
    // Older builds send Char.Combat and Room.Info too, but only the new
    // build puts your tank in Char.Combat and only it reads Exits from
    // Room.Info, so there those fields come from your prompt alone.
    let mut vars = Vars::new(true);
    feed(&mut vars, "char-vitals.gmcp");
    feed(&mut vars, "char-combat.gmcp");
    feed(&mut vars, "room-info.gmcp");
    let sent = |vars: &Vars, name: &str| {
        catalog(vars, &client(), &[])
            .into_iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no field {name}"))
            .sent
    };
    assert!(!vars.new_build());
    assert!(!sent(&vars, "tank_hp"));
    assert!(!sent(&vars, "exits"));
    assert!(sent(&vars, "opponent"), "every build sends the opponent");
    feed(&mut vars, "char-prompt.gmcp");
    assert!(vars.new_build());
    assert!(sent(&vars, "tank_hp"));
    assert!(sent(&vars, "exits"));
}

#[test]
fn the_games_prompt_reads_as_plain_text_in_the_picker() {
    // The picker shows the value beside The game's prompt, so it shows
    // the prompt as text, not the color codes it came with.
    let mut vars = live();
    vars.set_script("raw", "\u{1b}[38;5;240m(Wizi 60)\u{1b}[0m [1020/1020hp] ");
    let raw = catalog(&vars, &client(), &[])
        .into_iter()
        .find(|f| f.name == "raw")
        .expect("the game's prompt");
    assert_eq!(raw.value.as_deref(), Some("(Wizi 60) [1020/1020hp] "));
}
