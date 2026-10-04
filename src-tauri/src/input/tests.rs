use super::profile::{load_profile_file, slash_profile, PROFILE_SAVE_BUSY};
use super::script::slash_script;
use super::slash::{parse_braced_pattern, HELP_TEXT};
use super::target::{read_room_chars, set_room_chars};
use super::*;
use crate::profile::file::ProfileConfig;
use crate::profile::live::RoomChar;
use vosh_automation::alias::Alias;
use vosh_automation::trigger::{NamedColor, TriggerAction};
use vosh_automation::vars::Scope;

fn regex_capture(p: &Profile) -> vosh_prompt::config::RegexCapture {
    match &p.prompt.config().capture {
        vosh_prompt::CaptureConfig::Regex(capture) => capture.clone(),
        other => panic!("a regex capture, got {other:?}"),
    }
}

#[test]
fn prompt_writes_a_regex_capture_to_the_profile() {
    let state = AppState::default();
    let mut p = Profile::default();
    p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
    let ran = run_line(
        &state,
        &mut p,
        r"#prompt {\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)m\]}",
    );
    assert_eq!(
        ran.result.echo,
        ["Vosh reads hp, maxhp, and mana from your prompt with this pattern."]
    );
    let capture = regex_capture(&p);
    assert_eq!(
        capture.lines,
        [r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)m\]"]
    );
    assert!(
        !capture.settle,
        "an unanchored pattern waits for a line end"
    );
    assert_eq!(
        capture.source,
        Some(vosh_prompt::config::CaptureSource::Typed)
    );
    assert!(capture.seen_at.is_some());
    assert!(capture.names.is_empty());
    // The switch and the design stay, and no trigger is written.
    assert!(p.prompt.config().draw);
    assert_eq!(p.prompt.config().template, "%hp");
    assert!(p.triggers.get("prompt-capture").is_none());
    assert!(p.prompt.stage.has_recognizer());

    // An anchored pattern that ends in text settles.
    let ran = run_line(&state, &mut p, r"#prompt {^<(?<hp>\d+)hp> $}");
    assert_eq!(
        ran.result.echo,
        ["Vosh reads hp from your prompt with this pattern."]
    );
    assert!(regex_capture(&p).settle);
    // A pattern with no groups only says where your prompt is.
    let ran = run_line(&state, &mut p, "#prompt {^> $}");
    assert_eq!(
        ran.result.echo,
        ["Vosh reads your prompt with this pattern."]
    );
}

#[test]
fn prompt_with_a_bad_pattern_changes_nothing() {
    let state = AppState::default();
    let mut p = Profile::default();
    let _ = run_line(&state, &mut p, r"#prompt {^<(?<hp>\d+)hp> $}");
    let before = p.prompt.config().clone();
    let ran = run_line(&state, &mut p, r"#prompt {\[(?<hp>\d+}");
    assert!(
        ran.result.echo[0].starts_with("[Vosh cannot read that pattern."),
        "{:?}",
        ran.result.echo
    );
    assert_eq!(*p.prompt.config(), before);
    let ran = run_line(&state, &mut p, "#prompt {");
    assert!(ran.result.echo[0].starts_with("[usage #prompt"));
}

fn codes_of(p: &Profile) -> vosh_prompt::config::AabahranCapture {
    match &p.prompt.config().capture {
        vosh_prompt::CaptureConfig::Aabahran(codes) => codes.clone(),
        other => panic!("an aabahran capture, got {other:?}"),
    }
}

#[test]
fn prompt_game_stores_the_setting_as_the_game_does_and_says_what_it_reads() {
    let state = AppState::default();
    let mut p = Profile::default();
    p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
    let ran = run_line(
        &state,
        &mut p,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    assert_eq!(
        ran.result.echo,
        ["Vosh reads Health, Mana, and Moves with their maxes from this prompt. It also reads Tank and Tank health."]
    );
    assert!(ran.result.bytes.is_empty(), "Vosh never sends it");
    let codes = codes_of(&p);
    assert_eq!(codes.prompt, "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c");
    assert_eq!(codes.fprompt, "");
    assert!(codes.follow_game);
    assert_eq!(
        codes.source,
        Some(vosh_prompt::config::CaptureSource::Typed)
    );
    assert!(codes.seen_at.is_some());
    assert!(p.prompt.stage.has_recognizer());
    assert_eq!(p.prompt.config().template, "%hp", "the design stays");

    // As do_prompt stores it: prompt all, and a space added.
    let _ = run_line(&state, &mut p, "#prompt game {all}");
    assert_eq!(codes_of(&p).prompt, "%n%P%C<%hhp %mm %vmv> ");
    let _ = run_line(&state, &mut p, "#prompt game {<%hhp>}");
    assert_eq!(codes_of(&p).prompt, "<%hhp> ");
    // No space around the setting reaches the game.
    let _ = run_line(&state, &mut p, "#prompt game { <%hhp %mm> }");
    assert_eq!(codes_of(&p).prompt, "<%hhp %mm> ");
    let _ = run_line(&state, &mut p, "#prompt game { all }");
    assert_eq!(codes_of(&p).prompt, "%n%P%C<%hhp %mm %vmv> ");
}

#[test]
fn prompt_game_says_every_warning_and_refuses_what_it_cannot_read() {
    let state = AppState::default();
    let mut p = Profile::default();
    let ran = run_line(&state, &mut p, "#prompt game {<%h%m %vmv>}");
    assert_eq!(
        ran.result.echo,
        [
            "Vosh reads Moves from this prompt.",
            "Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game."
        ]
    );
    // The game keeps a typed backtick only from trust 55.
    trusted(&mut p);
    let before = p.prompt.config().clone();
    let ran = run_line(&state, &mut p, "#prompt game {<`%h>}");
    assert_eq!(
        ran.result.echo,
        ["[A color code runs into %h. Put a space between them in the game.]"]
    );
    assert_eq!(*p.prompt.config(), before);
    let ran = run_line(&state, &mut p, "#prompt game {off}");
    assert_eq!(
        ran.result.echo,
        ["[That turns prompts off in the game. Type the prompt setting you use.]"]
    );
    let ran = run_line(&state, &mut p, "#prompt game");
    assert_eq!(
        ran.result.echo,
        ["[usage #prompt game {your PROMPT setting}]"]
    );
}

/// Char.Status for an immortal with trust 55, whose typed backticks
/// the game keeps.
fn trusted(p: &mut Profile) {
    p.prompt.observe(
        "Char.Status",
        serde_json::json!({"name": "Tester", "level": 60}),
        chrono::Local::now().fixed_offset(),
    );
}

#[test]
fn prompt_game_stores_what_the_game_keeps_of_your_backticks() {
    // A mortal, or anyone before Char.Status names a level, loses
    // each backtick and the character after it.
    let state = AppState::default();
    let mut p = Profile::default();
    let _ = run_line(&state, &mut p, "#prompt game {`(240)[%h/%Hhp]}");
    assert_eq!(codes_of(&p).prompt, "240)[%h/%Hhp] ");
    trusted(&mut p);
    let _ = run_line(&state, &mut p, "#prompt game {`(240)[%h/%Hhp]}");
    assert_eq!(codes_of(&p).prompt, "`(240)[%h/%Hhp] ");
}

#[test]
fn prompt_fight_sets_the_fight_prompt_beside_your_prompt() {
    let state = AppState::default();
    let mut p = Profile::default();
    trusted(&mut p);
    let ran = run_line(&state, &mut p, "#prompt fight {`1%h``hp [%p] >}");
    assert_eq!(
        ran.result.echo,
        ["Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start."]
    );
    assert!(p.prompt.config().capture.is_none());
    let _ = run_line(&state, &mut p, "#prompt game {<%hhp>}");
    let ran = run_line(&state, &mut p, "#prompt fight {`1%h``hp [%p] >}");
    assert_eq!(
        ran.result.echo,
        ["Vosh reads Health from this fight prompt. It also reads Tank health."]
    );
    let codes = codes_of(&p);
    assert_eq!(codes.prompt, "<%hhp> ");
    assert_eq!(codes.fprompt, "`1%h``hp [%p] > ");
    let _ = run_line(&state, &mut p, "#prompt fight {off}");
    assert_eq!(codes_of(&p).fprompt, "");
    assert_eq!(codes_of(&p).prompt, "<%hhp> ");
}

#[test]
fn prompt_alone_says_how_vosh_reads_your_prompt() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-29T17:30:00-05:00").unwrap();
    let status = |p: &Profile| super::prompt::prompt_status(p, now).echo;
    let state = AppState::default();
    let mut p = Profile::default();
    assert_eq!(
        status(&p),
        ["Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start."]
    );
    p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
    let _ = run_line(
        &state,
        &mut p,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    assert_eq!(
        status(&p),
        ["Vosh reads your prompt from the codes %n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c. No prompt has matched since you connected. Drawing is on. It shows in the text."]
    );
    p.prompt.connect(true);
    let matched = chrono::DateTime::parse_from_rfc3339("2026-09-29T05:04:00-05:00").unwrap();
    p.prompt.note_prompt(matched);
    assert_eq!(
        status(&p),
        ["Vosh reads your prompt from the codes %n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c. It last matched at 5:04. Drawing is on. It shows in the text."]
    );
    // Three pulses with no prompt.
    for _ in 0..4 {
        p.prompt
            .observe("Char.Vitals", serde_json::json!({"hp": 1}), now);
    }
    assert_eq!(
        status(&p)[1],
        "No prompt has matched since 5:04. If you changed it in the game, point at it again."
    );
    let _ = run_line(&state, &mut p, r"#prompt {^<(?<hp>\d+)hp> $}");
    p.set_prompt_config(vosh_prompt::PromptConfig {
        draw: false,
        ..p.prompt.config().clone()
    });
    p.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": false, "prompt": "%h ", "fprompt": ""}),
        now,
    );
    assert_eq!(
        status(&p),
        [
            "Vosh reads your prompt with a pattern you pointed at. It last matched at 5:04. Drawing is off. It shows in the text.",
            "You turned prompts off in the game. Type prompt in the game to turn them back on."
        ]
    );
}

