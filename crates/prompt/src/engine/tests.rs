//! Tests of the engine's table, Char.Prompt, the game's replies and misses.

use super::char_prompt::codes_from_game;
use super::replies::OBSERVE_MS;
use super::*;
use crate::aabahran::Which;
use crate::config::{AabahranCapture, CaptureConfig, CaptureSource, RegexCapture};
use crate::stage::End;
use crate::testkit::at;
use crate::testkit::mud::PROMPT;
use crate::values::{Capture, ClientValues};
use serde_json::json;

fn aabahran() -> PromptConfig {
    PromptConfig {
        capture: CaptureConfig::Aabahran(AabahranCapture::default()),
        ..PromptConfig::default()
    }
}

/// A Forsaken Lands connection on the new build, with a prompt read
/// and a script value set.
fn playing() -> PromptEngine {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.vars.observe(
        "Char.Prompt",
        json!({"enabled": true, "prompt": "%h ", "fprompt": ""}),
        at(),
    );
    engine
        .vars
        .observe("Char.Vitals", json!({"hp": 850, "maxhp": 900}), at());
    engine.vars.capture(Capture {
        values: [("hp".to_string(), "840".to_string())].into(),
        raw: None,
    });
    engine.vars.set_script("mood", "grim");
    engine
}

#[test]
fn the_revision_moves_only_when_the_table_changes() {
    let mut engine = PromptEngine::default();
    assert_eq!(engine.revision(), 0);
    engine.set_config(PromptConfig::default());
    assert_eq!(engine.revision(), 0, "the same table");
    engine.set_config(aabahran());
    assert_eq!(engine.revision(), 1);
    engine.set_config(aabahran());
    assert_eq!(engine.revision(), 1);
    // A session event leaves the table alone.
    engine.connect(true);
    engine.disconnect();
    assert_eq!(engine.revision(), 1);
}

#[test]
fn the_rules_hold_on_the_host_or_with_an_aabahran_capture() {
    let mut engine = PromptEngine::default();
    assert!(!engine.forsaken());
    engine.connect(true);
    assert!(engine.forsaken());
    engine.disconnect();
    assert!(!engine.forsaken(), "no connection, no host");

    engine.set_config(aabahran());
    assert!(engine.forsaken(), "the capture reads Aabahran's codes");
    engine.connect(false);
    assert!(engine.forsaken());
    engine.set_config(PromptConfig::default());
    assert!(!engine.forsaken());
}

#[test]
fn a_switch_keeps_the_packets_and_drops_what_the_prompt_read() {
    let mut engine = playing();
    assert!(engine.vars.new_build());
    assert_eq!(engine.vars.prompt_vars().len(), 2);

    engine.switch_profile();
    engine.set_config(PromptConfig::from_legacy(true, "%hp"));
    assert!(engine.vars.new_build(), "the connection did not change");
    assert!(engine.vars.gmcp().get("Char.Vitals").is_some());
    assert!(engine.vars.prompt_vars().is_empty());
    assert_eq!(engine.config().template, "%hp");
    assert!(engine.forsaken(), "the host still holds the rules");
}

#[test]
fn a_disconnect_clears_the_session_and_keeps_the_table() {
    let mut engine = playing();
    engine.set_config(PromptConfig::from_legacy(true, "%hp"));
    engine.disconnect();
    assert!(!engine.vars.new_build());
    assert!(engine.vars.gmcp().get("Char.Vitals").is_none());
    assert!(engine.vars.prompt_vars().is_empty());
    assert_eq!(engine.config().template, "%hp");
    assert!(engine.config().draw);
}

#[test]
fn the_state_reports_the_fields_the_status_and_the_packages() {
    let mut engine = playing();
    engine.set_config(PromptConfig {
        capture: CaptureConfig::Aabahran(AabahranCapture {
            prompt: "<%hhp %mm> ".into(),
            ..AabahranCapture::default()
        }),
        ..PromptConfig::from_legacy(true, "%hp")
    });
    let state = engine.state(&ClientValues::default());
    assert!(state.new_build);
    assert_eq!(state.status, engine.status_report());
    assert_eq!(state.packages, ["Char.Prompt", "Char.Vitals"]);
    assert_eq!(state.open_row, None);
    let hp = state.catalog.iter().find(|f| f.name == "hp").expect("hp");
    assert!(hp.in_prompt);
    let mood = state.catalog.iter().find(|f| f.name == "mood");
    assert!(mood.is_some(), "a script name lists too");
    let gold = state
        .catalog
        .iter()
        .find(|f| f.name == "gold")
        .expect("gold");
    assert!(!gold.in_prompt);
}

#[test]
fn a_connection_starts_over() {
    let mut engine = playing();
    engine.connect(false);
    assert!(!engine.vars.new_build());
    assert!(engine.vars.prompt_vars().is_empty());
    assert!(!engine.forsaken());
}

#[test]
fn the_card_lends_the_band_while_its_preview_lasts() {
    use crate::values::overrides::PromptPreview;
    let mut engine = PromptEngine::default();
    engine.connect(true);
    assert!(!engine.stage.card_open());
    let labels = PromptPreview {
        placeholders: true,
        ..PromptPreview::default()
    };
    engine.set_preview(Some(labels.clone()));
    assert!(engine.stage.card_open());
    // A preview that draws the live prompt as it is counts as none.
    engine.set_preview(Some(PromptPreview::default()));
    assert!(!engine.stage.card_open());
    engine.set_preview(Some(labels));
    engine.disconnect();
    assert!(!engine.stage.card_open());
}

#[test]
fn the_preview_the_card_set_offline_lasts_through_the_connect() {
    use crate::values::overrides::{Preview, PromptPreview};
    let mut engine = PromptEngine::default();
    let placeholders = PromptPreview {
        placeholders: true,
        ..PromptPreview::default()
    };
    engine.set_preview(Some(placeholders.clone()));
    // The card stays open as you connect, so the first prompt draws
    // what it shows.
    engine.connect(true);
    assert_eq!(engine.preview(), Some(&placeholders));
    let low = PromptPreview {
        preview: Some(Preview::LowHealth),
        ..PromptPreview::default()
    };
    engine.set_preview(Some(low));
    // The connection going clears it.
    engine.disconnect();
    assert_eq!(engine.preview(), None);
}

#[test]
fn a_table_compiles_its_capture_for_the_stage() {
    let mut engine = PromptEngine::default();
    assert!(!engine.stage.has_recognizer());
    engine.set_config(PromptConfig {
        capture: CaptureConfig::Regex(RegexCapture {
            lines: vec![r"<(?<hp>\d+)hp>".into()],
            ..RegexCapture::default()
        }),
        ..PromptConfig::from_legacy(true, "%hp")
    });
    assert!(engine.stage.has_recognizer());
    assert!(engine.draws());
    engine.connect(false);
    assert!(
        engine.stage.has_recognizer(),
        "a connection keeps the capture"
    );
    engine.set_config(PromptConfig::from_legacy(true, "%hp"));
    assert!(!engine.stage.has_recognizer());
    // Drawing needs the switch on and a design.
    engine.set_config(PromptConfig::from_legacy(true, ""));
    assert!(!engine.draws());
    engine.set_config(PromptConfig::from_legacy(false, "%hp"));
    assert!(!engine.draws());
}

