use std::sync::Arc;

use serde_json::json;
use vosh_prompt::config::{AabahranCapture, RegexCapture};

use super::*;
use crate::app::state::{AppState, PROFILES_NOT_LOADED};
use crate::profile::file::ProfileConfig;
use crate::profile::set::{ProfileSet, DEFAULT_PROFILE_NAME};

fn codes(prompt: &str) -> CaptureConfig {
    CaptureConfig::Aabahran(AabahranCapture {
        prompt: prompt.into(),
        ..AabahranCapture::default()
    })
}

#[test]
fn a_table_with_a_capture_that_does_not_compile_changes_nothing() {
    let mut p = Profile::default();
    let bad = PromptConfig {
        capture: codes("<`%h> "),
        ..PromptConfig::from_legacy(true, "%hp")
    };
    assert_eq!(
        set_config(&mut p, bad),
        Err("A color code runs into %h. Put a space between them in the game.".into())
    );
    assert!(p.prompt.config().is_default());
    let pattern = PromptConfig {
        capture: CaptureConfig::Regex(RegexCapture {
            lines: vec![r"\[(?<hp>\d+".into()],
            ..RegexCapture::default()
        }),
        ..PromptConfig::default()
    };
    assert_eq!(
        set_config(&mut p, pattern),
        Err("Vosh cannot read that pattern.".into())
    );
}

#[test]
fn codes_the_game_sent_that_do_not_compile_still_take_a_new_design() {
    let mut p = Profile::default();
    let follows = PromptConfig {
        capture: CaptureConfig::Aabahran(AabahranCapture {
            prompt: "<%hhp> ".into(),
            follow_game: true,
            ..AabahranCapture::default()
        }),
        ..PromptConfig::from_legacy(true, "%hp")
    };
    assert_eq!(set_config(&mut p, follows), Ok(true));
    // The game sends codes where a color runs into a code, and the
    // capture follows them as they are.
    let at = chrono::DateTime::parse_from_rfc3339("2026-09-30T12:00:00-05:00").unwrap();
    let _ = p.prompt.observe(
        "Char.Prompt",
        json!({"enabled": true, "prompt": "<`%hhp> ", "fprompt": ""}),
        at,
    );
    let CaptureConfig::Aabahran(sent) = &p.prompt.config().capture else {
        panic!("the capture reads codes");
    };
    assert_eq!(sent.prompt, "<`%hhp> ");
    assert!(!p.prompt.stage.has_recognizer());
    // The card saves a new design with the capture it read back.
    let edited = PromptConfig {
        template: "[%hp]".into(),
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, edited), Ok(true));
    assert_eq!(p.prompt.config().template, "[%hp]");
    // So does the switch.
    let off = PromptConfig {
        draw: false,
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, off), Ok(true));
    assert!(!p.prompt.config().draw);
    // Other codes that do not compile still change nothing.
    let other = PromptConfig {
        capture: codes("<`%mm> "),
        template: "[%mana]".into(),
        ..p.prompt.config().clone()
    };
    assert_eq!(
        set_config(&mut p, other),
        Err("A color code runs into %m. Put a space between them in the game.".into())
    );
    assert_eq!(p.prompt.config().template, "[%hp]");
}

#[test]
fn a_table_that_compiles_is_taken_with_two_earlier_designs_at_most() {
    let mut p = Profile::default();
    let config = PromptConfig {
        previous_templates: vec!["a".into(), "b".into(), "c".into()],
        capture: codes("<%hhp %mm> "),
        ..PromptConfig::from_legacy(true, "%hp")
    };
    assert_eq!(set_config(&mut p, config.clone()), Ok(true));
    assert_eq!(p.prompt.config().previous_templates, ["a", "b"]);
    assert!(p.prompt.stage.has_recognizer());
    // A save writes the [ui] copy from the table, for older builds.
    let file = crate::profile::file::ProfileConfig::from_profile(&p);
    assert!(file.ui.prompt_template_enabled);
    assert_eq!(file.ui.prompt_template, "%hp");
    // The same table again changes nothing.
    assert_eq!(set_config(&mut p, config), Ok(false));
}