#[test]
fn prompt_show_picks_where_your_prompt_shows_and_the_status_says_it() {
    use vosh_prompt::PromptShow;
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-30T09:00:00-05:00").unwrap();
    let state = AppState::default();
    let mut p = Profile::default();
    // With nothing reading your prompt there is nothing to show.
    let ran = run_line(&state, &mut p, "#prompt show pinned");
    assert_eq!(
        ran.result.echo,
        ["Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start."]
    );
    assert_eq!(p.prompt.config().show, PromptShow::Text);

    p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
    let _ = run_line(
        &state,
        &mut p,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    for (line, show, echo, status) in [
        (
            "#prompt show pinned",
            PromptShow::Pinned,
            "Your latest prompt shows pinned above the command line.",
            "It shows pinned above the command line.",
        ),
        (
            "#prompt show Lifted",
            PromptShow::Lifted,
            "Each prompt shows on a raised band in the text.",
            "It shows lifted in the text.",
        ),
        (
            "#prompt show text",
            PromptShow::Text,
            "Your prompt shows in the text.",
            "It shows in the text.",
        ),
    ] {
        let ran = run_line(&state, &mut p, line);
        assert_eq!(ran.result.echo, [echo], "{line}");
        assert_eq!(p.prompt.config().show, show, "{line}");
        let said = super::prompt::prompt_status(&p, now).echo;
        assert!(
            said[0].ends_with(&format!("Drawing is on. {status}")),
            "{said:?}"
        );
    }
    // The design and the capture stay.
    assert_eq!(p.prompt.config().template, "%hp");
    assert!(p.prompt.config().capture.is_aabahran());

    for line in ["#prompt show", "#prompt show sideways"] {
        let ran = run_line(&state, &mut p, line);
        assert_eq!(
            ran.result.echo,
            ["[usage #prompt show text | lifted | pinned]"],
            "{line}"
        );
    }
    assert_eq!(p.prompt.config().show, PromptShow::Text);
    // The help names it.
    assert!(super::slash::HELP_TEXT.contains("#prompt show text|lifted|pinned"));
}

#[test]
fn prompt_default_puts_the_default_design_in_place_and_keeps_yours() {
    use vosh_prompt::{PromptShow, DEFAULT_DESIGN};
    let state = AppState::default();
    let mut p = Profile::default();
    p.set_prompt_config(vosh_prompt::PromptConfig {
        show: PromptShow::Pinned,
        ..vosh_prompt::PromptConfig::from_legacy(true, "%hp")
    });
    let _ = run_line(
        &state,
        &mut p,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    let capture = p.prompt.config().capture.clone();

    let ran = run_line(&state, &mut p, "#prompt default");
    assert_eq!(
        ran.result.echo,
        ["Your design is now Vosh's default. Vosh keeps the one you had as an earlier design."]
    );
    let leftover = &ran.result.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    let config = p.prompt.config();
    assert_eq!(config.template, DEFAULT_DESIGN);
    assert_eq!(config.previous_templates, ["%hp"]);
    // The switch, the place and the capture stay.
    assert!(config.draw);
    assert_eq!(config.show, PromptShow::Pinned);
    assert_eq!(config.capture, capture);
    // A save writes the [ui] copy from the table.
    let file = crate::profile::file::ProfileConfig::from_profile(&p);
    assert_eq!(file.ui.prompt_template, DEFAULT_DESIGN);

    let ran = run_line(&state, &mut p, "#prompt default");
    assert_eq!(ran.result.echo, ["Your design is already Vosh's default."]);
    assert_eq!(p.prompt.config().previous_templates, ["%hp"]);

    let ran = run_line(&state, &mut p, "#prompt default please");
    assert_eq!(ran.result.echo, ["[usage #prompt default]"]);
    // The help names it.
    assert!(super::slash::HELP_TEXT.contains("#prompt default "));
}

#[test]
fn prompt_draw_turns_drawing_on_and_off() {
    use vosh_prompt::DEFAULT_DESIGN;
    // No design and nothing reads the prompt yet.
    let state = AppState::default();
    let mut p = Profile::default();
    p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(false, ""));
    let ran = run_line(&state, &mut p, "#prompt draw on");
    assert_eq!(
        ran.result.echo,
        [
            "Drawing is on. Vosh draws your design in place of your prompt.",
            "Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start.",
        ]
    );
    let leftover = &ran.result.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    let config = p.prompt.config();
    assert!(config.draw);
    // Drawing with no design draws Vosh's default, as Settings does.
    assert_eq!(config.template, DEFAULT_DESIGN);
    let file = crate::profile::file::ProfileConfig::from_profile(&p);
    assert!(file.ui.prompt_template_enabled);

    let _ = run_line(
        &state,
        &mut p,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    let ran = run_line(&state, &mut p, "#prompt draw off");
    assert_eq!(
        ran.result.echo,
        ["Drawing is off. You see the game's own prompt again."]
    );
    assert!(!p.prompt.config().draw);
    assert_eq!(
        p.prompt.config().template,
        DEFAULT_DESIGN,
        "the design stays"
    );
    let ran = run_line(&state, &mut p, "#prompt draw ON");
    assert_eq!(
        ran.result.echo,
        ["Drawing is on. Vosh draws your design in place of your prompt."]
    );
    for line in ["#prompt draw", "#prompt draw maybe"] {
        let ran = run_line(&state, &mut p, line);
        assert_eq!(ran.result.echo, ["[usage #prompt draw on | off]"], "{line}");
    }
    assert!(p.prompt.config().draw);
    // The help names it.
    assert!(super::slash::HELP_TEXT.contains("#prompt draw on|off"));
}

#[test]
fn prompt_default_says_what_else_it_takes_to_see_the_design() {
    // Drawing off.
    let state = AppState::default();
    let mut p = Profile::default();
    p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(false, "%hp"));
    let _ = run_line(
        &state,
        &mut p,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    let ran = run_line(&state, &mut p, "#prompt default");
    assert_eq!(
        ran.result.echo,
        [
            "Your design is now Vosh's default. Vosh keeps the one you had as an earlier design.",
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
        ]
    );
    assert!(!p.prompt.config().draw);

    // Nothing reads your prompt yet, and there was no design to keep.
    let mut p = Profile::default();
    let ran = run_line(&state, &mut p, "#prompt default");
    assert_eq!(
        ran.result.echo,
        [
            "Your design is now Vosh's default.",
            "Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start.",
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
        ]
    );
    assert_eq!(p.prompt.config().template, vosh_prompt::DEFAULT_DESIGN);
    let leftover = &p.prompt.config().previous_templates;
    assert!(leftover.is_empty(), "{leftover:?}");

    // A fresh profile already holds the default design, and still
    // hears what else it takes.
    let mut p = Profile::default();
    p.set_prompt_config(vosh_prompt::PromptConfig::fresh());
    let ran = run_line(&state, &mut p, "#prompt default");
    assert_eq!(
        ran.result.echo,
        [
            "Your design is already Vosh's default.",
            "Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start.",
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
        ]
    );
    let _ = run_line(
        &state,
        &mut p,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    let ran = run_line(&state, &mut p, "#prompt default");
    assert_eq!(
        ran.result.echo,
        [
            "Your design is already Vosh's default.",
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
        ]
    );
    assert_eq!(p.prompt.config().template, vosh_prompt::DEFAULT_DESIGN);
    let leftover = &p.prompt.config().previous_templates;
    assert!(leftover.is_empty(), "{leftover:?}");
}

/// A table with the pattern the move from a capture trigger wrote.
fn migrated() -> vosh_prompt::PromptConfig {
    vosh_prompt::PromptConfig {
        capture: vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
            lines: vec![r"\[(?<hp>\d+)/(?<maxhp>\d+)hp\]".into()],
            source: Some(vosh_prompt::config::CaptureSource::Migrated),
            ..vosh_prompt::config::RegexCapture::default()
        }),
        ..vosh_prompt::PromptConfig::from_legacy(true, "%hp")
    }
}

#[test]
fn prompt_says_in_one_sentence_why_the_moved_pattern_stayed() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-30T09:00:00-05:00").unwrap();
    let mut p = Profile::default();
    p.set_prompt_config(migrated());
    p.prompt.connect(true);
    p.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "<`%h> ", "fprompt": ""}),
        now,
    );
    assert_eq!(*p.prompt.config(), migrated(), "the pattern stays");
    assert_eq!(
        super::prompt::prompt_status(&p, now).echo,
        [
            "Vosh reads your prompt with a pattern you pointed at. No prompt has matched since you connected. Drawing is on. It shows in the text.",
            "Vosh kept the pattern from your old capture trigger because a color code runs into %h in the prompt the game sent.",
        ]
    );
    // Once the game sends a prompt Vosh reads, the pattern switches
    // and the reason goes.
    p.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
        now,
    );
    assert_eq!(
        super::prompt::prompt_status(&p, now).echo,
        ["Vosh reads your prompt from the codes <%hhp>. No prompt has matched since you connected. Drawing is on. It shows in the text."]
    );
}