#[test]
fn prompt_vars_are_reported_when_they_change() {
    let mut engine = PromptEngine::default();
    engine.connect(false);
    assert_eq!(engine.take_prompt_vars(false), None, "nothing yet");
    assert_eq!(engine.take_prompt_vars(true), Some(BTreeMap::new()));
    engine.vars.set_script("mood", "grim");
    let vars = engine.take_prompt_vars(false).expect("a change");
    assert_eq!(vars.get("mood").map(String::as_str), Some("grim"));
    assert_eq!(engine.take_prompt_vars(false), None, "no change");
    assert!(
        engine.take_prompt_vars(true).is_some(),
        "a prompt asks anyway"
    );
    // A disconnect forgets what the webview heard, since it clears.
    engine.disconnect();
    assert_eq!(engine.take_prompt_vars(false), None);
}

#[test]
fn a_first_capture_reads_the_prompt_already_on_screen() {
    // First use on a profile that read nothing: the game's prompt came
    // and went into the ring before you chose its codes. The values
    // only the prompt shows, such as Wizi, read from it at once, so
    // the start list draws them without waiting for the next prompt.
    let line = "(Wizi 60) [1020/1020hp] ";
    let first = || {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.observe("Char.Vitals", json!({"hp": 1020, "maxhp": 1020}), at());
        // The prompt shows as sent, and its GA puts it in the ring.
        let mut out = crate::stage::Output::new(false);
        let _ = engine.stage.paint_partial(&mut out, line.as_bytes(), None);
        engine.record(Some((line.as_bytes(), line)), 5);
        engine
    };
    let mut engine = first();
    engine.set_config(following("[%h/%Hhp] "));
    let vars = engine.vars.prompt_vars();
    assert_eq!(vars.get("wizi").map(String::as_str), Some("60"));
    assert_eq!(vars.get("hp").map(String::as_str), Some("1020"));

    // A pulse after it means the line is older than what the game
    // sent since, so it reads nothing.
    let mut engine = first();
    engine.observe("Char.Vitals", json!({"hp": 900, "maxhp": 1020}), at());
    engine.set_config(following("[%h/%Hhp] "));
    assert_eq!(engine.vars.prompt_vars().get("wizi"), None);

    // Codes that do not read the line read nothing from it either.
    let mut engine = first();
    engine.set_config(following("<%hhp> "));
    assert_eq!(engine.vars.prompt_vars().get("wizi"), None);

    // Another profile taking over starts with no values read, even
    // when its codes read the line.
    let mut engine = first();
    engine.switch_profile();
    engine.set_config(following("[%h/%Hhp] "));
    assert_eq!(engine.vars.prompt_vars().get("wizi"), None);
}

#[test]
fn the_ring_records_whether_drawing_is_on_and_a_capture_exists() {
    let mut engine = PromptEngine::default();
    engine.connect(false);
    let mut out = crate::stage::Output::new(false);
    engine
        .stage
        .line(&mut out, b"Healer> ", "Healer> ", None, b"Healer> \r\n");
    engine.record(None, 5);
    let entry = engine.stage.ring().next().expect("an entry");
    assert!(!entry.draw);
    assert!(!entry.capture);
}

#[test]
fn an_immortal_reads_pacify_once_char_status_says_so() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(PromptConfig {
        capture: CaptureConfig::Aabahran(AabahranCapture {
            prompt: "<%h %u> ".into(),
            ..AabahranCapture::default()
        }),
        ..PromptConfig::default()
    });
    let pacify = |engine: &PromptEngine| {
        engine
            .stage
            .recognize(b"", "<10 pacified> ", crate::stage::End::Settled)
            .expect("the prompt")
            .values
            .contains_key("pacify")
    };
    assert!(!pacify(&engine), "a mortal until the game says");
    engine.observe(
        "Char.Status",
        json!({"name": "Tester", "level": 60, "race": "human", "class": "warrior"}),
        at(),
    );
    assert!(engine.who().immortal);
    assert!(pacify(&engine));
    // A new connection starts as a mortal again.
    engine.connect(true);
    assert!(!engine.who().immortal);
    assert!(!pacify(&engine));
}

/// A profile with no design of its own, drawing on, that reads
/// `prompt`.
fn mirroring(prompt: &str) -> PromptConfig {
    PromptConfig {
        draw: true,
        capture: CaptureConfig::Aabahran(AabahranCapture {
            prompt: prompt.into(),
            ..AabahranCapture::default()
        }),
        ..PromptConfig::fresh()
    }
}

/// Same as the game for these settings and `who`.
fn game(prompt: &str, fprompt: &str, who: Who) -> String {
    crate::card::presets::game(prompt, fprompt, who).expect("the codes compile")
}

#[test]
fn a_design_that_follows_the_game_follows_each_char_prompt() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(mirroring("<%hhp> "));
    assert_eq!(
        engine.config().template,
        game("<%hhp> ", "", Who::default())
    );
    assert!(engine.draws());
    let revision = engine.revision();

    // You change your prompt and fight prompt in the game.
    let fight = "`1%h``hp [%p] > ";
    engine.observe("Char.Prompt", char_prompt(true, PROMPT, fight), at());
    assert!(engine.config().mirror);
    assert_eq!(
        engine.config().template,
        game(PROMPT, fight, Who::default())
    );
    assert!(engine.revision() > revision);
    let seen = engine.take_seen();
    assert!(seen[0].applied);
    // The design reads what the new codes feed, so nothing is lost.
    let leftover = &seen[0].lost;
    assert!(leftover.is_empty(), "{leftover:?}");

    // A profile switch hands over a table read before the codes moved,
    // and the latest Char.Prompt writes the design again.
    engine.switch_profile();
    engine.set_config(mirroring("<%hhp> "));
    engine.follow_latest(at());
    assert_eq!(
        engine.config().template,
        game(PROMPT, fight, Who::default())
    );
}

#[test]
fn a_design_of_yours_stays_when_your_codes_change() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(following("<%hhp> "));
    engine.observe("Char.Prompt", char_prompt(true, PROMPT, ""), at());
    assert_eq!(codes(&engine).prompt, PROMPT);
    assert_eq!(engine.config().template, "%hp");
    assert!(!engine.config().mirror);
}

#[test]
fn a_design_that_follows_the_game_is_written_for_who_you_are() {
    // A 256 color left open right before %u. For a mortal %u repeats the
    // text of the code before it, which could finish the color, so Vosh
    // cannot draw the setting. For an immortal %u prints a word.
    let prompt = "<`(12%u %h> ";
    let immortal = Who {
        immortal: true,
        ..Who::default()
    };
    let mortal = crate::card::presets::game(prompt, "", Who::default()).unwrap_or_default();
    let drawn = game(prompt, "", immortal);
    // Who you are changes the design, so only a design written again for
    // who you are can match it.
    assert_ne!(mortal, drawn);
    assert_eq!(mortal, "");

    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(mirroring(prompt));
    assert_eq!(engine.config().template, mortal);
    assert!(!engine.draws());
    engine.observe(
        "Char.Status",
        json!({"name": "Tester", "level": 60, "race": "human", "class": "warrior"}),
        at(),
    );
    assert!(engine.who().immortal);
    assert_eq!(engine.config().template, drawn);
    assert!(engine.config().mirror);
    assert!(engine.draws());
    // A new connection starts as a mortal again.
    engine.connect(true);
    assert_eq!(engine.config().template, mortal);
    assert!(engine.config().mirror);
}

/// A profile that follows the game's settings with `prompt`.
fn following(prompt: &str) -> PromptConfig {
    PromptConfig {
        draw: true,
        template: "%hp".into(),
        capture: CaptureConfig::Aabahran(AabahranCapture {
            prompt: prompt.into(),
            ..AabahranCapture::default()
        }),
        ..PromptConfig::default()
    }
}