/// Same as the game for `prompt`, for a mortal.
fn same_as_the_game(prompt: &str) -> String {
    vosh_prompt::card::presets::game(prompt, "", vosh_prompt::aabahran::Who::default())
        .expect("the codes compile")
}

#[test]
fn turning_drawing_on_with_no_design_follows_the_game() {
    let mut p = Profile::default();
    let config = PromptConfig {
        capture: codes("<%hhp> "),
        ..PromptConfig::from_legacy(false, "")
    };
    assert_eq!(set_config(&mut p, config), Ok(true));
    assert_eq!(p.prompt.config().template, "");
    let on = PromptConfig {
        draw: true,
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, on), Ok(true));
    assert!(p.prompt.config().mirror);
    assert_eq!(p.prompt.config().template, same_as_the_game("<%hhp> "));
    // A table read before the codes changed brings back no old design.
    let stale = PromptConfig {
        template: "%hp ".into(),
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, stale), Ok(false));
    assert_eq!(p.prompt.config().template, same_as_the_game("<%hhp> "));
    // An edit makes the design yours.
    let edited = PromptConfig {
        template: format!("{}> ", same_as_the_game("<%hhp> ")),
        mirror: false,
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, edited.clone()), Ok(true));
    assert_eq!(*p.prompt.config(), edited);
    // Start empty while drawing stays on keeps the design empty.
    let empty = PromptConfig {
        template: String::new(),
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, empty), Ok(true));
    assert_eq!(p.prompt.config().template, "");
    assert!(!p.prompt.config().mirror);
    // Same as the game in the start list follows the game again.
    let game = PromptConfig {
        template: same_as_the_game("<%hhp> "),
        mirror: true,
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, game), Ok(true));
    assert!(p.prompt.config().mirror);
    assert_eq!(p.prompt.config().template, same_as_the_game("<%hhp> "));
}

#[test]
fn turning_drawing_on_keeps_vosh_default_you_chose() {
    let mut p = Profile::default();
    let mut chosen = PromptConfig {
        capture: codes("<%hhp> "),
        ..PromptConfig::fresh()
    };
    assert!(chosen.use_default_design());
    assert_eq!(set_config(&mut p, chosen), Ok(true));
    let on = PromptConfig {
        draw: true,
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, on), Ok(true));
    assert_eq!(p.prompt.config().template, vosh_prompt::DEFAULT_DESIGN);
    assert!(!p.prompt.config().mirror);
}

#[test]
fn start_empty_keeps_the_design_empty_as_drawing_turns_on() {
    // A fresh profile draws nothing and follows the game. Start empty in
    // the card's start list turns drawing on with no design, and the
    // design stays empty and becomes yours.
    let mut p = Profile::default();
    let fresh = PromptConfig {
        capture: codes("<%hhp> "),
        ..PromptConfig::fresh()
    };
    assert_eq!(set_config(&mut p, fresh), Ok(true));
    assert!(!p.prompt.config().draw);
    assert!(p.prompt.config().mirror);
    let empty = PromptConfig {
        template: String::new(),
        draw: true,
        mirror: false,
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config_as_is(&mut p, empty), Ok(true));
    assert!(p.prompt.config().draw);
    assert_eq!(p.prompt.config().template, "");
    // So does a design that was empty already.
    let off = PromptConfig {
        draw: false,
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, off), Ok(true));
    let again = PromptConfig {
        draw: true,
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config_as_is(&mut p, again), Ok(true));
    assert_eq!(p.prompt.config().template, "");
}