#[test]
fn a_pattern_you_set_never_switches_to_the_codes_the_game_sends() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-30T09:00:00-05:00").unwrap();
    let state = AppState::default();
    let mut p = Profile::default();
    p.set_prompt_config(migrated());
    p.prompt.connect(true);
    let _ = run_line(&state, &mut p, r"#prompt {\[(?<hp>\d+)/(?<maxhp>\d+)hp\]}");
    let typed = p.prompt.config().clone();
    assert!(!typed.capture.is_migrated());
    p.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
        now,
    );
    assert_eq!(*p.prompt.config(), typed);

    // #unprompt leaves nothing to switch.
    let mut p = Profile::default();
    p.set_prompt_config(migrated());
    p.prompt.connect(true);
    let _ = run_line(&state, &mut p, "#unprompt");
    p.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
        now,
    );
    assert!(p.prompt.config().capture.is_none());
}

#[test]
fn unprompt_stops_reading_and_keeps_the_design() {
    let state = AppState::default();
    let mut p = Profile::default();
    p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
    let ran = run_line(&state, &mut p, "#unprompt");
    assert_eq!(
        ran.result.echo,
        ["Vosh does not read your prompt in this profile."]
    );
    let _ = run_line(&state, &mut p, r"#prompt {^<(?<hp>\d+)hp> $}");
    let ran = run_line(&state, &mut p, "#unprompt");
    assert_eq!(
        ran.result.echo,
        ["Vosh stopped reading your prompt. Your design stays saved."]
    );
    assert!(p.prompt.config().capture.is_none());
    assert!(!p.prompt.stage.has_recognizer());
    assert_eq!(p.prompt.config().template, "%hp");
    assert!(p.prompt.config().draw);
}

/// Run `lines` through the pipeline the way the typed path does and
/// note each one.
fn effects_of(lines: &[&str]) -> LineEffects {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut effects = LineEffects::default();
    for line in lines {
        let ran = run_line(&state, &mut p, line);
        effects.note_ran(line, &ran);
    }
    effects
}

const DIRTY: LineEffects = LineEffects {
    replaced: false,
    dirty: true,
    tick_changed: false,
};

const REPLACED: LineEffects = LineEffects {
    replaced: true,
    dirty: false,
    tick_changed: false,
};