fn char_prompt(enabled: bool, prompt: &str, fprompt: &str) -> serde_json::Value {
    json!({"enabled": enabled, "prompt": prompt, "fprompt": fprompt})
}

fn codes(engine: &PromptEngine) -> AabahranCapture {
    match &engine.config().capture {
        CaptureConfig::Aabahran(codes) => codes.clone(),
        other => panic!("an aabahran capture, got {other:?}"),
    }
}

#[test]
fn a_changed_char_prompt_updates_the_capture_and_is_noted_once() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(following("<%hhp> "));
    let revision = engine.revision();
    engine.observe(
        "Char.Prompt",
        char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", ""),
        at(),
    );
    assert!(engine.vars.new_build(), "the first packet is the sign");
    let got = codes(&engine);
    assert_eq!(got.prompt, "%n%P%C<%hhp %mm %vmv> ");
    assert_eq!(got.source, Some(CaptureSource::Gmcp));
    assert_eq!(got.seen_at.as_deref(), Some("2026-09-29T12:58:02-05:00"));
    assert!(
        engine.revision() > revision,
        "Settings reads the table again"
    );
    assert_eq!(
        engine.take_seen(),
        [GamePromptSeen {
            kind: SeenKind::Gmcp,
            text: "%n%P%C<%hhp %mm %vmv> ".into(),
            applied: true,
            lost: Vec::new(),
        }]
    );
    let leftover = &engine.take_seen();
    assert!(leftover.is_empty(), "{leftover:?}");
    // The capture reads the new codes at once.
    assert!(engine
        .stage
        .recognize(b"", "<10hp 20m 30mv> ", crate::stage::End::Settled)
        .is_some());

    // The same settings again change nothing and raise no toast.
    let revision = engine.revision();
    engine.observe(
        "Char.Prompt",
        char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", ""),
        at(),
    );
    assert_eq!(engine.revision(), revision);
    assert_eq!(
        engine.take_seen(),
        [GamePromptSeen {
            kind: SeenKind::Gmcp,
            text: "%n%P%C<%hhp %mm %vmv> ".into(),
            applied: false,
            lost: Vec::new(),
        }]
    );

    // A new fight prompt alone names the fight prompt.
    engine.observe(
        "Char.Prompt",
        char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", "`1%h``hp [%p] > "),
        at(),
    );
    assert_eq!(codes(&engine).fprompt, "`1%h``hp [%p] > ");
    assert_eq!(engine.take_seen()[0].text, "`1%h``hp [%p] > ");
}

#[test]
fn enabled_raises_and_clears_prompts_off_and_keeps_the_codes() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(following("%n%P%C<%hhp %mm %vmv> "));
    engine.observe(
        "Char.Prompt",
        char_prompt(false, "%n%P%C<%hhp %mm %vmv> ", ""),
        at(),
    );
    assert!(engine.prompts_off());
    assert_eq!(codes(&engine).prompt, "%n%P%C<%hhp %mm %vmv> ");
    assert!(!engine.take_seen()[0].applied);
    engine.observe(
        "Char.Prompt",
        char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", ""),
        at(),
    );
    assert!(!engine.prompts_off());
    engine.observe(
        "Char.Prompt",
        char_prompt(false, "%n%P%C<%hhp %mm %vmv> ", ""),
        at(),
    );
    engine.disconnect();
    assert!(
        !engine.prompts_off(),
        "a new connection starts with prompts on"
    );
}

#[test]
fn only_an_aabahran_capture_that_follows_the_game_takes_a_char_prompt() {
    for config in [
        PromptConfig::from_legacy(true, "%hp"),
        PromptConfig {
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![r"<(?<hp>\d+)hp>".into()],
                ..RegexCapture::default()
            }),
            ..PromptConfig::default()
        },
        PromptConfig {
            capture: CaptureConfig::Aabahran(AabahranCapture {
                prompt: "<%hhp> ".into(),
                follow_game: false,
                ..AabahranCapture::default()
            }),
            ..PromptConfig::default()
        },
    ] {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(config.clone());
        engine.observe("Char.Prompt", char_prompt(true, "%h %m ", ""), at());
        assert_eq!(*engine.config(), config);
        assert!(!engine.take_seen()[0].applied);
    }
}

#[test]
fn a_switch_applies_the_latest_char_prompt_to_the_new_profile() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(PromptConfig::from_legacy(true, "%hp"));
    engine.observe("Char.Prompt", char_prompt(true, "%h %m ", ""), at());
    let _ = engine.take_seen();

    engine.switch_profile();
    engine.set_config(following("<%hhp> "));
    engine.follow_latest(at());
    assert_eq!(codes(&engine).prompt, "%h %m ");
    assert_eq!(engine.take_seen().len(), 1);

    // A profile that reads nothing saves nothing from it.
    engine.switch_profile();
    engine.set_config(PromptConfig::from_legacy(true, "%hp"));
    engine.follow_latest(at());
    assert!(engine.config().capture.is_none());
    let leftover = &engine.take_seen();
    assert!(leftover.is_empty(), "{leftover:?}");

    // Without a packet this session there is nothing to apply.
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(following("<%hhp> "));
    engine.follow_latest(at());
    assert_eq!(codes(&engine).prompt, "<%hhp> ");
}

/// An engine whose game sent `prompt` in Char.Prompt, under a profile
/// with no design of its own.
fn sent_by_its_game(prompt: &str) -> PromptEngine {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(mirroring("<%hhp> "));
    engine.observe("Char.Prompt", char_prompt(true, prompt, ""), at());
    let _ = engine.take_seen();
    engine
}

#[test]
fn a_choice_made_in_another_session_keeps_the_codes_this_game_sent() {
    let mut engine = sent_by_its_game(PROMPT);
    let own = codes(&engine);
    assert_eq!(own.source, Some(CaptureSource::Gmcp));

    // The other session turned drawing off, pinned the prompt and kept
    // an earlier design, under codes its own game sent.
    let revision = engine.revision();
    engine.take_choice(PromptConfig {
        draw: false,
        show: crate::PromptShow::Pinned,
        previous_templates: vec!["%hp".into()],
        ..mirroring("<%h/%Hhp> ")
    });
    assert!(engine.revision() > revision);
    let config = engine.config();
    assert!(!config.draw);
    assert_eq!(config.show, crate::PromptShow::Pinned);
    assert_eq!(config.previous_templates, ["%hp"]);
    assert_eq!(codes(&engine), own);
    assert!(engine.config().mirror);
    assert_eq!(engine.config().template, game(PROMPT, "", Who::default()));

    // A design you wrote comes over, and the codes still stay.
    engine.take_choice(following("<%h/%Hhp> "));
    assert_eq!(engine.config().template, "%hp");
    assert!(!engine.config().mirror);
    assert_eq!(codes(&engine), own);

    // Codes that follow the game stay, whatever the other session held.
    let mut held = sent_by_its_game(PROMPT);
    held.set_config(PromptConfig {
        capture: CaptureConfig::Aabahran(AabahranCapture {
            follow_game: false,
            ..own.clone()
        }),
        ..held.config().clone()
    });
    held.take_choice(mirroring("<%h/%Hhp> "));
    assert_eq!(codes(&held), own, "following the game again keeps them");
}