#[tokio::test]
async fn designs_list_every_other_profile_with_a_design() {
    let dir = tempfile::tempdir().unwrap();
    let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
    set.create("Second").unwrap();
    set.create("Third").unwrap();
    set.create("Fourth").unwrap();
    set.create("Fifth").unwrap();
    set.create("Sixth").unwrap();
    set.create("Seventh").unwrap();
    let mut second = ProfileConfig::default();
    second.set_prompt(PromptConfig::from_legacy(false, "[%hp]"));
    second.save(&set.profile_path("Second")).unwrap();
    // A design kept only in [ui], as older builds wrote it.
    let mut third = ProfileConfig::default();
    third.ui.prompt_template = "%mana".into();
    third.save(&set.profile_path("Third")).unwrap();
    // Fourth never saved a file, so it follows the game. Fifth chose
    // Vosh's default design, which the start list already offers.
    let mut fifth = ProfileConfig::default();
    fifth.set_prompt(PromptConfig::from_legacy(true, vosh_prompt::DEFAULT_DESIGN));
    fifth.save(&set.profile_path("Fifth")).unwrap();
    // Sixth changed the default design, so it holds a design of its
    // own.
    let sixth_design = vosh_prompt::DEFAULT_DESIGN.trim_end();
    let mut sixth = ProfileConfig::default();
    sixth.set_prompt(PromptConfig::from_legacy(true, sixth_design));
    sixth.save(&set.profile_path("Sixth")).unwrap();
    // Seventh holds the default an earlier build shipped, as that build
    // wrote it, which loads following the game.
    std::fs::write(
        set.profile_path("Seventh"),
        format!(
            "[prompt]\ndraw = false\ntemplate = {}\n",
            toml::Value::String(vosh_prompt::config::RETIRED_DEFAULTS[0].into())
        ),
    )
    .unwrap();
    // Eighth follows the game, so its design is its codes, which Same
    // as the game offers for yours.
    set.create("Eighth").unwrap();
    let mut eighth = ProfileConfig::default();
    eighth.set_prompt(PromptConfig {
        draw: true,
        capture: codes("<%hhp %mm> "),
        ..PromptConfig::fresh()
    });
    eighth.save(&set.profile_path("Eighth")).unwrap();
    let eighth = ProfileConfig::load(&set.profile_path("Eighth")).unwrap();
    assert_eq!(
        eighth.prompt_config().template,
        same_as_the_game("<%hhp %mm> ")
    );
    let mut active = ProfileConfig::default();
    active.set_prompt(PromptConfig::from_legacy(true, "%move"));
    active
        .save(&set.profile_path(DEFAULT_PROFILE_NAME))
        .unwrap();
    let state: SharedState = Arc::new(AppState::default());
    *state.profile_set.lock().await = Some(set);
    let list = designs(&state).await.unwrap();
    let got: Vec<(&str, &str, &str)> = list
        .iter()
        .map(|d| {
            (
                d.profile.as_str(),
                d.display_name.as_str(),
                d.template.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("Second", "Second", "[%hp]"),
            ("Third", "Third", "%mana"),
            ("Sixth", "Sixth", sixth_design),
        ]
    );
    let json = serde_json::to_value(&list[0]).unwrap();
    assert_eq!(
        json,
        json!({"profile": "Second", "display_name": "Second", "template": "[%hp]"})
    );

    // Without a profile set there is nothing to read.
    let empty: SharedState = Arc::new(AppState::default());
    assert_eq!(designs(&empty).await, Err(PROFILES_NOT_LOADED.to_string()));
}

#[test]
fn compile_reports_with_the_values_this_session_supplies() {
    let mut p = Profile::default();
    let request: CompileRequest = serde_json::from_value(json!({
        "kind": "regex",
        "lines": [r"^<(?<hp>\d+)hp> $"],
    }))
    .unwrap();
    let ids = |report: &CompileReport| -> Vec<&'static str> {
        report.presets.iter().map(|preset| preset.id).collect()
    };
    assert_eq!(
        ids(&compile(&p, &request)),
        ["default", "minimal", "how_full", "detailed", "empty"]
    );
    p.prompt.connect(false);
    p.prompt.observe(
        "Char.Vitals",
        json!({"hp": 10, "maxhp": 20, "mana": 5, "maxmana": 9, "move": 1, "maxmove": 2}),
        chrono::Local::now().fixed_offset(),
    );
    assert_eq!(
        ids(&compile(&p, &request)),
        ["default", "minimal", "how_full", "percent", "bars", "detailed", "empty"]
    );
    let codes: CompileRequest = serde_json::from_value(json!({
        "kind": "aabahran",
        "prompt": "[%h/%Hhp]",
        "typed": true,
    }))
    .unwrap();
    let report = compile(&p, &codes);
    assert!(report.ok);
    assert_eq!(report.prompt, "[%h/%Hhp] ");
}