/// Whether `line` changed the tick settings of `p`.
fn changes_tick(p: &mut Profile, line: &str) -> bool {
    run_line(&AppState::default(), p, line).tick_changed
}

#[test]
fn a_tick_command_that_changes_a_setting_says_so() {
    let mut p = Profile::default();
    for line in [
        "#tick warn at 10",
        "#tick warn at 5",
        "#tick warn message duck",
        "#tick warn color red",
        "#tick warn off",
        "#tick interval 40",
        "#tick on {^The sun}",
        "#tick off",
        "#tick fire score",
        "#tick nofire",
        "#tick sound off",
        "#tick disable",
        "#tick enable",
    ] {
        assert!(changes_tick(&mut p, line), "{line}");
    }
}

#[test]
fn a_line_that_leaves_the_tick_settings_alone_says_nothing() {
    let state = AppState::default();
    let mut p = Profile::default();
    let _ = run_line(&state, &mut p, "#tick warn at 10");
    for line in [
        "look",
        "#tick",
        "#tick warn",
        "#tick reset",
        "#tick warn at 10",
        "#tick warn at nonsense",
        "#tick interval 0",
        "#alias greet wave",
    ] {
        assert!(!changes_tick(&mut p, line), "{line}");
    }
}

#[test]
fn the_effects_remember_a_tick_change_across_the_run() {
    let effects = effects_of(&["#tick warn at 10", "look"]);
    assert!(effects.tick_changed);
    assert!(effects.dirty);
    assert!(!effects_of(&["#tick", "look"]).tick_changed);
}

#[test]
fn slash_commands_mark_the_profile_dirty() {
    for line in [
        "#alias greet wave",
        "#trigger flee {^You flee} send look",
        "  #var x 1",
    ] {
        assert_eq!(effects_of(&[line]), DIRTY, "{line}");
    }
    assert_eq!(effects_of(&["look", "greet"]), LineEffects::default());
}

#[test]
fn profile_save_load_and_reset_wait_for_the_relaunch_after_the_wizard() {
    let state = AppState::default();
    state
        .relaunch_pending
        .store(true, std::sync::atomic::Ordering::Release);
    let mut p = Profile::default();
    p.aliases
        .set(vosh_automation::alias::Alias::new("kk", "kick %1"));
    for sub in ["save", "load", "reset"] {
        let mut replaced = false;
        let result = slash_profile(&state, &mut p, sub, &mut replaced);
        assert_eq!(
            result.echo,
            ["[Quit Vosh and open it again to finish the move to loadouts.]"],
            "{sub}"
        );
        assert!(!replaced, "{sub}");
        assert!(p.aliases.get("kk").is_some(), "{sub}");
    }
}

/// `#profile save` in the app data folder of `state`, again while
/// another test holds the persist lock the save only tries. It
/// compares against the busy echo the save itself builds, so a new
/// wording or format cannot stop the retry, and it panics when the
/// lock stays held for 10 s, so a test that leaks a guard fails
/// instead of hanging.
fn save_profile_in(state: &AppState, p: &mut Profile) -> InputResult {
    let busy = InputResult::error(PROFILE_SAVE_BUSY).echo;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let result = slash_profile(state, p, "save", &mut false);
        if result.echo != busy {
            return result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "PERSIST_LOCK held too long"
        );
        std::thread::yield_now();
    }
}

#[test]
fn profile_and_script_commands_use_the_app_data_folder() {
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let state = AppState::default();
    state.app_data.set(app_data.to_path_buf()).unwrap();
    let mut p = Profile::default();
    p.aliases
        .set(vosh_automation::alias::Alias::new("kk", "kick %1"));
    // With no index there is no active profile to save.
    let saved = save_profile_in(&state, &mut p);
    assert_eq!(saved.echo, ["[could not resolve profile path]"]);
    assert!(!app_data.join("profile.toml").exists());

    // A fresh profile. The index names it, and its first save
    // writes its file.
    std::fs::write(
        app_data.join("profiles.toml"),
        "active = \"Healer\"\n\n[[profiles]]\nname = \"Healer\"\n",
    )
    .unwrap();
    let healer = app_data.join("profiles").join("Healer.toml");
    let saved = save_profile_in(&state, &mut p);
    assert_eq!(
        saved.echo,
        [format!("profile saved to {}", healer.display())]
    );
    assert!(healer.exists());
    assert!(!app_data.join("profile.toml").exists());

    let mut fresh = Profile::default();
    let mut replaced = false;
    let loaded = slash_profile(&state, &mut fresh, "load", &mut replaced);
    assert_eq!(
        loaded.echo[0],
        format!("profile loaded from {}", healer.display())
    );
    assert!(replaced);
    assert!(fresh.aliases.get("kk").is_some());

    let script = app_data.join("scripts").join("greet.lua");
    std::fs::create_dir_all(script.parent().unwrap()).unwrap();
    std::fs::write(&script, "local greeting = 'hi'\n").unwrap();
    let loaded = slash_script(
        &state,
        &mut fresh,
        "load greet",
        &mut ApplyResult::default(),
    );
    assert_eq!(loaded.echo, [format!("loaded {}", script.display())]);
}

#[test]
fn a_reset_replaces_the_profile_and_saves_nothing() {
    for line in ["#profile reset", "#profile  reset", "# profile reset"] {
        assert_eq!(effects_of(&[line]), REPLACED, "{line}");
    }
    // An edit before the reset is gone with it. An edit after it
    // says the live state is wanted.
    assert_eq!(effects_of(&["#alias a b", "#profile reset"]), REPLACED);
    assert_eq!(
        effects_of(&["#profile reset", "#alias a b"]),
        LineEffects {
            replaced: true,
            dirty: true,
            tick_changed: false,
        }
    );
}

#[test]
fn a_load_replaces_the_profile_only_when_its_file_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Healer.toml");
    std::fs::write(&path, "tracked = = [\n").unwrap();
    let mut p = Profile::default();
    let _ = process(&mut p, "#alias greet wave");

    let mut replaced = false;
    let r = load_profile_file(&mut p, &path, &mut replaced);
    assert!(!replaced);
    assert!(r.echo[0].contains("load failed"), "{:?}", r.echo);
    assert!(p.aliases.get("greet").is_some());
    // So the alias still saves, and a reset before it stays unsaved.
    let mut effects = LineEffects::default();
    effects.note("#alias greet wave", false);
    effects.note("#profile load", replaced);
    assert_eq!(effects, DIRTY);
    let mut effects = REPLACED;
    effects.note("#profile load", replaced);
    assert_eq!(effects, REPLACED);

    ProfileConfig::default().save(&path).unwrap();
    let _ = load_profile_file(&mut p, &path, &mut replaced);
    assert!(replaced);
    assert!(p.aliases.get("greet").is_none());
    let mut effects = DIRTY;
    effects.note("#profile load", replaced);
    assert_eq!(effects, REPLACED);
}

#[test]
fn a_reset_or_load_that_echoes_saves_nothing() {
    // Loadout mode turns the pair into echoes, and an echo is no
    // change to save.
    let mut effects = LineEffects::default();
    effects.note("#profile reset", false);
    effects.note("#profile load", false);
    assert_eq!(effects, LineEffects::default());
}