#[test]
fn a_capture_you_set_comes_over_and_one_each_game_decides_stays() {
    let mut engine = sent_by_its_game(PROMPT);
    let own = engine.config().capture.clone();

    // Codes that no longer follow the game are yours.
    let fixed = CaptureConfig::Aabahran(AabahranCapture {
        prompt: "<%h/%Hhp> ".into(),
        follow_game: false,
        source: Some(CaptureSource::Typed),
        ..AabahranCapture::default()
    });
    let chosen = |capture: &CaptureConfig| PromptConfig {
        capture: capture.clone(),
        ..mirroring("")
    };
    engine.take_choice(chosen(&fixed));
    assert_eq!(engine.config().capture, fixed);
    // So is a pattern you set, and reading none.
    let pattern = CaptureConfig::Regex(RegexCapture {
        lines: vec![r"<(?<hp>\d+)hp>".into()],
        source: Some(CaptureSource::Typed),
        ..RegexCapture::default()
    });
    engine.take_choice(chosen(&pattern));
    assert_eq!(engine.config().capture, pattern);
    engine.take_choice(chosen(&CaptureConfig::None));
    assert!(engine.config().capture.is_none());
    // Codes from the other game reach an engine that reads none.
    engine.take_choice(chosen(&own));
    assert_eq!(engine.config().capture, own);

    // The pattern a capture trigger left and the codes the game switched
    // it to are each the game's, so each engine keeps its own.
    let migrated = CaptureConfig::Regex(RegexCapture {
        lines: vec![r"<(?<hp>\d+)hp>".into()],
        source: Some(CaptureSource::Migrated),
        ..RegexCapture::default()
    });
    engine.take_choice(chosen(&migrated));
    assert_eq!(engine.config().capture, own);
    let mut moved = PromptEngine::default();
    moved.set_config(chosen(&migrated));
    moved.take_choice(chosen(&own));
    assert_eq!(moved.config().capture, migrated);
}

#[test]
fn every_char_prompt_fixture_is_taken_as_sent() {
    for file in [
        "char-prompt.gmcp",
        "char-prompt-fight.gmcp",
        "char-prompt-off.gmcp",
    ] {
        let path = format!(
            "{}/../../fixtures/gmcp/aabahran/{file}",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let (package, body) = text.trim().split_once(' ').unwrap();
        let data: serde_json::Value = serde_json::from_str(body).unwrap();
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(following(""));
        engine.observe(package, data.clone(), at());
        let got = codes(&engine);
        assert_eq!(got.prompt, data["prompt"].as_str().unwrap(), "{file}");
        assert_eq!(got.fprompt, data["fprompt"].as_str().unwrap(), "{file}");
        assert!(engine.stage.has_recognizer(), "{file} compiles as sent");
        assert_eq!(
            engine.prompts_off(),
            !data["enabled"].as_bool().unwrap(),
            "{file}"
        );
    }
}

/// A Forsaken Lands connection that follows the game with `prompt`,
/// without Char.Prompt.
fn older_build(prompt: &str) -> PromptEngine {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(following(prompt));
    engine
}

fn at_ms(ms: i64) -> chrono::DateTime<chrono::FixedOffset> {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap()
        .fixed_offset()
}

const SENT: i64 = 1_759_000_000_000;

fn line(engine: &mut PromptEngine, text: &str, after_ms: i64) {
    engine.observe_line(text.as_bytes(), text, at_ms(SENT + after_ms));
}

#[test]
fn a_reply_after_your_send_updates_the_capture() {
    // Any send opens the window, prompt abbreviated or another
    // command such as chan.
    for sent in ["prompt %h %m ", "prom %h %m ", "p %h %m ", "chan"] {
        let mut engine = older_build("<%hhp> ");
        engine.note_send(&format!("{sent}\r\n"), SENT);
        line(&mut engine, "Prompt set to %h %m ", 40);
        let got = codes(&engine);
        assert_eq!(got.prompt, "%h %m ", "{sent}");
        assert_eq!(got.source, Some(CaptureSource::Session));
        assert!(got.seen_at.is_some());
        assert_eq!(
            engine.take_seen(),
            [GamePromptSeen {
                kind: SeenKind::Prompt,
                text: "%h %m ".into(),
                applied: true,
                lost: Vec::new(),
            }]
        );
        let seen = engine.session_setting().expect("noted for the card");
        assert_eq!(seen.prompt.as_deref(), Some("%h %m "));
    }
    // An alias that sends it counts the same, since the session hands
    // over what went to the game.
    let mut engine = older_build("<%hhp> ");
    engine.note_send("say hi\r\nfprompt %h>\r\n", SENT);
    line(&mut engine, "Fight prompt set to %h> ", 10);
    assert_eq!(codes(&engine).fprompt, "%h> ");
    assert_eq!(engine.take_seen()[0].kind, SeenKind::Fprompt);
    line(&mut engine, "Fight prompt cleared.", 20);
    assert_eq!(codes(&engine).fprompt, "");
}

#[test]
fn a_new_prompt_names_the_parts_of_your_design_nothing_feeds_any_more() {
    // Same as the game reads your tank's health. On an older build
    // only %P sends it, so dropping %P in the game leaves that part
    // blank, and Vosh says so once.
    let mut engine = older_build("%n%P%C[%h/%Hhp]%c");
    let mut config = engine.config().clone();
    config.template = "%{if:tank}%tank: %{tank_hp:game}%nl%{end}[%hp/%{maxhp}hp]".into();
    engine.set_config(config);
    engine.observe(
        "Char.Vitals",
        json!({"hp": 1020, "maxhp": 1020, "mana": 800, "maxmana": 800, "move": 930, "maxmove": 930}),
        at_ms(SENT),
    );
    engine.note_send("prompt %n%C[%h/%Hhp]%c\r\n", SENT);
    line(&mut engine, "Prompt set to %n%C[%h/%Hhp]%c", 40);
    let seen = engine.take_seen();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].applied);
    // Health still comes from Char.Vitals, so only the tank's health
    // is lost.
    assert_eq!(seen[0].lost, ["tank_hp"]);
    // A change that keeps every part fed names none.
    engine.note_send("prompt %n%P%C[%h/%Hhp]%c\r\n", SENT + 100);
    line(&mut engine, "Prompt set to %n%P%C[%h/%Hhp]%c", 140);
    let leftover = &engine.take_seen()[0].lost;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn on_the_new_build_char_combat_keeps_your_tanks_health() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    let mut config = following("%n%P%C[%h/%Hhp]%c");
    config.template = "%{if:tank}%tank: %{tank_hp:game}%nl%{end}[%hp]".into();
    engine.set_config(config);
    engine.observe(
        "Char.Prompt",
        char_prompt(true, "%n%P%C[%h/%Hhp]%c", ""),
        at(),
    );
    engine.observe(
        "Char.Combat",
        json!({"target": "a rat", "hp_pct": 80, "condition": "fine", "tank": {"name": "Tester", "hp_pct": 90}}),
        at(),
    );
    let _ = engine.take_seen();
    engine.observe(
        "Char.Prompt",
        char_prompt(true, "%n%C[%h/%Hhp]%c", ""),
        at(),
    );
    let seen = engine.take_seen();
    assert!(seen[0].applied);
    assert!(seen[0].lost.is_empty(), "{:?}", seen[0].lost);
}

#[test]
fn a_reply_after_two_seconds_or_with_no_send_is_ignored() {
    let mut engine = older_build("<%hhp> ");
    line(&mut engine, "Prompt set to %h ", 0);
    assert_eq!(codes(&engine).prompt, "<%hhp> ", "no send yet");
    engine.note_send("prompt %h\r\n", SENT);
    line(&mut engine, "Prompt set to %h ", OBSERVE_MS + 1);
    assert_eq!(codes(&engine).prompt, "<%hhp> ");
    let leftover = &engine.take_seen();
    assert!(leftover.is_empty(), "{leftover:?}");
    line(&mut engine, "Prompt set to %h ", OBSERVE_MS);
    assert_eq!(codes(&engine).prompt, "%h ");
}