#[test]
fn a_ring_entry_becomes_a_capture_with_the_names_vosh_suggests() {
    let mut p = Profile::default();
    p.prompt.connect(false);
    let mut out = vosh_prompt::stage::Output::new(false);
    let line = "<100hp 50m 30mv> ";
    p.prompt
        .stage
        .line(&mut out, line.as_bytes(), line, None, b"");
    p.prompt.record(None, 1);
    let id = p.prompt.stage.ring().last().expect("the entry").id;
    let report = capture_from_line(&p, id, &[]).expect("the report");
    assert!(report.ok);
    // The names the numbers read into, without the ones you left out.
    let read_into = |report: &CompileReport| -> Vec<String> {
        report
            .numbers
            .iter()
            .filter(|n| !n.name.is_empty())
            .map(|n| n.name.clone())
            .collect()
    };
    assert_eq!(read_into(&report), ["hp", "mana", "move"]);
    assert_eq!(report.numbers.len(), 3);
    // Before Char.Vitals comes, the presets draw only what the line
    // reads.
    let ids = |report: &CompileReport| -> Vec<&'static str> {
        report.presets.iter().map(|preset| preset.id).collect()
    };
    let renamed = capture_from_line(&p, id, &["health".into(), String::new()]).expect("the report");
    assert_eq!(read_into(&renamed), ["health", "move"]);
    assert_eq!(
        ids(&renamed),
        ["default", "minimal", "how_full", "detailed", "empty"]
    );
    assert_eq!(
        capture_from_line(&p, id + 1, &[]),
        Err("Vosh no longer keeps that line. Pick another one.".into())
    );
    // Once Char.Vitals came, the presets draw every vital.
    p.prompt.observe(
        "Char.Vitals",
        json!({"hp": 10, "maxhp": 20, "mana": 5, "maxmana": 9, "move": 1, "maxmove": 2}),
        chrono::Local::now().fixed_offset(),
    );
    let report = capture_from_line(&p, id, &[]).expect("the report");
    assert_eq!(
        ids(&report),
        ["default", "minimal", "how_full", "percent", "bars", "detailed", "empty"]
    );
    // The vitals this game sends that Vosh has no name for are offered
    // as names, each with its package.
    let leftover = &report.gmcp_names;
    assert!(leftover.is_empty(), "{leftover:?}");
    p.prompt.observe(
        "Char.Vitals",
        json!({"hp": 10, "maxhp": 20, "mp": 5, "mv": 9, "hidden": false}),
        chrono::Local::now().fixed_offset(),
    );
    let named = capture_from_line(&p, id, &[]).expect("the report");
    let names: Vec<(&str, &str)> = named
        .gmcp_names
        .iter()
        .map(|n| (n.name.as_str(), n.package.as_str()))
        .collect();
    assert_eq!(names, [("mp", "Char.Vitals"), ("mv", "Char.Vitals")]);
    // A prompt of several lines gives its last.
    let mut out = vosh_prompt::stage::Output::new(false);
    p.prompt.stage.line(&mut out, b"x", "x", None, b"");
    let block = "Tester: [===|---]\n<5hp> ";
    p.prompt.record(Some((block.as_bytes(), block)), 2);
    let id = p.prompt.stage.ring().last().expect("the entry").id;
    let report = capture_from_line(&p, id, &[]).expect("the report");
    assert_eq!(report.shapes[0].lines, [r"^<(?<hp>-?\d+)hp> +$"]);
    assert!(report.shapes[0].settle);
}