#[test]
fn a_lua_alias_body_that_changes_durable_state_marks_the_profile_dirty() {
    let state = AppState::default();
    let mut p = Profile::default();
    p.aliases
        .set(Alias::new("kk", "ignored").with_script("mud.send('kick')"));
    p.aliases
        .set(Alias::new("keep", "ignored").with_script("mud.alias('greet', 'wave')"));
    let mut effects = LineEffects::default();
    for line in ["kk", "look"] {
        let ran = run_line(&state, &mut p, line);
        effects.note_ran(line, &ran);
    }
    assert_eq!(effects, LineEffects::default());
    let ran = run_line(&state, &mut p, "keep");
    effects.note_ran("keep", &ran);
    assert_eq!(effects, DIRTY);
}

#[test]
fn lua_a_line_runs_hands_on_all_it_asks_for() {
    let state = AppState::default();
    let mut p = Profile::default();
    let ran = run_line(
        &state,
        &mut p,
        "#lua mud.echo('hi') mud.send('look') mud.timer(1, function() end) \
         mud.input('#echo again') mud.set_prompt_var('mark', 'on')",
    );
    let leftover = &ran.result.echo;
    assert!(leftover.is_empty(), "{leftover:?}");
    let leftover = &ran.result.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(ran.lua.echoes, ["hi"]);
    assert_eq!(ran.lua.send_bytes, b"look\r\n");
    assert_eq!(ran.lua.new_timers.len(), 1);
    assert_eq!(ran.lua.inputs, ["#echo again"]);
    assert!(ran.lua.prompt_vars_changed);

    // A reload runs each loaded script again, timers and all.
    p.script
        .load_script("t", "mud.timer(1, function() end)".into())
        .unwrap();
    let ran = run_line(&state, &mut p, "#script reload");
    assert_eq!(ran.result.echo, ["scripts reloaded"]);
    assert_eq!(ran.lua.new_timers.len(), 1);
}

#[test]
fn scripts_lists_lua_triggers_by_name() {
    let mut p = Profile::default();
    process(
        &mut p,
        "#lua mud.trigger('zeta', 'z', function() end) \
         mud.trigger('alpha', 'a', function() end)",
    );
    let r = process(&mut p, "#scripts");
    assert_eq!(
        r.echo,
        [
            "no scripts loaded",
            "2 lua trigger(s):",
            "    [  0] alpha /a/",
            "    [  0] zeta /z/",
        ]
    );
}