#[test]
fn prompt_off_saves_nothing_and_raises_prompts_off_in_either_order() {
    // Your own send said prompt off.
    let mut engine = older_build("<%hhp> ");
    engine.note_send("prompt off\r\n", SENT);
    line(&mut engine, "Prompt set to garbage", 5);
    assert_eq!(codes(&engine).prompt, "<%hhp> ");
    assert!(engine.prompts_off());

    // An alias sent it under another name, and the reply comes after
    // the line the older builds print first.
    let mut engine = older_build("<%hhp> ");
    engine.note_send("quiet\r\n", SENT);
    line(&mut engine, "You will no longer see prompts.", 5);
    assert!(engine.prompts_off());
    line(&mut engine, "Prompt set to garbage", 6);
    assert_eq!(codes(&engine).prompt, "<%hhp> ");
    let seen = engine.take_seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].kind, SeenKind::Off);

    // prompt turns them on again with Current prompt, and a
    // recognized prompt clears them too.
    engine.note_send("prompt\r\n", SENT + 3_000);
    line(&mut engine, "Current prompt: <%hhp> ", 3_010);
    assert!(!engine.prompts_off());
    engine.note_send("prompt off\r\n", SENT + 4_000);
    line(&mut engine, "You will no longer see prompts.", 4_010);
    assert!(engine.prompts_off());
    engine.note_prompt(at());
    assert!(!engine.prompts_off());
}

#[test]
fn prompt_after_prompt_off_keeps_your_codes_over_the_leftover_buffer() {
    // An older build's prompt off stores a buffer it never filled,
    // and prompt with no argument then shows it, control bytes and
    // all. That is no setting of yours: prompts come back on and the
    // codes you have stay.
    for reply in ["Current prompt: \u{1}\u{2}", "Prompt set to \u{1}\u{2}"] {
        let mut engine = older_build("<%hhp> ");
        engine.note_send("prompt off\r\n", SENT);
        line(&mut engine, "You will no longer see prompts.", 5);
        line(&mut engine, "Prompt set to \u{1}\u{2}", 6);
        assert!(engine.prompts_off());
        engine.take_seen();
        engine.note_send("prompt\r\n", SENT + 3_000);
        line(&mut engine, reply, 3_010);
        assert_eq!(codes(&engine).prompt, "<%hhp> ", "{reply:?}");
        assert!(!engine.prompts_off(), "{reply:?} turns prompts on");
        assert!(engine.take_seen().is_empty(), "{reply:?}");
    }
}

#[test]
fn channels_shows_your_prompt_and_turns_nothing_on() {
    // prompt off on an older build stores a buffer it never filled.
    let mut engine = older_build("<%hhp> ");
    engine.note_send("prompt off\r\n", SENT);
    line(&mut engine, "You will no longer see prompts.", 5);
    line(&mut engine, "Prompt set to \u{1}\u{2}", 6);
    let _ = engine.take_seen();
    // channels shows that buffer and leaves prompts off.
    engine.note_send("channels\r\n", SENT + 3_000);
    line(&mut engine, "Your current prompt is: \u{1}\u{2}", 3_010);
    assert!(engine.prompts_off());
    assert_eq!(engine.status(), Status::PromptsOff);
    assert_eq!(codes(&engine).prompt, "<%hhp> ");
    let leftover = &engine.take_seen();
    assert!(leftover.is_empty(), "{leftover:?}");
    // With prompts on, the setting it shows is yours.
    engine.note_send("prompt %h\r\n", SENT + 4_000);
    line(&mut engine, "Prompt set to %h ", 4_010);
    assert!(!engine.prompts_off());
    engine.note_send("channels\r\n", SENT + 5_000);
    line(&mut engine, "Your current prompt is: %h %m ", 5_010);
    assert_eq!(codes(&engine).prompt, "%h %m ");
    assert!(!engine.prompts_off());
}

#[test]
fn with_char_prompt_this_session_the_observer_does_nothing() {
    let mut engine = older_build("<%hhp> ");
    engine.observe("Char.Prompt", char_prompt(true, "%h ", ""), at());
    let _ = engine.take_seen();
    engine.note_send("prompt %m\r\n", SENT);
    assert!(!engine.observing(SENT + 10));
    line(&mut engine, "Prompt set to %m ", 10);
    assert_eq!(codes(&engine).prompt, "%h ");
    let leftover = &engine.take_seen();
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(engine.session_setting().is_none());
}

#[test]
fn the_observer_keeps_to_the_forsaken_lands() {
    let mut engine = PromptEngine::default();
    engine.connect(false);
    engine.note_send("prompt %h\r\n", SENT);
    assert!(!engine.observing(SENT));
    // A profile without a capture still notes the setting for the
    // card on The Forsaken Lands.
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.note_send("prompt %h\r\n", SENT);
    line(&mut engine, "Prompt set to %h ", 1);
    assert!(engine.config().capture.is_none());
    assert_eq!(
        engine.session_setting().and_then(|s| s.prompt.as_deref()),
        Some("%h ")
    );
    assert!(!engine.take_seen()[0].applied);
}

#[test]
fn the_code_reader_the_card_chose_reads_the_replies_on_another_host() {
    // A local server of The Forsaken Lands, with no capture yet. More
    // > Use Forsaken Lands prompt codes… gives it the rules, so
    // the reply to prompt fills the card's fields.
    let mut engine = PromptEngine::default();
    engine.connect(false);
    engine.set_reader(true);
    assert!(engine.forsaken());
    engine.note_send("prompt\r\n", SENT);
    line(
        &mut engine,
        "Current prompt: %n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c",
        40,
    );
    let seen = engine.take_seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].kind, SeenKind::Prompt);
    assert_eq!(seen[0].text, "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c");
    assert!(!seen[0].applied, "no capture takes it before you save");

    // The card stays open across a connection, and the rules with it.
    engine.disconnect();
    engine.connect(false);
    assert!(engine.forsaken());

    // Once the card lets it go, the host plays by its own rules.
    engine.set_reader(false);
    assert!(!engine.forsaken());
    engine.note_send("prompt\r\n", SENT + 3_000);
    line(&mut engine, "Current prompt: %h ", 3_040);
    let leftover = &engine.take_seen();
    assert!(leftover.is_empty(), "{leftover:?}");

    // Another profile taking over lets it go too.
    engine.set_reader(true);
    engine.switch_profile();
    assert!(!engine.forsaken());
}

fn vitals(engine: &mut PromptEngine) {
    engine.observe("Char.Vitals", json!({"hp": 10, "maxhp": 20}), at());
}

#[test]
fn a_design_reads_a_clock_only_with_a_clock_piece_and_drawing_on() {
    let clock = |template: &str, draw: bool| {
        let mut engine = PromptEngine::default();
        engine.set_config(PromptConfig {
            draw,
            template: template.into(),
            ..following("<%hhp> ")
        });
        engine.clock()
    };
    let tick = Clock {
        tick: true,
        wall: false,
    };
    let wall = Clock {
        tick: false,
        wall: true,
    };
    assert_eq!(clock("<%hp> %tick", true), Some(tick));
    assert_eq!(clock("%{tick:bar:10} ", true), Some(tick));
    // A color that follows the tick changes with it too.
    assert_eq!(clock("%bg_tick%hp ", true), Some(tick));
    assert_eq!(clock("%{time:hms} ", true), Some(wall));
    assert_eq!(clock("%{date:md} ", true), Some(wall));
    assert_eq!(
        clock("%{if:tick}%tick%{end} %time", true),
        Some(Clock {
            tick: true,
            wall: true
        })
    );
    assert_eq!(clock("<%hp %mana %move> ", true), None);
    assert_eq!(clock(crate::DEFAULT_DESIGN, true), None);
    assert_eq!(clock("<%hp> %tick", false), None);
    assert_eq!(clock("", true), None);
}