#[test]
fn renders_draw_live_or_sample_values_with_overrides() {
    let mut p = Profile::default();
    p.prompt.connect(false);
    p.prompt.observe(
        "Char.Vitals",
        json!({"hp": 850, "maxhp": 900}),
        chrono::Local::now().fixed_offset(),
    );
    let requests: Vec<RenderRequest> = serde_json::from_value(json!([
        {"template": "%hp/%{maxhp}"},
        {"template": "%hp/%{maxhp}", "values": "sample"},
        {"template": "%hp/%{maxhp}", "overrides": {"values": {"hp": 180}}},
        {"template": "%hp/%{maxhp}", "overrides": {"lament": true}},
        {"template": "[%gold]", "placeholders": true},
        {"template": "%hp/%{maxhp}", "preview": "low_health"},
        {"template": "%hp", "preview": "low_health", "overrides": {"values": {"hp": 7}}},
        {"template": "%hp/%{maxhp}", "values": "sample", "preview": "lament"},
        {"template": "%hp", "preview": "now"},
    ]))
    .unwrap();
    let plain: Vec<String> = render_all(&p, &requests)
        .into_iter()
        .map(|r| r.plain)
        .collect();
    assert_eq!(
        plain,
        [
            "850/900",
            "1020/1020",
            "180/900",
            "?/?",
            "[Gold]",
            "180/900",
            "7",
            "?/?",
            "850"
        ]
    );
}

#[test]
fn an_edit_writes_the_design_and_draws_it_with_placeholders() {
    let p = Profile::default();
    let op: EditOp = serde_json::from_value(json!({
        "op": "insert_field",
        "at": 1,
        "field": "gold",
    }))
    .unwrap();
    let edited = edit(&p, "[", &op).unwrap();
    assert_eq!(edited.template, "[%gold");
    assert_eq!(edited.rendered.plain, "[Gold");
    assert_eq!(edited.rendered.spans.len(), 2);
    assert_eq!(edited.piece, Some(1));
    let unknown: EditOp = serde_json::from_value(json!({
        "op": "insert_field",
        "at": 0,
        "field": "nope",
    }))
    .unwrap();
    assert_eq!(
        edit(&p, "[", &unknown),
        Err("Vosh does not know that value.".into())
    );
    // Each op reads from the card in its own shape.
    for op in [
        json!({"op": "set_format", "piece": 0, "format": {"format": "bar", "width": 6}}),
        json!({"op": "set_color", "piece": 0, "color": {"kind": "named", "index": 2}}),
        json!({"op": "set_style", "piece": 0, "style": "italic", "on": true}),
        json!({"op": "set_when", "piece": 0, "when": "not_fight"}),
        json!({"op": "set_text", "piece": 0, "text": "x"}),
        json!({"op": "remove", "piece": 0}),
        json!({"op": "insert_text", "at": 0, "text": "x"}),
        json!({"op": "insert_nl", "at": 0}),
        json!({"op": "move", "piece": 0, "to": 1}),
    ] {
        serde_json::from_value::<EditOp>(op.clone()).unwrap_or_else(|e| panic!("{op}: {e}"));
    }
}

#[test]
fn a_design_is_described_with_the_values_the_card_shows() {
    let mut p = Profile::default();
    p.prompt.connect(false);
    p.prompt.observe(
        "Char.Vitals",
        json!({"hp": 850, "maxhp": 900}),
        chrono::Local::now().fixed_offset(),
    );
    let live = describe(&p, "[%hp]", None, None);
    let hp = &live.pieces[1];
    assert_eq!(hp.label, "Health");
    assert_eq!(hp.meta.as_deref(), Some("850 of 900"));
    let low = describe(&p, "[%hp]", Some(Preview::LowHealth), None);
    assert_eq!(
        low.pieces[1].meta.as_deref(),
        Some("180 of 900 in this preview")
    );
    assert_eq!(low.tokens.len(), 3);
    let json = serde_json::to_value(&live).unwrap();
    assert_eq!(json["pieces"][1]["format"], "value");
    assert_eq!(json["pieces"][1]["color"], json!({"kind": "default"}));
    assert_eq!(json["pieces"][1]["when"], "always");
    assert_eq!(json["tokens"][1]["kind"], "value");
    // The picker's forms, a field with a parameter among them.
    let hp_forms = forms(&p, "hp", None);
    assert_eq!(hp_forms[1].sample.plain, "850/900");
    assert_eq!(hp_forms[1].label, "Current and max");
    let json = serde_json::to_value(&hp_forms[0]).unwrap();
    assert_eq!(json["format"], "value");
    assert_eq!(json["segment"], "850");
    let labels: Vec<&str> = forms(&p, "aff:sanctuary", None)
        .iter()
        .map(|f| f.label)
        .collect();
    assert_eq!(labels, ["Time left", "Mark when on", "Mark when off"]);
}