#[test]
fn plain_input_appends_crlf() {
    let mut p = Profile::default();
    let r = process(&mut p, "look");
    assert_eq!(r.bytes, b"look\r\n");
    let leftover = &r.echo;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn empty_input_sends_bare_crlf() {
    let mut p = Profile::default();
    let r = process(&mut p, "");
    assert_eq!(r.bytes, b"\r\n");
    let leftover = &r.echo;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn whitespace_only_input_sends_bare_crlf() {
    let mut p = Profile::default();
    let r = process(&mut p, "   ");
    assert_eq!(r.bytes, b"\r\n");
}

#[test]
fn alias_expansion_runs_through_pipeline() {
    let mut p = Profile::default();
    p.aliases.set(Alias::new("greet", "wave;bow"));
    let r = process(&mut p, "greet");
    assert_eq!(r.bytes, b"wave\r\nbow\r\n");
}

#[test]
fn a_lua_alias_runs_its_body_in_the_order_you_typed() {
    let mut p = Profile::default();
    p.vars.set(Scope::Session, "target", "goblin");
    p.aliases.set(
        Alias::new("kk", "ignored")
            .with_script("mud.send('kick ' .. captures[1])\nmud.echo('kicked')"),
    );
    // The body runs where you typed the alias, with the words after
    // its name, so what it sends goes out between the commands
    // around it.
    let r = process(&mut p, "look;kk $target;wave");
    assert_eq!(r.bytes, b"look\r\nkick goblin\r\nwave\r\n");
    assert_eq!(r.echo, ["kicked"]);
    let r = process(&mut p, "kk dragon;wave");
    assert_eq!(r.bytes, b"kick dragon\r\nwave\r\n");
    assert_eq!(r.echo, ["kicked"]);
}

#[test]
fn a_lua_alias_body_reads_the_variables_as_they_are_now() {
    // No script is loaded and no Lua trigger is set, so nothing else
    // gives Lua the variables before the body runs.
    let mut p = Profile::default();
    p.aliases
        .set(Alias::new("kt", "ignored").with_script("mud.send('kick ' .. mud.var('target'))"));
    let _ = process(&mut p, "#var target goblin");
    assert_eq!(process(&mut p, "kt").bytes, b"kick goblin\r\n");
    let _ = process(&mut p, "#var target orc");
    assert_eq!(process(&mut p, "kt").bytes, b"kick orc\r\n");
}

#[test]
fn what_else_a_lua_alias_body_asks_for_comes_back_with_the_line() {
    let state = AppState::default();
    let mut p = Profile::default();
    p.aliases.set(Alias::new("later", "ignored").with_script(
        "mud.timer(1, function() end)\nmud.input('#echo again')\n\
         mud.set_prompt_var('mark', 'on')\nmud.send('now')",
    ));
    let ran = run_line(&state, &mut p, "later;look");
    assert_eq!(ran.result.bytes, b"now\r\nlook\r\n");
    let leftover = &ran.lua.send_bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    let leftover = &ran.lua.echoes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(ran.lua.new_timers.len(), 1);
    assert_eq!(ran.lua.inputs, ["#echo again"]);
    assert!(ran.lua.prompt_vars_changed);
    assert!(!ran.lua.durable_changed);
}

#[test]
fn variables_substitute_before_alias_expansion() {
    let mut p = Profile::default();
    p.vars.set(Scope::Session, "target", "goblin");
    p.aliases.set(Alias::new("hit", "kick %0"));
    let r = process(&mut p, "hit $target");
    assert_eq!(r.bytes, b"kick goblin\r\n");
}

#[test]
fn semicolon_in_user_input_splits_into_two_sends() {
    let mut p = Profile::default();
    let r = process(&mut p, "look;sip water");
    assert_eq!(r.bytes, b"look\r\nsip water\r\n");
}

#[test]
fn slash_alias_replaces_an_alias_in_its_group() {
    let mut p = Profile::default();
    let mut heal = Alias::new("hl", "cast heal");
    heal.group = Some("healing".into());
    p.aliases.set(heal);
    let _ = process(&mut p, "#alias hl cast 'cure light'");
    let hl = p.aliases.get("hl").unwrap();
    assert_eq!(hl.expansion, "cast 'cure light'");
    assert_eq!(hl.group.as_deref(), Some("healing"));
}

#[test]
fn slash_alias_sets_and_lists() {
    let mut p = Profile::default();
    let _ = process(&mut p, "#alias greet wave;bow");
    let r = process(&mut p, "#aliases");
    assert!(r.echo.iter().any(|l| l.contains("greet -> wave;bow")));
}

fn rc(name: &str, npc: bool) -> RoomChar {
    RoomChar {
        name: name.to_string(),
        npc,
    }
}

#[test]
fn room_chars_need_a_name_and_read_npc_in_each_form() {
    let entries = serde_json::json!([
        {"name": "Bob", "npc": false},
        {"name": "ogre", "npc": true},
        {"name": "rat", "npc": "1"},
        {"name": "Ann", "npc": "0"},
        {"name": "troll", "npc": 1},
        {"name": "Cal"},
        {"name": "", "npc": true},
        {"npc": true},
        "Dee"
    ]);
    let chars = read_room_chars(entries.as_array().unwrap());
    assert_eq!(
        chars,
        vec![
            rc("Bob", false),
            rc("ogre", true),
            rc("rat", true),
            rc("Ann", false),
            rc("troll", true),
            rc("Cal", false),
        ]
    );
}

#[test]
fn tar_by_index_sets_target_and_idx() {
    let mut p = Profile::default();
    set_room_chars(&mut p, vec![rc("Bob", false), rc("ogre", true)]);
    let r = process(&mut p, "tar 2");
    assert_eq!(p.target.name.as_deref(), Some("ogre"));
    assert_eq!(p.target.room_idx, Some(2));
    assert!(r.echo.iter().any(|l| l.contains("ogre")));
}

#[test]
fn tar_string_keeps_literal_resolves_idx_via_substring() {
    // Non-numeric `tar <string>` stores the user's literal keyword
    // (so `kill ${target}` sends `kill gris`, which the MUD's
    // keyword matcher handles), but still resolves room_idx via
    // case-insensitive substring so the `>` marker lands on the
    // matching chip.
    let mut p = Profile::default();
    set_room_chars(
        &mut p,
        vec![rc("The Baron Grisvald", true), rc("ogre", true)],
    );
    let _ = process(&mut p, "tar gris");
    assert_eq!(p.target.name.as_deref(), Some("gris"));
    assert_eq!(p.target.room_idx, Some(1));
}

#[test]
fn tar_unknown_keeps_literal_with_no_idx() {
    let mut p = Profile::default();
    set_room_chars(&mut p, vec![rc("Bob", false)]);
    let _ = process(&mut p, "tar Alice");
    assert_eq!(p.target.name.as_deref(), Some("Alice"));
    assert_eq!(p.target.room_idx, None);
}

#[test]
fn target_syncs_to_var_store_for_interpolation() {
    let mut p = Profile::default();
    set_room_chars(&mut p, vec![rc("Bob", false)]);
    let _ = process(&mut p, "tar 1");
    // `${target}` should now interpolate to "Bob".
    let r = process(&mut p, "cast 'bless' ${target}");
    assert_eq!(r.bytes, b"cast 'bless' Bob\r\n");
}

#[test]
fn tarn_cycles_forward_and_wraps() {
    let mut p = Profile::default();
    set_room_chars(&mut p, vec![rc("A", true), rc("B", true), rc("C", true)]);
    let _ = process(&mut p, "tarn");
    assert_eq!(p.target.name.as_deref(), Some("A"));
    let _ = process(&mut p, "tarn");
    assert_eq!(p.target.name.as_deref(), Some("B"));
    let _ = process(&mut p, "tarn");
    let _ = process(&mut p, "tarn");
    assert_eq!(p.target.name.as_deref(), Some("A"));
}

#[test]
fn tarclear_drops_target_and_var() {
    let mut p = Profile::default();
    set_room_chars(&mut p, vec![rc("Bob", false)]);
    let _ = process(&mut p, "tar 1");
    let _ = process(&mut p, "tarclear");
    assert!(p.target.name.is_none());
    assert!(p.vars.get("target").is_none());
}

#[test]
fn quick_key_expands_to_verb_plus_target() {
    let mut p = Profile::default();
    set_room_chars(&mut p, vec![rc("ogre", true)]);
    let _ = process(&mut p, "tar 1");
    let _ = process(&mut p, "#qkey gg kick");
    let r = process(&mut p, "gg");
    assert_eq!(r.bytes, b"kick ogre\r\n");
}

#[test]
fn a_quick_key_echoes_like_a_typed_command() {
    let mut p = Profile::default();
    set_room_chars(&mut p, vec![rc("ogre", true)]);
    let _ = process(&mut p, "tar 1");
    let _ = process(&mut p, "#qkey gg kick");
    // The caret is on by default, before the command in the
    // terminal's own color.
    assert_eq!(
        process(&mut p, "gg").echo,
        vec!["\x1b[90m\u{203a} \x1b[0mkick ogre".to_string()]
    );
    // The Sent command color wraps the command, never the caret.
    p.ui.input_echo_color = Some("#88AAff".into());
    assert_eq!(
        process(&mut p, "gg").echo,
        vec!["\x1b[90m\u{203a} \x1b[0m\x1b[38;2;136;170;255mkick ogre\x1b[0m".to_string()]
    );
    // With the caret off the command echoes bare.
    p.ui.input_echo_caret = false;
    p.ui.input_echo_color = None;
    assert_eq!(process(&mut p, "gg").echo, vec!["kick ogre".to_string()]);
}

#[test]
fn the_caret_is_the_one_the_command_line_draws() {
    let page = include_str!("../../../src/lib/maskedInput.ts");
    assert!(page.contains(r"export const ECHO_CARET = '\x1b[90m\u203a \x1b[0m';"));
    assert_eq!(ECHO_CARET, "\x1b[90m\u{203a} \x1b[0m");
}

#[test]
fn a_sent_command_color_that_does_not_read_leaves_the_command_plain() {
    let mut ui = crate::profile::ui::UiConfig {
        input_echo_caret: false,
        ..crate::profile::ui::UiConfig::default()
    };
    for color in ["", "red", "#12345", "#12g456", "rgb(1,2,3)"] {
        ui.input_echo_color = Some(color.into());
        assert_eq!(command_echo("look", &ui), "look", "{color}");
    }
    ui.input_echo_color = Some(" ff8800 ".into());
    assert_eq!(command_echo("look", &ui), "\x1b[38;2;255;136;0mlook\x1b[0m");
    assert_eq!(command_echo("", &ui), "");
}

#[test]
fn quick_key_uses_literal_keyword_not_full_name() {
    // `tar gris` keeps "gris" as the target. Quick-keys should
    // expand to `<verb> gris` so the MUD's keyword matcher
    // resolves it on its side rather than getting the full
    // descriptor "The Baron Grisvald".
    let mut p = Profile::default();
    set_room_chars(&mut p, vec![rc("The Baron Grisvald", true)]);
    let _ = process(&mut p, "tar gris");
    let _ = process(&mut p, "#qkey gg cast 'fireball'");
    let r = process(&mut p, "gg");
    assert_eq!(r.bytes, b"cast 'fireball' gris\r\n");
}

#[test]
fn quick_key_without_target_errors() {
    let mut p = Profile::default();
    let _ = process(&mut p, "#qkey gg kick");
    let r = process(&mut p, "gg");
    let leftover = &r.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(r.echo.iter().any(|l| l.contains("no target")));
}

#[test]
fn alias_cannot_shadow_quick_key() {
    let mut p = Profile::default();
    let _ = process(&mut p, "#qkey gg kick");
    let r = process(&mut p, "#alias gg cast 'fireball'");
    assert!(r.echo.iter().any(|l| l.contains("quick-key")));
    assert!(p.aliases.get("gg").is_none());
}

#[test]
fn qkey_cannot_shadow_alias() {
    // Use a name that isn't a default quick-key slot so the alias
    // can register first, then verify qkey refuses to shadow it.
    let mut p = Profile::default();
    let _ = process(&mut p, "#alias kk kick");
    let r = process(&mut p, "#qkey kk kick");
    assert!(r.echo.iter().any(|l| l.contains("alias")));
    assert!(p.target.quick_keys.iter().all(|q| q.name != "kk"));
}

#[test]
fn qkey_cannot_use_reserved_target_keyword() {
    let mut p = Profile::default();
    let r = process(&mut p, "#qkey tar foo");
    assert!(r.echo.iter().any(|l| l.contains("target keyword")));
}

#[test]
fn default_quick_keys_are_present_but_empty() {
    let p = Profile::default();
    let names: Vec<&str> = p
        .target
        .quick_keys
        .iter()
        .map(|q| q.name.as_str())
        .collect();
    assert_eq!(names, ["gg", "xx", "zz", "tt"]);
    assert!(p.target.quick_keys.iter().all(|q| q.verb.is_empty()));
}

#[test]
fn record_captures_then_saves_alias() {
    let mut p = Profile::default();
    let _ = process(&mut p, "#record buff");
    let _ = process(&mut p, "cast 'sanctuary' self");
    let _ = process(&mut p, "cast 'haste' self");
    let _ = process(&mut p, "cast 'bless' self");
    let _ = process(&mut p, "#endrec");
    let alias = p.aliases.list();
    let buff = alias
        .iter()
        .find(|a| a.name == "buff")
        .expect("alias saved");
    assert_eq!(
        buff.expansion,
        "cast 'sanctuary' self;cast 'haste' self;cast 'bless' self"
    );
}

#[test]
fn a_recording_replaces_an_alias_in_its_group() {
    let mut p = Profile::default();
    let mut buff = Alias::new("buff", "cast 'armor' self");
    buff.group = Some("buffs".into());
    p.aliases.set(buff);
    let _ = process(&mut p, "#record buff");
    let _ = process(&mut p, "cast 'haste' self");
    let _ = process(&mut p, "#endrec");
    let buff = p.aliases.get("buff").unwrap();
    assert_eq!(buff.expansion, "cast 'haste' self");
    assert_eq!(buff.group.as_deref(), Some("buffs"));
}

#[test]
fn record_skips_slash_lines() {
    let mut p = Profile::default();
    let _ = process(&mut p, "#record probe");
    let _ = process(&mut p, "look");
    // A slash command shouldn't be captured.
    let _ = process(&mut p, "#aliases");
    let _ = process(&mut p, "score");
    let _ = process(&mut p, "#endrec");
    let buff = p
        .aliases
        .list()
        .into_iter()
        .find(|a| a.name == "probe")
        .expect("alias saved");
    assert_eq!(buff.expansion, "look;score");
}

#[test]
fn record_cancel_discards() {
    let mut p = Profile::default();
    let _ = process(&mut p, "#record nope");
    let _ = process(&mut p, "kill rabbit");
    let _ = process(&mut p, "#record cancel");
    assert!(p.recording_macro.is_none());
    assert!(p.aliases.list().iter().all(|a| a.name != "nope"));
}

#[test]
fn slash_unalias_removes() {
    let mut p = Profile::default();
    let _ = process(&mut p, "#alias greet wave");
    let _ = process(&mut p, "#unalias greet");
    let r = process(&mut p, "greet");
    assert_eq!(r.bytes, b"greet\r\n");
}

#[test]
fn slash_var_set_and_show() {
    let mut p = Profile::default();
    let r = process(&mut p, "#var hp 100");
    assert!(r.echo.iter().any(|l| l == "var hp set"));
    let r = process(&mut p, "#var hp");
    assert!(r.echo.iter().any(|l| l == "hp = 100"));
}

#[test]
fn slash_help_lists_commands() {
    let mut p = Profile::default();
    let r = process(&mut p, "#help");
    assert!(r.echo.iter().any(|l| l.contains("#alias")));
    assert!(r.echo.iter().any(|l| l.contains("#var")));
}

#[test]
fn unknown_slash_returns_error_echo() {
    let mut p = Profile::default();
    let r = process(&mut p, "#nope");
    let leftover = &r.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(r.echo.iter().any(|l| l.contains("unknown slash command")));
}

#[test]
fn alias_recursion_returns_error_echo_not_panic() {
    let mut p = Profile::default();
    p.aliases.set(Alias::new("loop", "loop"));
    let r = process(&mut p, "loop");
    let leftover = &r.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(r.echo.iter().any(|l| l.contains("recursion limit")));
}

#[test]
fn slash_trigger_highlight_registers() {
    let mut p = Profile::default();
    let r = process(&mut p, "#trigger tells {tells you} highlight cyan bold");
    assert!(r.echo.iter().any(|l| l == "trigger tells set"));
    assert_eq!(p.triggers.len(), 1);
    let trig = p.triggers.get("tells").unwrap();
    match trig.actions.first() {
        Some(TriggerAction::Highlight { style }) => {
            assert_eq!(style.fg, Some(NamedColor::Cyan));
            assert!(style.bold);
        }
        _ => panic!("expected highlight action"),
    }
}

#[test]
fn slash_trigger_gag_registers() {
    let mut p = Profile::default();
    let r = process(&mut p, "#trigger spam {tingle} gag");
    assert!(r.echo.iter().any(|l| l == "trigger spam set"));
    assert!(matches!(
        p.triggers.get("spam").unwrap().actions.first(),
        Some(TriggerAction::Gag)
    ));
}

#[test]
fn slash_trigger_send_with_capture() {
    let mut p = Profile::default();
    let r = process(
        &mut p,
        r"#trigger loot {The (\w+) is DEAD} send loot $1 from corpse",
    );
    assert!(r.echo.iter().any(|l| l == "trigger loot set"));
    match p.triggers.get("loot").unwrap().actions.first() {
        Some(TriggerAction::Send { template }) => {
            assert_eq!(template, "loot $1 from corpse");
        }
        _ => panic!("expected send action"),
    }
}

#[test]
fn slash_trigger_invalid_regex_rejected() {
    let mut p = Profile::default();
    let r = process(&mut p, "#trigger bad {[unclosed} gag");
    assert!(r.echo.iter().any(|l| l.contains("rejected")));
    assert_eq!(p.triggers.len(), 0);
}

#[test]
fn slash_triggers_lists_each_first_pattern_in_its_mode() {
    use vosh_automation::trigger::{MatchMode, Trigger, TriggerPattern};
    let mut p = Profile::default();
    let _ = process(&mut p, r"#trigger hungry {^You are hungry\.$} gag");
    for (name, pattern, mode) in [
        ("thirsty", "You are thirsty.", MatchMode::Text),
        ("tells", "Tolliver tells you", MatchMode::StartsWith),
    ] {
        p.triggers
            .set(Trigger {
                patterns: vec![TriggerPattern {
                    mode,
                    ..TriggerPattern::regex(pattern)
                }],
                ..Trigger::new(name, "", TriggerAction::Gag)
            })
            .unwrap();
    }
    let r = process(&mut p, "#triggers");
    assert_eq!(
        r.echo,
        [
            "3 trigger(s) by priority:",
            r"    [  0] hungry /^You are hungry\.$/ -> gag",
            "    [  0] thirsty text \"You are thirsty.\" -> gag",
            "    [  0] tells starts with \"Tolliver tells you\" -> gag",
        ]
    );
}

#[test]
fn slash_untrigger_removes() {
    let mut p = Profile::default();
    let _ = process(&mut p, "#trigger spam {tingle} gag");
    let _ = process(&mut p, "#untrigger spam");
    assert_eq!(p.triggers.len(), 0);
}

#[test]
fn parse_braced_pattern_handles_escaped_close() {
    let (pattern, rest) = parse_braced_pattern(r"{a\}b} send hi").unwrap();
    assert_eq!(pattern, "a}b");
    assert_eq!(rest, "send hi");
}

#[test]
fn logs_forget_passwords_reads_like_the_slash_dispatcher() {
    for line in [
        "#logs forget-passwords",
        "  #logs   forget-passwords  ",
        "# logs forget-passwords",
    ] {
        assert_eq!(logs_command(line), Some(LogsCommand::Preview), "{line:?}");
    }
    for line in [
        "#logs forget-passwords now",
        "#logs  forget-passwords   now ",
    ] {
        assert_eq!(logs_command(line), Some(LogsCommand::Forget), "{line:?}");
    }
    for line in [
        "#logs",
        "#logs forget",
        "#logs forget-passwords later",
        "#logs forget-passwords now please",
        "#logs Forget-Passwords",
    ] {
        assert_eq!(logs_command(line), Some(LogsCommand::Usage), "{line:?}");
    }
    for line in [
        "#log forget-passwords",
        "logs forget-passwords",
        "#logsforget-passwords",
        "say #logs forget-passwords",
    ] {
        assert_eq!(logs_command(line), None, "{line:?}");
    }
}

#[test]
fn help_with_words_opens_help_on_them() {
    assert_eq!(help_query("#help prompt"), Some("prompt".to_string()));
    // A run of spaces between the words reads as one.
    assert_eq!(
        help_query("  #help   tick  timer "),
        Some("tick timer".to_string())
    );
    assert_eq!(
        help_query("#help prompt \t show"),
        Some("prompt show".to_string())
    );
    assert_eq!(help_query("# help map"), Some("map".to_string()));
}

#[test]
fn help_alone_still_prints_the_summary() {
    for line in ["#help", "  #help   ", "# help"] {
        assert_eq!(help_query(line), None, "{line:?}");
    }
    let mut p = Profile::default();
    let r = process(&mut p, "#help");
    assert_eq!(r.echo.first().map(String::as_str), Some("slash commands:"));
    let leftover = &r.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn other_lines_never_open_help() {
    for line in [
        "#helpme now",
        "help prompt",
        "say #help prompt",
        "#alias help x",
    ] {
        assert_eq!(help_query(line), None, "{line:?}");
    }
}

#[test]
fn the_summary_names_help_with_words() {
    assert!(HELP_TEXT
        .lines()
        .any(|l| l.trim_start().starts_with("#help <words> ")
            && l.contains("open Help on those words")));
}

#[test]
fn help_lists_logs_forget_passwords() {
    let lines: Vec<&str> = HELP_TEXT.lines().collect();
    assert!(lines
        .iter()
        .any(|l| l.trim_start().starts_with("#logs forget-passwords ")
            && l.contains("count the lines where you sent a password")));
    assert!(lines.iter().any(
        |l| l.trim_start().starts_with("#logs forget-passwords now ")
            && l.contains("blank those lines in the session log")
    ));
}

#[test]
fn logs_from_a_timer_or_script_says_where_it_runs() {
    // Typed input runs #logs before the pipeline. Only a timer, the
    // tick command, or Lua reaches it here, and those never blank a log.
    let mut p = Profile::default();
    for line in ["#logs forget-passwords now", "#logs"] {
        let r = process(&mut p, line);
        let leftover = &r.bytes;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(r.echo, vec!["[type #logs at the input bar]".to_string()]);
    }
}

/// A timer every 30 seconds that sends `command`, in `group`.
fn timer_in(id: u32, command: &str, group: Option<&str>) -> crate::profile::live::Timer {
    crate::profile::live::Timer {
        id,
        name: String::new(),
        interval_secs: 30,
        command: command.into(),
        enabled: true,
        group: group.map(Into::into),
    }
}

#[test]
fn slash_group_turns_a_timer_group_off_and_on() {
    let mut p = Profile::default();
    p.timers.push(timer_in(1, "drink water", Some("upkeep")));
    p.timers.push(timer_in(2, "save", None));
    let r = process(&mut p, "#group upkeep off");
    assert_eq!(r.echo, ["group `upkeep` disabled for timers"]);
    assert!(!p.timer_fires(&p.timers[0]));
    // A timer with no group never turns off with one.
    assert!(p.timer_fires(&p.timers[1]));
    let r = process(&mut p, "#group upkeep");
    assert_eq!(
        r.echo,
        [
            "group `upkeep`:",
            "  triggers: (none tagged)",
            "  aliases : (none tagged)",
            "  macros  : (none tagged)",
            "  timers  : off",
        ]
    );
    let r = process(&mut p, "#groups");
    assert_eq!(r.echo, ["1 group(s):", "  upkeep: timers=off"]);
    let r = process(&mut p, "#group upkeep on");
    assert_eq!(r.echo, ["group `upkeep` enabled for timers"]);
    assert!(p.timer_fires(&p.timers[0]));
    assert!(p.disabled_timer_groups.is_empty());
}

#[test]
fn slash_group_turns_every_store_that_holds_the_group() {
    let mut p = Profile::default();
    let mut kick = Alias::new("kk", "kick");
    kick.group = Some("combat".into());
    p.aliases.set(kick);
    p.timers.push(timer_in(1, "bash", Some("combat")));
    let r = process(&mut p, "#group combat off");
    assert_eq!(r.echo, ["group `combat` disabled for aliases + timers"]);
    assert!(!p.aliases.is_group_enabled("combat"));
    assert!(!p.timer_fires(&p.timers[0]));
    let r = process(&mut p, "#group nothing off");
    assert_eq!(
        r.echo,
        ["[group `nothing` not found in triggers, aliases, macros, or timers]"]
    );
    let leftover = &p.disabled_timer_groups;
    assert!(!leftover.contains("nothing"), "{leftover:?}");
}

#[test]
fn a_timer_group_turned_off_by_lua_stops_its_timers() {
    let mut p = Profile::default();
    p.timers.push(timer_in(1, "drink water", Some("upkeep")));
    let _ = process(&mut p, "#lua mud.set_group_enabled('upkeep', false)");
    assert!(!p.timer_fires(&p.timers[0]));
}