#[test]
fn three_pulses_without_a_prompt_are_not_matching_and_one_match_clears_it() {
    let mut engine = older_build("<%hhp> ");
    assert_eq!(engine.status(), Status::Matching);
    let first = engine.take_status_change().expect("the first report");
    assert_eq!(first.status, Status::Matching);
    assert_eq!(first.last_match_at, None);
    assert_eq!(engine.take_status_change(), None, "no change");
    // The pulse that starts the session is no miss.
    vitals(&mut engine);
    vitals(&mut engine);
    vitals(&mut engine);
    assert_eq!(engine.status(), Status::Matching, "two misses");
    vitals(&mut engine);
    assert_eq!(engine.status(), Status::NotMatching);
    assert_eq!(
        engine.take_status_change().map(|r| r.status),
        Some(Status::NotMatching)
    );
    engine.note_prompt(at());
    let report = engine.take_status_change().expect("matching again");
    assert_eq!(report.status, Status::Matching);
    assert_eq!(
        report.last_match_at.as_deref(),
        Some("2026-09-29T12:58:02-05:00")
    );
    // A pulse with a prompt in it is no miss.
    for _ in 0..5 {
        vitals(&mut engine);
        engine.note_prompt(at());
    }
    assert_eq!(engine.status(), Status::Matching);
}

#[test]
fn pulses_with_prompts_off_count_no_miss() {
    let mut engine = older_build("%n%P%C<%hhp %mm %vmv> ");
    engine.observe(
        "Char.Prompt",
        char_prompt(false, "%n%P%C<%hhp %mm %vmv> ", ""),
        at(),
    );
    // The new build keeps sending the packages each pulse.
    for _ in 0..5 {
        vitals(&mut engine);
    }
    assert_eq!(engine.status(), Status::PromptsOff);
    engine.observe(
        "Char.Prompt",
        char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", ""),
        at(),
    );
    assert_eq!(engine.status(), Status::Matching, "no misses were kept");
}

#[test]
fn a_profile_without_a_capture_misses_nothing() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    for _ in 0..5 {
        vitals(&mut engine);
    }
    assert_eq!(engine.status(), Status::NoCapture);
    engine.set_config(following("<%hhp> "));
    assert_eq!(engine.status(), Status::Matching);
}

#[test]
fn other_games_count_misses_by_send() {
    let mut engine = PromptEngine::default();
    engine.connect(false);
    engine.set_config(PromptConfig {
        capture: CaptureConfig::Regex(RegexCapture {
            lines: vec![r"^<(?<hp>\d+)hp> $".into()],
            ..RegexCapture::default()
        }),
        ..PromptConfig::default()
    });
    // Char.Vitals starts no miss here, and a send with no reply is
    // none either.
    for _ in 0..4 {
        vitals(&mut engine);
        engine.note_send("look\r\n", 0);
    }
    assert_eq!(engine.status(), Status::Matching);
    for _ in 0..3 {
        engine.note_text();
        engine.note_send("look\r\n", 0);
    }
    assert_eq!(engine.status(), Status::NotMatching);
    engine.note_text();
    engine.note_prompt(at());
    assert_eq!(engine.status(), Status::Matching);
}

#[test]
fn a_new_table_keeps_the_values() {
    let mut engine = playing();
    engine.set_config(PromptConfig::from_legacy(false, "%mana"));
    assert_eq!(engine.vars.prompt_vars().len(), 2);
    assert!(engine.vars.new_build());
}

/// The pattern the old capture trigger held.
const OLD_PATTERN: &str =
    r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";
/// The PROMPT that pattern was written for, as the game stores it.
const OLD: &str = PROMPT;
/// The PROMPT James set after the move, as the game stores it.
const NEW: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv (%K hp) %s [%S]> ";
/// A design that draws in place of the prompt.
const DESIGN: &str = "%{c:100,100,100}[%c_reset%s_italic%hp]";

/// A table as the move from the capture trigger wrote it.
fn migrated() -> PromptConfig {
    PromptConfig {
        draw: true,
        template: DESIGN.into(),
        previous_templates: vec!["%hp".into()],
        capture: CaptureConfig::Regex(RegexCapture {
            lines: vec![OLD_PATTERN.into()],
            settle: false,
            source: Some(CaptureSource::Migrated),
            ..RegexCapture::default()
        }),
        ..PromptConfig::default()
    }
}

/// A Forsaken Lands connection whose profile holds the migrated
/// capture.
fn moved() -> PromptEngine {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(migrated());
    engine
}

/// The table stays as it was apart from the capture.
fn keeps_the_rest(engine: &PromptEngine) {
    let config = engine.config();
    assert!(config.draw);
    assert_eq!(config.template, DESIGN);
    assert_eq!(config.previous_templates, ["%hp"]);
}

#[test]
fn the_first_char_prompt_switches_a_migrated_capture_to_the_codes() {
    let mut engine = moved();
    let revision = engine.revision();
    engine.observe("Char.Prompt", char_prompt(true, OLD, "`1%h``> "), at());
    assert_eq!(
        codes(&engine),
        AabahranCapture {
            prompt: OLD.into(),
            fprompt: "`1%h``> ".into(),
            follow_game: true,
            seen_at: Some("2026-09-29T12:58:02-05:00".into()),
            source: Some(CaptureSource::Gmcp),
        }
    );
    keeps_the_rest(&engine);
    assert!(engine.revision() > revision, "Settings reads it again");
    assert_eq!(
        engine.take_seen(),
        [GamePromptSeen {
            kind: SeenKind::Gmcp,
            text: OLD.into(),
            applied: true,
            lost: Vec::new(),
        }],
        "the toast follows"
    );
    assert!(engine.forsaken());
    assert_eq!(engine.kept_pattern(), None);
    // The codes compile at once, so the next prompt draws.
    let block = engine
        .stage
        .recognize(b"", "[1020/1020hp 800/800mn 930/930mv]", End::Line)
        .expect("the prompt");
    assert_eq!(block.values.get("maxmove").map(String::as_str), Some("930"));

    // From here it follows the game as any aabahran capture does.
    engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
    assert_eq!(codes(&engine).prompt, NEW);
    assert!(engine.take_seen()[0].applied);
    let line = "(Wizi 60) (Incog 60) [1020/1020hp 800/800mn 930/930mv (100 hp) common [std]> ";
    let block = engine
        .stage
        .recognize(b"", line, End::Settled)
        .expect("the new prompt");
    assert_eq!(block.values.get("wizi").map(String::as_str), Some("60"));
    assert_eq!(block.values.get("hp_pct").map(String::as_str), Some("100"));
    keeps_the_rest(&engine);
}

#[test]
fn the_codes_from_the_game_keep_the_settings_as_sent_and_follow_the_game() {
    let got = codes_from_game(
        NEW,
        "`1%h``hp> ",
        CaptureSource::Session,
        at(),
        Who::default(),
    )
    .expect("they compile");
    assert_eq!(
        got,
        AabahranCapture {
            prompt: NEW.into(),
            fprompt: "`1%h``hp> ".into(),
            follow_game: true,
            seen_at: Some("2026-09-29T12:58:02-05:00".into()),
            source: Some(CaptureSource::Session),
        }
    );
    let error = codes_from_game("<`%h> ", "", CaptureSource::Gmcp, at(), Who::default())
        .expect_err("a color runs into %h");
    assert_eq!(error.code, "%h");
    assert_eq!(error.which, Which::Prompt);
}