fn line_trigger(
    name: &str,
    pattern: &str,
    target: vosh_automation::trigger::TriggerTarget,
) -> vosh_automation::trigger::Trigger {
    vosh_automation::trigger::Trigger {
        name: name.into(),
        patterns: vec![vosh_automation::trigger::TriggerPattern::regex(pattern)],
        priority: 5,
        enabled: true,
        actions: Vec::new(),
        preset: None,
        group: None,
        target,
    }
}

#[test]
fn line_triggers_that_match_a_prompt_the_capture_reads_are_named_once() {
    use vosh_automation::trigger::TriggerTarget::{Line, Prompt};
    let mut p = Profile::default();
    p.prompt.connect(true);
    for trigger in [
        line_trigger("Sleep when mana is low", r"\[\d+/\d+hp \d{1,2}/\d+mn", Line),
        line_trigger("Flee below 20 percent", r"\[(\d+)/(\d+)hp", Line),
        line_trigger("Already on prompts", r"hp", Prompt),
        line_trigger("Room exits", r"^\[Exits:", Line),
        vosh_automation::trigger::Trigger {
            enabled: false,
            ..line_trigger("Turned off", r"hp", Line)
        },
        vosh_automation::trigger::Trigger {
            preset: Some("vitals".into()),
            ..line_trigger("From a preset", r"mv\]", Line)
        },
    ] {
        p.triggers.set(trigger).unwrap();
    }
    let mut out = vosh_prompt::stage::Output::new(false);
    for (line, at) in [
        ("[Exits: south]", 1),
        ("[1020/1020hp 8/800mn 930/930mv] ", 2),
        ("[1020/1020hp 800/800mn 930/930mv] ", 3),
    ] {
        p.prompt
            .stage
            .line(&mut out, line.as_bytes(), line, None, b"");
        p.prompt.record(None, at);
    }
    let named = line_triggers(&p, &codes("[%h/%Hhp %m/%Mmn %v/%Vmv]"));
    let names: Vec<(&str, &str, bool)> = named
        .iter()
        .map(|t| (t.name.as_str(), t.pattern.as_str(), t.preset))
        .collect();
    assert_eq!(
        names,
        [
            (
                "Sleep when mana is low",
                r"\[\d+/\d+hp \d{1,2}/\d+mn",
                false
            ),
            ("Flee below 20 percent", r"\[(\d+)/(\d+)hp", false),
            ("From a preset", r"mv\]", true),
        ]
    );
    // No capture reads nothing, so nothing is named.
    let leftover = &line_triggers(&p, &CaptureConfig::None);
    assert!(leftover.is_empty(), "{leftover:?}");
    let json = serde_json::to_value(&named[1]).unwrap();
    assert_eq!(
        json,
        json!({"name": "Flee below 20 percent", "pattern": r"\[(\d+)/(\d+)hp", "preset": false})
    );
}

#[test]
fn the_state_lists_the_catalog_with_live_states() {
    let mut p = Profile::default();
    p.prompt.connect(true);
    p.prompt.observe(
        "Char.Prompt",
        json!({"enabled": true, "prompt": "%h ", "fprompt": ""}),
        chrono::Local::now().fixed_offset(),
    );
    let state = prompt_state(&p);
    assert!(state.new_build);
    // The Forsaken Lands rules hold on its host.
    assert!(state.forsaken);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["status"]["status"], "no_capture");
    assert_eq!(json["open_row"], serde_json::Value::Null);
    let hp = json["catalog"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "hp")
        .unwrap();
    assert_eq!(hp["state"], "missing");
    assert_eq!(hp["group"], "vitals");
    assert_eq!(hp["label"], "Health");
}