#[test]
fn prompts_off_still_switches_a_migrated_capture() {
    let mut engine = moved();
    engine.observe("Char.Prompt", char_prompt(false, OLD, ""), at());
    assert_eq!(codes(&engine).prompt, OLD);
    assert!(engine.prompts_off());
    assert_eq!(engine.status(), Status::PromptsOff);
    assert!(engine.take_seen()[0].applied);
}

#[test]
fn only_the_migrated_capture_switches() {
    let typed = PromptConfig {
        capture: CaptureConfig::Regex(RegexCapture {
            lines: vec![OLD_PATTERN.into()],
            source: Some(CaptureSource::Typed),
            ..RegexCapture::default()
        }),
        ..migrated()
    };
    let unsourced = PromptConfig {
        capture: CaptureConfig::Regex(RegexCapture {
            lines: vec![OLD_PATTERN.into()],
            ..RegexCapture::default()
        }),
        ..migrated()
    };
    let none = PromptConfig {
        capture: CaptureConfig::None,
        ..migrated()
    };
    for config in [typed, unsourced, none] {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(config.clone());
        engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
        assert_eq!(*engine.config(), config);
        assert!(!engine.take_seen()[0].applied);
        engine.note_send("prompt x\r\n", SENT);
        line(&mut engine, "Prompt set to x ", 5);
        assert_eq!(*engine.config(), config);
    }
}

#[test]
fn a_migrated_capture_switches_only_under_the_forsaken_lands_rules() {
    let mut engine = PromptEngine::default();
    engine.connect(false);
    engine.set_config(migrated());
    assert!(!engine.forsaken());
    engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
    assert_eq!(*engine.config(), migrated());
    assert!(!engine.take_seen()[0].applied);
}

#[test]
fn a_prompt_that_does_not_compile_keeps_the_pattern_and_says_why() {
    let mut engine = moved();
    engine.observe("Char.Prompt", char_prompt(true, "<`%h> ", ""), at());
    assert_eq!(*engine.config(), migrated());
    assert!(engine.stage.has_recognizer(), "the pattern still reads");
    assert!(!engine.take_seen()[0].applied, "no toast");
    assert_eq!(
        engine.kept_pattern().as_deref(),
        Some("Vosh kept the pattern from your old capture trigger because a color code runs into %h in the prompt the game sent.")
    );
    // A fight prompt that does not compile says so.
    engine.observe("Char.Prompt", char_prompt(true, OLD, "`(2%f1 "), at());
    assert_eq!(*engine.config(), migrated());
    assert_eq!(
        engine.kept_pattern().as_deref(),
        Some("Vosh kept the pattern from your old capture trigger because a color code runs into %f1 in the fight prompt the game sent.")
    );
    // The next prompt that compiles switches it, and the reason goes.
    engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
    assert_eq!(codes(&engine).prompt, OLD);
    assert_eq!(engine.kept_pattern(), None);

    // A new connection starts with no reason.
    let mut engine = moved();
    engine.observe("Char.Prompt", char_prompt(true, "<`%h> ", ""), at());
    assert!(engine.kept_pattern().is_some());
    engine.disconnect();
    assert_eq!(engine.kept_pattern(), None);
    assert_eq!(*engine.config(), migrated());
}

#[test]
fn the_reply_to_your_prompt_switches_a_migrated_capture_without_char_prompt() {
    let mut engine = moved();
    // A fight prompt alone says nothing of your PROMPT.
    engine.note_send("fprompt\r\n", SENT);
    line(&mut engine, "Current fight prompt: <%hhp fight> ", 5);
    assert_eq!(*engine.config(), migrated());
    assert!(!engine.take_seen()[0].applied);

    engine.note_send("prom x\r\n", SENT + 3_000);
    line(&mut engine, "Prompt set to <%hhp %mm> ", 3_010);
    let got = codes(&engine);
    assert_eq!(got.prompt, "<%hhp %mm> ");
    assert_eq!(got.fprompt, "<%hhp fight> ", "the fight prompt it showed");
    assert!(got.follow_game);
    assert_eq!(got.source, Some(CaptureSource::Session));
    assert!(got.seen_at.is_some());
    keeps_the_rest(&engine);
    assert_eq!(
        engine.take_seen(),
        [GamePromptSeen {
            kind: SeenKind::Prompt,
            text: "<%hhp %mm> ".into(),
            applied: true,
            lost: Vec::new(),
        }]
    );

    // What channels shows switches it too.
    let mut engine = moved();
    engine.note_send("channels\r\n", SENT);
    line(&mut engine, "Your current prompt is: <%hhp> ", 5);
    assert_eq!(codes(&engine).prompt, "<%hhp> ");
    assert_eq!(codes(&engine).fprompt, "");
}

#[test]
fn the_reply_to_prompt_off_switches_nothing() {
    // Your own send said prompt off.
    let mut engine = moved();
    engine.note_send("prompt off\r\n", SENT);
    line(&mut engine, "Prompt set to \u{1}\u{2}", 5);
    assert_eq!(*engine.config(), migrated());
    assert!(engine.prompts_off());
    // An alias sent it, and the reply follows the line it prints first.
    let mut engine = moved();
    engine.note_send("quiet\r\n", SENT);
    line(&mut engine, "You will no longer see prompts.", 5);
    line(&mut engine, "Prompt set to \u{1}\u{2}", 6);
    assert_eq!(*engine.config(), migrated());
    // channels while prompts are off shows no setting of yours.
    engine.note_send("channels\r\n", SENT + 3_000);
    line(&mut engine, "Your current prompt is: \u{1}\u{2}", 3_010);
    assert_eq!(*engine.config(), migrated());
    // A reply after two seconds counts for nothing either.
    engine.note_send("prompt x\r\n", SENT + 4_000);
    line(&mut engine, "Prompt set to <%hhp> ", 4_000 + OBSERVE_MS + 1);
    assert_eq!(*engine.config(), migrated());
    assert!(engine.take_seen().iter().all(|s| !s.applied));
}

#[test]
fn a_switch_applies_the_latest_char_prompt_to_a_migrated_capture() {
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(PromptConfig::from_legacy(true, "%hp"));
    engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
    assert!(!engine.take_seen()[0].applied, "default reads nothing");

    engine.switch_profile();
    engine.set_config(migrated());
    engine.follow_latest(at());
    assert_eq!(codes(&engine).prompt, NEW);
    assert_eq!(codes(&engine).source, Some(CaptureSource::Gmcp));
    keeps_the_rest(&engine);
    assert_eq!(engine.take_seen().len(), 1);

    // The reason a switch kept the pattern is the new profile's.
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(migrated());
    engine.observe("Char.Prompt", char_prompt(true, "<`%h> ", ""), at());
    assert!(engine.kept_pattern().is_some());
    assert!(!engine.take_seen()[0].applied);
    engine.switch_profile();
    engine.set_config(PromptConfig::from_legacy(true, "%hp"));
    engine.follow_latest(at());
    assert_eq!(engine.kept_pattern(), None);
    assert!(engine.config().capture.is_none());
    engine.switch_profile();
    engine.set_config(migrated());
    engine.follow_latest(at());
    assert_eq!(*engine.config(), migrated());
    assert!(engine.kept_pattern().is_some());
    let leftover = &engine.take_seen();
    assert!(leftover.is_empty(), "{leftover:?}");
}