#[test]
fn opening_the_card_keeps_the_design_it_found_first() {
    let mut p = Profile::default();
    let config = PromptConfig {
        previous_templates: vec!["older".into()],
        ..PromptConfig::from_legacy(true, "%hp")
    };
    assert_eq!(set_config(&mut p, config), Ok(true));
    let (opened, changed) = card_open(&mut p);
    assert!(changed);
    assert_eq!(opened.previous_templates, ["%hp", "older"]);
    assert_eq!(p.prompt.config().previous_templates, ["%hp", "older"]);
    // Opening again on the same design changes nothing.
    let (again, changed) = card_open(&mut p);
    assert!(!changed);
    assert_eq!(again, opened);
    // An empty design is never kept.
    let empty = PromptConfig {
        template: String::new(),
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, empty), Ok(true));
    assert!(!card_open(&mut p).1);
    // A third design pushes the oldest out.
    let third = PromptConfig {
        template: "%move".into(),
        ..p.prompt.config().clone()
    };
    assert_eq!(set_config(&mut p, third), Ok(true));
    assert_eq!(card_open(&mut p).0.previous_templates, ["%move", "%hp"]);
}

/// A live profile whose `[prompt]` table holds more than Settings
/// shows: a capture and an earlier design.
pub(crate) fn prompt_profile() -> crate::profile::live::Profile {
    let mut p = crate::profile::live::Profile::default();
    p.set_prompt_config(vosh_prompt::PromptConfig {
        draw: true,
        template: "%hp".into(),
        previous_templates: vec!["%mana".into()],
        capture: vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
            lines: vec![r"\[(?<hp>\d+)hp\]".into()],
            ..vosh_prompt::config::RegexCapture::default()
        }),
        ..vosh_prompt::PromptConfig::default()
    });
    p
}

#[test]
fn the_prompt_show_state_says_where_it_shows_and_whether_a_capture_reads_it() {
    let mut p = crate::profile::live::Profile::default();
    assert_eq!(
        super::prompt_show_state(&p),
        super::PromptShowState {
            show: "text".into(),
            capture: false,
            draw: false,
            game_sent: false,
            zone: 1,
            prompts_off: false,
        }
    );
    p = prompt_profile();
    let mut config = p.prompt.config().clone();
    config.show = vosh_prompt::PromptShow::Lifted;
    p.set_prompt_config(config);
    p.prompt.connect(true);
    p.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
        chrono::Local::now().fixed_offset(),
    );
    let state = super::prompt_show_state(&p);
    assert_eq!(state.show, "lifted");
    assert!(state.capture);
    assert_eq!(state.draw, p.prompt.config().draw);
    assert!(state.game_sent);
    assert_eq!(state.zone, 1);
    assert!(!state.prompts_off);
    p.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": false, "prompt": "<%hhp> ", "fprompt": ""}),
        chrono::Local::now().fixed_offset(),
    );
    assert!(super::prompt_show_state(&p).prompts_off);
}

#[tokio::test]
async fn hidden_get_answers_what_the_session_last_reported() {
    let state: super::SharedState = std::sync::Arc::new(AppState::default());
    let nothing = serde_json::json!({
        "vitals": false, "tank": false, "opponent": false, "affects": false, "group": false,
    });
    let json = |h| serde_json::to_value(h).unwrap();
    assert_eq!(json(super::reported_hidden(&state).await), nothing);
    {
        let mut p = state.profile.lock().await;
        p.prompt.connect(true);
        let at = chrono::Local::now().fixed_offset();
        // The older build names the song and sends the true values.
        p.prompt.vars.observe(
            "Char.Affects",
            serde_json::json!({"affects":[{"name":"lamented tears","kind":"song","duration":3}]}),
            at,
        );
        p.prompt.vars.observe(
            "Char.Vitals",
            serde_json::json!({"hp":850,"maxhp":900,"mana":760,"maxmana":820,"move":250,"maxmove":250}),
            at,
        );
    }
    // Worked out, but the session has not reported it yet.
    assert_eq!(json(super::reported_hidden(&state).await), nothing);
    let reported = state.profile.lock().await.prompt.vars.take_hidden_change();
    assert!(reported.is_some_and(|h| h.vitals() && h.affects && h.group));
    assert_eq!(
        super::reported_hidden(&state).await,
        reported.expect("a report")
    );
}