/// A migrated table whose pattern reads `pattern`, with `names` for
/// its groups, and a design that reads `design`.
fn migrated_with(pattern: &str, names: &[(&str, &str)], design: &str) -> PromptConfig {
    PromptConfig {
        draw: true,
        template: design.into(),
        previous_templates: Vec::new(),
        capture: CaptureConfig::Regex(RegexCapture {
            lines: vec![pattern.into()],
            names: names
                .iter()
                .map(|(group, var)| ((*group).to_string(), (*var).to_string()))
                .collect(),
            source: Some(CaptureSource::Migrated),
            ..RegexCapture::default()
        }),
        ..PromptConfig::default()
    }
}

#[test]
fn a_pattern_that_fills_a_name_no_code_fills_keeps_the_pattern_and_says_why() {
    // Groups named for themselves, outside the catalog, as the help
    // invites you to name them.
    let own = migrated_with(r"\[(?<h>\d+)/(?<mh>\d+)hp\]", &[], "[%h/%mh hp]");
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(own.clone());
    engine.observe("Char.Prompt", char_prompt(true, "[%h/%Hhp]%c", ""), at());
    assert_eq!(*engine.config(), own, "the pattern and the design stay");
    assert!(!engine.take_seen()[0].applied, "no toast and no save");
    assert_eq!(
        engine.kept_pattern().as_deref(),
        Some("Vosh kept the pattern from your old capture trigger because it fills values named h and mh, and no prompt code fills those names.")
    );
    // The pattern still reads the values the design shows.
    let block = engine
        .stage
        .recognize(b"", "[100/200hp]", End::Line)
        .expect("the prompt");
    assert_eq!(block.values.get("h").map(String::as_str), Some("100"));
    assert_eq!(block.values.get("mh").map(String::as_str), Some("200"));
    // No PROMPT the game shows later changes that.
    engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
    assert_eq!(*engine.config(), own);
    assert!(engine.kept_pattern().is_some());

    // A group the old trigger handed to a name of its own, through the
    // names the move wrote.
    let named = migrated_with(
        r"\[(?<h>\d+)/(?<maxhp>\d+)hp",
        &[("h", "health")],
        "HP=%health",
    );
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(named.clone());
    engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
    assert_eq!(*engine.config(), named);
    assert_eq!(
        engine.kept_pattern().as_deref(),
        Some("Vosh kept the pattern from your old capture trigger because it fills a value named health, and no prompt code fills that name.")
    );

    // The observer keeps it too, on a build without Char.Prompt, and
    // a profile switch that hands it the latest packet.
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(named.clone());
    engine.note_send("prompt x\r\n", SENT);
    line(&mut engine, "Prompt set to <%hhp %Hmhp> ", 5);
    assert_eq!(*engine.config(), named);
    assert!(!engine.take_seen()[0].applied);
    assert!(engine.kept_pattern().is_some());
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
    engine.switch_profile();
    engine.set_config(named.clone());
    engine.follow_latest(at());
    assert_eq!(*engine.config(), named);
    assert!(engine.take_seen().iter().all(|s| !s.applied));
    assert!(engine.kept_pattern().is_some());
}

#[test]
fn a_pattern_that_fills_only_names_vosh_knows_switches() {
    // A group the old trigger never read, another spelling of a max,
    // and a percent the codes fill all leave the switch alone.
    let known = migrated_with(
        r"\[(?<hp>\d+)/(?<mhp>\d+)hp (?<hp_pct>\d+)% (?<extra>\w+)\]",
        &[("extra", "")],
        "%hp/%mhp",
    );
    let mut engine = PromptEngine::default();
    engine.connect(true);
    engine.set_config(known);
    engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
    assert_eq!(codes(&engine).prompt, NEW);
    assert_eq!(engine.kept_pattern(), None);

    // A value the game's PROMPT no longer shows goes with it, since
    // no code fills a name the catalog knows but the prompt leaves out.
    let mut engine = moved();
    engine.observe("Char.Prompt", char_prompt(true, "<%hhp %mm> ", ""), at());
    assert_eq!(codes(&engine).prompt, "<%hhp %mm> ");
    assert!(engine.take_seen()[0].applied);
}

#[test]
fn a_reconnect_keeps_the_pattern_until_the_game_shows_your_prompt() {
    let mut engine = moved();
    // A link dead reconnect sends no Char.Prompt, and the pattern
    // reads the prompt as before.
    vitals(&mut engine);
    assert_eq!(*engine.config(), migrated());
    assert!(engine
        .stage
        .recognize(b"", "[1020/1020hp 800/800mn 930/930mv]", End::Line)
        .is_some());
    // prompt in the game sends it.
    engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
    assert_eq!(codes(&engine).prompt, OLD);
}

/// An engine reading Aabahran's `prompt` and drawing `template` while
/// `draw` is on.
fn zoned(prompt: &str, template: &str, draw: bool) -> PromptEngine {
    let mut engine = PromptEngine::default();
    engine.set_config(PromptConfig {
        draw,
        template: template.into(),
        capture: CaptureConfig::Aabahran(AabahranCapture {
            prompt: prompt.into(),
            ..AabahranCapture::default()
        }),
        ..PromptConfig::default()
    });
    engine
}

#[test]
fn the_band_keeps_the_rows_the_tallest_prompt_can_take() {
    // One line, no tank code: one row.
    assert_eq!(zoned("[%h/%Hhp]%c", "<%hp>", true).zone(), 1);
    // James's PROMPT prints the tank line above the vitals in a fight.
    // A design that reads nothing on it leaves it as sent.
    assert_eq!(zoned(PROMPT, "<%hp>", true).zone(), 2);
    // One that reads the tank takes it over.
    assert_eq!(
        zoned(PROMPT, "%tank %{tank_hp:pct}%% <%hp>", true).zone(),
        1
    );
    // Every line break counts, inside a condition too.
    let detailed = "%{if:fight}%opponent%nl%{end}%hp";
    assert_eq!(zoned(PROMPT, detailed, true).zone(), 3);
    assert_eq!(
        zoned("[%h/%Hhp]%c", "%hp%{nl}%mana%nl%move", true).zone(),
        3
    );
    // A design that reads the prompt as sent takes its lines.
    assert_eq!(zoned(PROMPT, "%{raw}", true).zone(), 2);
    assert_eq!(zoned(PROMPT, "%{raw}%nl%hp", true).zone(), 3);
    // Not drawing, the game's own lines.
    assert_eq!(zoned(PROMPT, detailed, false).zone(), 2);
    assert_eq!(zoned("[%h/%Hhp]%c", "", true).zone(), 1);
    // No more than six.
    let tall = "%hp%nl%hp%nl%hp%nl%hp%nl%hp%nl%hp%nl%hp%nl%hp";
    assert_eq!(zoned(PROMPT, tall, true).zone(), crate::stage::ZONE_MAX);
}

#[test]
fn a_regex_capture_and_no_capture_keep_one_row_and_its_breaks() {
    let mut engine = PromptEngine::default();
    assert_eq!(engine.zone(), 1);
    engine.set_config(PromptConfig {
        draw: true,
        template: "%hp%nl%mana".into(),
        capture: CaptureConfig::Regex(RegexCapture {
            lines: vec![r"\[(?<hp>\d+)hp\]".into()],
            ..RegexCapture::default()
        }),
        ..PromptConfig::default()
    });
    assert_eq!(engine.zone(), 2);
}
