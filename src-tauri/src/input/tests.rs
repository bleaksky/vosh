use super::profile::{load_profile_file, slash_profile, PROFILE_SAVE_BUSY};
use super::script::slash_script;
use super::slash::{parse_braced_pattern, HELP_TEXT, SLASH_COMMANDS};
use super::target::{read_room_chars, set_room_chars};
use super::*;
use crate::profile::file::ProfileConfig;
use crate::prompt::take_config;
use crate::session::connection::{Connection, RoomChar};
use vosh_automation::alias::Alias;
use vosh_automation::trigger::{NamedColor, TriggerAction};

fn regex_capture(c: &Connection) -> vosh_prompt::config::RegexCapture {
    match &c.prompt.config().capture {
        vosh_prompt::CaptureConfig::Regex(capture) => capture.clone(),
        other => panic!("a regex capture, got {other:?}"),
    }
}

#[test]
fn prompt_writes_a_regex_capture_to_the_profile() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(true, "%hp"),
    );
    let ran = run_line(
        &state,
        &mut p,
        &mut c,
        r"#prompt {\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)m\]}",
    );
    assert_eq!(
        ran.result.echo,
        ["Vosh reads hp, maxhp, and mana from your prompt with this pattern."]
    );
    let capture = regex_capture(&c);
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
    assert!(c.prompt.config().draw);
    assert_eq!(c.prompt.config().template, "%hp");
    assert!(p.triggers.get("prompt-capture").is_none());
    assert!(c.prompt.stage.has_recognizer());

    // An anchored pattern that ends in text settles.
    let ran = run_line(&state, &mut p, &mut c, r"#prompt {^<(?<hp>\d+)hp> $}");
    assert_eq!(
        ran.result.echo,
        ["Vosh reads hp from your prompt with this pattern."]
    );
    assert!(regex_capture(&c).settle);
    // A pattern with no groups only says where your prompt is.
    let ran = run_line(&state, &mut p, &mut c, "#prompt {^> $}");
    assert_eq!(
        ran.result.echo,
        ["Vosh reads your prompt with this pattern."]
    );
}

#[test]
fn prompt_with_a_bad_pattern_changes_nothing() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let _ = run_line(&state, &mut p, &mut c, r"#prompt {^<(?<hp>\d+)hp> $}");
    let before = c.prompt.config().clone();
    let ran = run_line(&state, &mut p, &mut c, r"#prompt {\[(?<hp>\d+}");
    assert!(
        ran.result.echo[0].starts_with("[Vosh cannot read that pattern."),
        "{:?}",
        ran.result.echo
    );
    assert_eq!(*c.prompt.config(), before);
    let ran = run_line(&state, &mut p, &mut c, "#prompt {");
    assert!(ran.result.echo[0].starts_with("[usage #prompt"));
}

fn codes_of(c: &Connection) -> vosh_prompt::config::AabahranCapture {
    match &c.prompt.config().capture {
        vosh_prompt::CaptureConfig::Aabahran(codes) => codes.clone(),
        other => panic!("an aabahran capture, got {other:?}"),
    }
}

#[test]
fn prompt_game_stores_the_setting_as_the_game_does_and_says_what_it_reads() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(true, "%hp"),
    );
    let ran = run_line(
        &state,
        &mut p,
        &mut c,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    assert_eq!(
        ran.result.echo,
        ["Vosh reads Health, Mana, and Moves with their maxes from this prompt. It also reads Tank and Tank health."]
    );
    assert!(ran.result.bytes.is_empty(), "Vosh never sends it");
    let codes = codes_of(&c);
    assert_eq!(codes.prompt, "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c");
    assert_eq!(codes.fprompt, "");
    assert!(codes.follow_game);
    assert_eq!(
        codes.source,
        Some(vosh_prompt::config::CaptureSource::Typed)
    );
    assert!(codes.seen_at.is_some());
    assert!(c.prompt.stage.has_recognizer());
    assert_eq!(c.prompt.config().template, "%hp", "the design stays");

    // As do_prompt stores it: prompt all, and a space added.
    let _ = run_line(&state, &mut p, &mut c, "#prompt game {all}");
    assert_eq!(codes_of(&c).prompt, "%n%P%C<%hhp %mm %vmv> ");
    let _ = run_line(&state, &mut p, &mut c, "#prompt game {<%hhp>}");
    assert_eq!(codes_of(&c).prompt, "<%hhp> ");
    // No space around the setting reaches the game.
    let _ = run_line(&state, &mut p, &mut c, "#prompt game { <%hhp %mm> }");
    assert_eq!(codes_of(&c).prompt, "<%hhp %mm> ");
    let _ = run_line(&state, &mut p, &mut c, "#prompt game { all }");
    assert_eq!(codes_of(&c).prompt, "%n%P%C<%hhp %mm %vmv> ");
}

#[test]
fn prompt_game_says_every_warning_and_refuses_what_it_cannot_read() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let ran = run_line(&state, &mut p, &mut c, "#prompt game {<%h%m %vmv>}");
    assert_eq!(
        ran.result.echo,
        [
            "Vosh reads Moves from this prompt.",
            "Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game."
        ]
    );
    // The game keeps a typed backtick only from trust 55.
    trusted(&mut c);
    let before = c.prompt.config().clone();
    let ran = run_line(&state, &mut p, &mut c, "#prompt game {<`%h>}");
    assert_eq!(
        ran.result.echo,
        ["[A color code runs into %h. Put a space between them in the game.]"]
    );
    assert_eq!(*c.prompt.config(), before);
    let ran = run_line(&state, &mut p, &mut c, "#prompt game {off}");
    assert_eq!(
        ran.result.echo,
        ["[That turns prompts off in the game. Type the prompt setting you use.]"]
    );
    let ran = run_line(&state, &mut p, &mut c, "#prompt game");
    assert_eq!(
        ran.result.echo,
        ["[usage #prompt game {your PROMPT setting}]"]
    );
}

/// Char.Status for an immortal with trust 55, whose typed backticks
/// the game keeps.
fn trusted(c: &mut Connection) {
    c.prompt.observe(
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
    let mut c = Connection::default();
    let _ = run_line(&state, &mut p, &mut c, "#prompt game {`(240)[%h/%Hhp]}");
    assert_eq!(codes_of(&c).prompt, "240)[%h/%Hhp] ");
    trusted(&mut c);
    let _ = run_line(&state, &mut p, &mut c, "#prompt game {`(240)[%h/%Hhp]}");
    assert_eq!(codes_of(&c).prompt, "`(240)[%h/%Hhp] ");
}

#[test]
fn prompt_fight_sets_the_fight_prompt_beside_your_prompt() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    trusted(&mut c);
    let ran = run_line(&state, &mut p, &mut c, "#prompt fight {`1%h``hp [%p] >}");
    assert_eq!(
        ran.result.echo,
        ["Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start."]
    );
    assert!(c.prompt.config().capture.is_none());
    let _ = run_line(&state, &mut p, &mut c, "#prompt game {<%hhp>}");
    let ran = run_line(&state, &mut p, &mut c, "#prompt fight {`1%h``hp [%p] >}");
    assert_eq!(
        ran.result.echo,
        ["Vosh reads Health from this fight prompt. It also reads Tank health."]
    );
    let codes = codes_of(&c);
    assert_eq!(codes.prompt, "<%hhp> ");
    assert_eq!(codes.fprompt, "`1%h``hp [%p] > ");
    let _ = run_line(&state, &mut p, &mut c, "#prompt fight {off}");
    assert_eq!(codes_of(&c).fprompt, "");
    assert_eq!(codes_of(&c).prompt, "<%hhp> ");
}

#[test]
fn prompt_alone_says_how_vosh_reads_your_prompt() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-29T17:30:00-05:00").unwrap();
    let status = |c: &Connection| super::prompt::prompt_status(c, now).echo;
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    assert_eq!(
        status(&c),
        ["Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start."]
    );
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(true, "%hp"),
    );
    let _ = run_line(
        &state,
        &mut p,
        &mut c,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    assert_eq!(
        status(&c),
        ["Vosh reads your prompt from the codes %n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c. No prompt has matched since you connected. Drawing is on. It shows in the text."]
    );
    c.prompt.connect(true);
    let matched = chrono::DateTime::parse_from_rfc3339("2026-09-29T05:04:00-05:00").unwrap();
    c.prompt.note_prompt(matched);
    assert_eq!(
        status(&c),
        ["Vosh reads your prompt from the codes %n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c. It last matched at 5:04. Drawing is on. It shows in the text."]
    );
    // Three pulses with no prompt.
    for _ in 0..4 {
        c.prompt
            .observe("Char.Vitals", serde_json::json!({"hp": 1}), now);
    }
    assert_eq!(
        status(&c)[1],
        "No prompt has matched since 5:04. If you changed it in the game, point at it again."
    );
    let _ = run_line(&state, &mut p, &mut c, r"#prompt {^<(?<hp>\d+)hp> $}");
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..c.prompt.config().clone()
    };
    take_config(&mut p, &mut c, config);
    c.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": false, "prompt": "%h ", "fprompt": ""}),
        now,
    );
    assert_eq!(
        status(&c),
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
    let mut c = Connection::default();
    // With nothing reading your prompt there is nothing to show.
    let ran = run_line(&state, &mut p, &mut c, "#prompt show pinned");
    assert_eq!(
        ran.result.echo,
        ["Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start."]
    );
    assert_eq!(c.prompt.config().show, PromptShow::Text);

    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(true, "%hp"),
    );
    let _ = run_line(
        &state,
        &mut p,
        &mut c,
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
        let ran = run_line(&state, &mut p, &mut c, line);
        assert_eq!(ran.result.echo, [echo], "{line}");
        assert_eq!(c.prompt.config().show, show, "{line}");
        let said = super::prompt::prompt_status(&c, now).echo;
        assert!(
            said[0].ends_with(&format!("Drawing is on. {status}")),
            "{said:?}"
        );
    }
    // The design and the capture stay.
    assert_eq!(c.prompt.config().template, "%hp");
    assert!(c.prompt.config().capture.is_aabahran());

    for line in ["#prompt show", "#prompt show sideways"] {
        let ran = run_line(&state, &mut p, &mut c, line);
        assert_eq!(
            ran.result.echo,
            ["[usage #prompt show text | lifted | pinned]"],
            "{line}"
        );
    }
    assert_eq!(c.prompt.config().show, PromptShow::Text);
    // The help names it.
    assert!(super::slash::HELP_TEXT.contains("#prompt show text|lifted|pinned"));
}

#[test]
fn prompt_default_puts_the_default_design_in_place_and_keeps_yours() {
    use vosh_prompt::{PromptShow, DEFAULT_DESIGN};
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig {
            show: PromptShow::Pinned,
            ..vosh_prompt::PromptConfig::from_legacy(true, "%hp")
        },
    );
    let _ = run_line(
        &state,
        &mut p,
        &mut c,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    let capture = c.prompt.config().capture.clone();

    let ran = run_line(&state, &mut p, &mut c, "#prompt default");
    assert_eq!(
        ran.result.echo,
        ["Your design is now Vosh's default. Vosh keeps the one you had as an earlier design."]
    );
    let leftover = &ran.result.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    let config = c.prompt.config();
    assert_eq!(config.template, DEFAULT_DESIGN);
    assert_eq!(config.previous_templates, ["%hp"]);
    // The switch, the place and the capture stay.
    assert!(config.draw);
    assert_eq!(config.show, PromptShow::Pinned);
    assert_eq!(config.capture, capture);
    // A save writes the [ui] copy from the table.
    let file = crate::profile::file::ProfileConfig::from_profile(&p);
    assert_eq!(file.ui.prompt_template, DEFAULT_DESIGN);

    let ran = run_line(&state, &mut p, &mut c, "#prompt default");
    assert_eq!(ran.result.echo, ["Your design is already Vosh's default."]);
    assert_eq!(c.prompt.config().previous_templates, ["%hp"]);

    let ran = run_line(&state, &mut p, &mut c, "#prompt default please");
    assert_eq!(ran.result.echo, ["[usage #prompt default]"]);
    // The help names it.
    assert!(super::slash::HELP_TEXT.contains("#prompt default "));
}

/// Same as the game for the PROMPT these tests type, as the game stores
/// it, for a mortal.
fn same_as_the_game() -> String {
    vosh_prompt::card::presets::game(
        vosh_prompt::testkit::mud::PROMPT,
        "",
        vosh_prompt::aabahran::Who::default(),
    )
    .expect("the codes compile")
}

#[test]
fn prompt_draw_turns_drawing_on_and_off() {
    // No design and nothing reads the prompt yet.
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(false, ""),
    );
    let ran = run_line(&state, &mut p, &mut c, "#prompt draw on");
    assert_eq!(
        ran.result.echo,
        [
            "Drawing is on. Vosh draws your design in place of your prompt.",
            "Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start.",
        ]
    );
    let leftover = &ran.result.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    let config = c.prompt.config();
    assert!(config.draw);
    // Drawing with no design follows the game, as Settings does. With
    // no codes to follow you see the game's own prompt.
    assert!(config.mirror);
    assert_eq!(config.template, "");
    assert!(!c.prompt.draws());
    let file = crate::profile::file::ProfileConfig::from_profile(&p);
    assert!(file.ui.prompt_template_enabled);

    // Once Vosh reads your codes it draws them as the game does.
    let _ = run_line(
        &state,
        &mut p,
        &mut c,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    assert_eq!(c.prompt.config().template, same_as_the_game());
    assert!(c.prompt.draws());
    let ran = run_line(&state, &mut p, &mut c, "#prompt draw off");
    assert_eq!(
        ran.result.echo,
        ["Drawing is off. You see the game's own prompt again."]
    );
    assert!(!c.prompt.config().draw);
    assert_eq!(
        c.prompt.config().template,
        same_as_the_game(),
        "the design stays"
    );
    assert!(c.prompt.config().mirror);
    let ran = run_line(&state, &mut p, &mut c, "#prompt draw ON");
    assert_eq!(
        ran.result.echo,
        ["Drawing is on. Vosh draws your design in place of your prompt."]
    );
    for line in ["#prompt draw", "#prompt draw maybe"] {
        let ran = run_line(&state, &mut p, &mut c, line);
        assert_eq!(ran.result.echo, ["[usage #prompt draw on | off]"], "{line}");
    }
    assert!(c.prompt.config().draw);
    // The help names it.
    assert!(super::slash::HELP_TEXT.contains("#prompt draw on|off"));

    // A pattern of another game gives no codes to follow, so drawing on
    // with no design still shows the game's own prompt, and says so.
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(false, ""),
    );
    let _ = run_line(&state, &mut p, &mut c, r"#prompt {^<(?<hp>\d+)hp> $}");
    let ran = run_line(&state, &mut p, &mut c, "#prompt draw on");
    assert_eq!(
        ran.result.echo,
        [
            "Drawing is on. Vosh draws your design in place of your prompt.",
            "You have no design yet, so you see the game's own prompt. Pick one in Customize prompt.",
        ]
    );
    assert!(c.prompt.config().draw);
    assert!(c.prompt.config().mirror);
    assert!(!c.prompt.draws());
    // So do codes the game sent that Vosh cannot draw, where a color
    // runs into a code.
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig {
            capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
                prompt: "<`%h> ".into(),
                ..vosh_prompt::config::AabahranCapture::default()
            }),
            ..vosh_prompt::PromptConfig::from_legacy(false, "")
        },
    );
    let ran = run_line(&state, &mut p, &mut c, "#prompt draw on");
    assert_eq!(
        ran.result.echo[1],
        "You have no design yet, so you see the game's own prompt. Pick one in Customize prompt."
    );
    assert!(!c.prompt.draws());
}

#[test]
fn prompt_default_says_what_else_it_takes_to_see_the_design() {
    // Drawing off.
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(false, "%hp"),
    );
    let _ = run_line(
        &state,
        &mut p,
        &mut c,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    let ran = run_line(&state, &mut p, &mut c, "#prompt default");
    assert_eq!(
        ran.result.echo,
        [
            "Your design is now Vosh's default. Vosh keeps the one you had as an earlier design.",
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
        ]
    );
    assert!(!c.prompt.config().draw);

    // Nothing reads your prompt yet, and there was no design to keep.
    let mut p = Profile::default();
    let mut c = Connection::default();
    let ran = run_line(&state, &mut p, &mut c, "#prompt default");
    assert_eq!(
        ran.result.echo,
        [
            "Your design is now Vosh's default.",
            "Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start.",
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
        ]
    );
    assert_eq!(c.prompt.config().template, vosh_prompt::DEFAULT_DESIGN);
    let leftover = &c.prompt.config().previous_templates;
    assert!(leftover.is_empty(), "{leftover:?}");

    // A fresh profile follows the game, so Vosh's default is a choice
    // that keeps nothing, and it still hears what else it takes.
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(&mut p, &mut c, vosh_prompt::PromptConfig::fresh());
    let _ = run_line(
        &state,
        &mut p,
        &mut c,
        "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}",
    );
    assert_eq!(c.prompt.config().template, same_as_the_game());
    let ran = run_line(&state, &mut p, &mut c, "#prompt default");
    assert_eq!(
        ran.result.echo,
        [
            "Your design is now Vosh's default.",
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
        ]
    );
    assert!(!c.prompt.config().mirror);
    // New codes leave the default you chose alone.
    let _ = run_line(
        &state,
        &mut p,
        &mut c,
        "#prompt game {%n%P%C<%hhp %mm %vmv> }",
    );
    assert_eq!(c.prompt.config().template, vosh_prompt::DEFAULT_DESIGN);
    let ran = run_line(&state, &mut p, &mut c, "#prompt default");
    assert_eq!(
        ran.result.echo,
        [
            "Your design is already Vosh's default.",
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
        ]
    );
    assert_eq!(c.prompt.config().template, vosh_prompt::DEFAULT_DESIGN);
    let leftover = &c.prompt.config().previous_templates;
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
    let mut c = Connection::default();
    take_config(&mut p, &mut c, migrated());
    c.prompt.connect(true);
    c.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "<`%h> ", "fprompt": ""}),
        now,
    );
    assert_eq!(*c.prompt.config(), migrated(), "the pattern stays");
    assert_eq!(
        super::prompt::prompt_status(&c, now).echo,
        [
            "Vosh reads your prompt with a pattern you pointed at. No prompt has matched since you connected. Drawing is on. It shows in the text.",
            "Vosh kept the pattern from your old capture trigger because a color code runs into %h in the prompt the game sent.",
        ]
    );
    // Once the game sends a prompt Vosh reads, the pattern switches
    // and the reason goes.
    c.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
        now,
    );
    assert_eq!(
        super::prompt::prompt_status(&c, now).echo,
        ["Vosh reads your prompt from the codes <%hhp>. No prompt has matched since you connected. Drawing is on. It shows in the text."]
    );
}

#[test]
fn a_pattern_you_set_never_switches_to_the_codes_the_game_sends() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-30T09:00:00-05:00").unwrap();
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(&mut p, &mut c, migrated());
    c.prompt.connect(true);
    let _ = run_line(
        &state,
        &mut p,
        &mut c,
        r"#prompt {\[(?<hp>\d+)/(?<maxhp>\d+)hp\]}",
    );
    let typed = c.prompt.config().clone();
    assert!(!typed.capture.is_migrated());
    c.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
        now,
    );
    assert_eq!(*c.prompt.config(), typed);

    // #unprompt leaves nothing to switch.
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(&mut p, &mut c, migrated());
    c.prompt.connect(true);
    let _ = run_line(&state, &mut p, &mut c, "#unprompt");
    c.prompt.observe(
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
        now,
    );
    assert!(c.prompt.config().capture.is_none());
}

#[test]
fn unprompt_stops_reading_and_keeps_the_design() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(true, "%hp"),
    );
    let ran = run_line(&state, &mut p, &mut c, "#unprompt");
    assert_eq!(
        ran.result.echo,
        ["Vosh does not read your prompt in this profile."]
    );
    let _ = run_line(&state, &mut p, &mut c, r"#prompt {^<(?<hp>\d+)hp> $}");
    let ran = run_line(&state, &mut p, &mut c, "#unprompt");
    assert_eq!(
        ran.result.echo,
        ["Vosh stopped reading your prompt. Your design stays saved."]
    );
    assert!(c.prompt.config().capture.is_none());
    assert!(!c.prompt.stage.has_recognizer());
    assert_eq!(c.prompt.config().template, "%hp");
    assert!(c.prompt.config().draw);
}

/// Run `lines` through the pipeline the way the typed path does and
/// note each one.
fn effects_of(lines: &[&str]) -> LineEffects {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let mut effects = LineEffects::default();
    for line in lines {
        let ran = run_line(&state, &mut p, &mut c, line);
        effects.note_ran(line, &ran);
    }
    effects
}

const DIRTY: LineEffects = LineEffects {
    replaced: false,
    dirty: true,
    tick_before: None,
    prompt: None,
};

const REPLACED: LineEffects = LineEffects {
    replaced: true,
    dirty: false,
    tick_before: None,
    prompt: None,
};

/// Whether `line` changed the tick settings of `p`.
fn changes_tick(p: &mut Profile, c: &mut Connection, line: &str) -> bool {
    run_line(&AppState::default(), p, c, line)
        .tick_before
        .is_some()
}

#[test]
fn a_tick_command_that_changes_a_setting_says_so() {
    let mut p = Profile::default();
    let mut c = Connection::default();
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
        assert!(changes_tick(&mut p, &mut c, line), "{line}");
    }
}

#[test]
fn a_line_that_leaves_the_tick_settings_alone_says_nothing() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let _ = run_line(&state, &mut p, &mut c, "#tick warn at 10");
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
        assert!(!changes_tick(&mut p, &mut c, line), "{line}");
    }
}

#[test]
fn the_effects_remember_a_tick_change_across_the_run() {
    let effects = effects_of(&["#tick warn at 10", "#tick warn at 5", "look"]);
    assert_eq!(effects.tick_before, Some(TickConfig::default()));
    assert!(effects.dirty);
    assert_eq!(effects_of(&["#tick", "look"]).tick_before, None);
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
        let result = slash_profile(
            &state,
            &mut p,
            &mut Connection::default(),
            sub,
            &mut replaced,
        );
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
        let result = slash_profile(state, p, &mut Connection::default(), "save", &mut false);
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
    // A profile with no name, before any profile loads, has no file to
    // save to.
    let saved = save_profile_in(&state, &mut p);
    assert_eq!(saved.echo, ["[could not resolve profile path]"]);
    assert!(!app_data.join("profile.toml").exists());

    // A fresh profile. Its name names its file, and its first save
    // writes it.
    p.name = Some("Healer".into());
    let healer = app_data.join("profiles").join("Healer.toml");
    let saved = save_profile_in(&state, &mut p);
    assert_eq!(
        saved.echo,
        [format!("profile saved to {}", healer.display())]
    );
    assert!(healer.exists());
    assert!(!app_data.join("profile.toml").exists());

    let mut fresh = Profile {
        name: Some("Healer".into()),
        ..Profile::default()
    };
    let mut replaced = false;
    let loaded = slash_profile(
        &state,
        &mut fresh,
        &mut Connection::default(),
        "load",
        &mut replaced,
    );
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
        &mut Connection::default(),
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
            tick_before: None,
            prompt: None,
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
    let r = load_profile_file(&mut p, &mut Connection::default(), &path, &mut replaced);
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
    let _ = load_profile_file(&mut p, &mut Connection::default(), &path, &mut replaced);
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
    let mut c = Connection::default();
    p.aliases
        .set(Alias::new("kk", "ignored").with_script("mud.send('kick')"));
    p.aliases
        .set(Alias::new("keep", "ignored").with_script("mud.alias('greet', 'wave')"));
    let mut effects = LineEffects::default();
    for line in ["kk", "look"] {
        let ran = run_line(&state, &mut p, &mut c, line);
        effects.note_ran(line, &ran);
    }
    assert_eq!(effects, LineEffects::default());
    let ran = run_line(&state, &mut p, &mut c, "keep");
    effects.note_ran("keep", &ran);
    assert_eq!(effects, DIRTY);
}

#[test]
fn lua_a_line_runs_hands_on_all_it_asks_for() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let ran = run_line(
        &state,
        &mut p,
        &mut c,
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
    assert_eq!(
        ran.lua.inputs,
        [(LineFrom::YourLua, "#echo again".to_string())]
    );
    assert!(ran.lua.prompt_vars_changed);
}

/// A state whose app data folder is a new temporary folder, and its
/// scripts folder.
fn state_with_scripts() -> (tempfile::TempDir, AppState, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::default();
    state.app_data.set(dir.path().to_path_buf()).unwrap();
    let scripts = dir.path().join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    (dir, state, scripts)
}

/// The text of each `[lua]` error line in `echoes`.
fn lua_errors(echoes: &[String]) -> Vec<String> {
    echoes
        .iter()
        .filter_map(|line| {
            line.strip_prefix("\x1b[90m[lua]\x1b[0m \x1b[31m")?
                .strip_suffix("\x1b[0m")
                .map(str::to_string)
        })
        .collect()
}

#[test]
fn script_reload_reads_each_file_again_in_load_order() {
    let (_dir, state, scripts) = state_with_scripts();
    let mut p = Profile::default();
    let mut c = Connection::default();
    std::fs::write(
        scripts.join("b.lua"),
        "mud.echo('b one') mud.timer(60, function() end)",
    )
    .unwrap();
    std::fs::write(scripts.join("a.lua"), "mud.echo('a one')").unwrap();
    let first = [
        run_line(&state, &mut p, &mut c, "#script load b"),
        run_line(&state, &mut p, &mut c, "#script load a"),
    ];
    assert_eq!(first[0].lua.new_timers.len(), 1);
    // You edit both, and b now has a typo.
    std::fs::write(scripts.join("b.lua"), "mud.echo('b two')\nmud.ech('x')").unwrap();
    std::fs::write(scripts.join("a.lua"), "mud.echo('a two')").unwrap();
    let ran = run_line(&state, &mut p, &mut c, "#script reload");
    assert_eq!(ran.result.echo, ["scripts reloaded"]);
    // In load order, and the error in b stops nothing after it.
    assert_eq!(
        ran.lua.echoes,
        [
            "b two",
            "\x1b[90m[lua]\x1b[0m \x1b[31mb.lua:2: attempt to call a nil value (field 'ech')\x1b[0m",
            "a two",
        ]
    );
    // b failed, so it keeps the timer it had.
    let leftover = &ran.lua.cancel_timers;
    assert!(leftover.is_empty(), "{leftover:?}");
    // A file that went away leaves its script as it was.
    std::fs::remove_file(scripts.join("a.lua")).unwrap();
    std::fs::write(
        scripts.join("b.lua"),
        "mud.echo('b three') mud.timer(60, function() end)",
    )
    .unwrap();
    let ran = run_line(&state, &mut p, &mut c, "#script reload");
    assert_eq!(
        lua_errors(&ran.lua.echoes),
        ["Vosh could not read a.lua and left it as it was."]
    );
    assert_eq!(ran.lua.echoes[0], "b three");
    // The timer the last good run of b started goes, and a new one starts.
    assert_eq!(ran.lua.cancel_timers.len(), 1);
    assert_eq!(ran.lua.new_timers.len(), 1);
    assert_eq!(c.script.loaded_script_names(), ["a.lua", "b.lua"]);
}

#[test]
fn script_reload_reads_a_plugin_again_too() {
    let (dir, state, _scripts) = state_with_scripts();
    let plugin = dir.path().join("plugins").join("meals");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("manifest.toml"), "[plugin]\nname = \"meals\"\n").unwrap();
    std::fs::write(
        plugin.join("main.lua"),
        "mud.trigger('hunger', 'You are hungry', function() mud.echo('eat') end)",
    )
    .unwrap();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let outcome = c.script.load_script(
        vosh_script::Owner::Plugin("meals".into()),
        "@meals/main.lua",
        &std::fs::read_to_string(plugin.join("main.lua")).unwrap(),
    );
    assert!(!outcome.failed);
    std::fs::write(
        plugin.join("main.lua"),
        "mud.trigger('hunger', 'You are hungry', function() mud.echo('eat now') end)",
    )
    .unwrap();
    run_line(&state, &mut p, &mut c, "#script reload");
    let fired = c.script.match_line("You are hungry.");
    let apply = crate::script::apply_actions(&mut p, &mut c, fired);
    assert_eq!(apply.echoes, ["eat now"]);
}

#[test]
fn script_reload_tries_again_a_plugin_whose_first_load_failed() {
    let (dir, state, _scripts) = state_with_scripts();
    let plugin = dir.path().join("plugins").join("meals");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("manifest.toml"), "[plugin]\nname = \"meals\"\n").unwrap();
    std::fs::write(
        plugin.join("main.lua"),
        "mud.trigger('hunger', 'You are hungry')",
    )
    .unwrap();
    let mut p = Profile::default();
    let mut c = Connection::default();
    p.plugins.enabled = vec!["meals".into()];
    let launch =
        crate::app::plugins::follow_profile_plugins(&mut p, &mut c, &dir.path().join("plugins"));
    assert_eq!(lua_errors(&launch.echoes).len(), 1, "{:?}", launch.echoes);
    // You fix the file, and a reload loads it.
    std::fs::write(
        plugin.join("main.lua"),
        "mud.trigger('hunger', 'You are hungry', function() mud.echo('eat') end)",
    )
    .unwrap();
    let ran = run_line(&state, &mut p, &mut c, "#script reload");
    assert_eq!(ran.result.echo, ["scripts reloaded"]);
    let fired = c.script.match_line("You are hungry.");
    let apply = crate::script::apply_actions(&mut p, &mut c, fired);
    assert_eq!(apply.echoes, ["eat"]);
}

#[test]
fn a_plugin_vosh_could_not_read_says_so_and_a_reload_tries_it_again() {
    let (dir, state, _scripts) = state_with_scripts();
    let plugins = dir.path().join("plugins");
    let mut p = Profile::default();
    let mut c = Connection::default();
    p.plugins.enabled = vec!["meals".into()];
    let launch = crate::app::plugins::follow_profile_plugins(&mut p, &mut c, &plugins);
    assert_eq!(
        lua_errors(&launch.echoes),
        ["Vosh could not read plugin meals and left it off."]
    );
    // You add its files, and a reload loads it.
    let plugin = plugins.join("meals");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("manifest.toml"), "[plugin]\nname = \"meals\"\n").unwrap();
    std::fs::write(
        plugin.join("main.lua"),
        "mud.trigger('hunger', 'You are hungry', function() mud.echo('eat') end)",
    )
    .unwrap();
    let ran = run_line(&state, &mut p, &mut c, "#script reload");
    let leftover = &ran.lua.echoes;
    assert!(leftover.is_empty(), "{leftover:?}");
    let fired = c.script.match_line("You are hungry.");
    let apply = crate::script::apply_actions(&mut p, &mut c, fired);
    assert_eq!(apply.echoes, ["eat"]);
}

#[test]
fn lua_you_type_reads_the_variables_with_no_other_lua_loaded() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    run_line(&state, &mut p, &mut c, "#var mark on");
    let ran = run_line(
        &state,
        &mut p,
        &mut c,
        "#lua mud.echo(tostring(mud.var('mark')))",
    );
    assert_eq!(ran.lua.echoes, ["on"]);
}

#[test]
fn lua_errors_and_print_reach_the_terminal_as_lua_lines() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let ran = run_line(&state, &mut p, &mut c, "#lua print('hp', 80)");
    assert_eq!(ran.lua.echoes, ["\x1b[90m[lua]\x1b[0m hp\t80"]);
    let ran = run_line(&state, &mut p, &mut c, "#lua mud.echo('one') error('boom')");
    assert_eq!(
        ran.lua.echoes,
        ["one", "\x1b[90m[lua]\x1b[0m \x1b[31m#lua:1: boom\x1b[0m",]
    );
    // A line of text from print with a break in it shows as two lines.
    let ran = run_line(&state, &mut p, &mut c, "#lua print('a\\nb')");
    assert_eq!(
        ran.lua.echoes,
        ["\x1b[90m[lua]\x1b[0m a", "\x1b[90m[lua]\x1b[0m b"]
    );
    let ran = run_line(&state, &mut p, &mut c, "#lua while true do end");
    assert_eq!(
        ran.lua.echoes,
        ["\x1b[90m[lua]\x1b[0m \x1b[31mVosh stopped your #lua line after 100 ms.\x1b[0m"]
    );
}

#[test]
fn a_loose_script_runs_as_its_file_and_stops_until_a_reload() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::default();
    state.app_data.set(dir.path().to_path_buf()).unwrap();
    let scripts = dir.path().join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(
        scripts.join("combat.lua"),
        "mud.trigger('hunger', 'You are hungry', function() while true do end end)\n",
    )
    .unwrap();
    let mut p = Profile::default();
    let mut c = Connection::default();
    // A bare name and the name with .lua load as one script.
    let ran = run_line(&state, &mut p, &mut c, "#script load combat");
    assert_eq!(
        ran.result.echo,
        [format!("loaded {}", scripts.join("combat.lua").display())]
    );
    run_line(&state, &mut p, &mut c, "#script load combat.lua");
    assert_eq!(c.script.loaded_script_names(), ["combat.lua"]);
    let outcome = c.script.match_line("You are hungry.");
    let apply = crate::script::apply_actions(&mut p, &mut c, outcome);
    assert_eq!(
        apply.echoes,
        ["\x1b[90m[lua]\x1b[0m \x1b[31mVosh stopped combat.lua after 100 ms. It stays off until #script reload.\x1b[0m"]
    );
    let leftover = &c.script.lua_triggers();
    assert!(leftover.is_empty(), "{leftover:?}");
    // A reload runs it again.
    run_line(&state, &mut p, &mut c, "#script reload");
    assert_eq!(c.script.lua_triggers().len(), 1);
    // A script with an error says so, and no loaded line shows.
    std::fs::write(
        scripts.join("typo.lua"),
        "mud.echo('one')\nmud.ech('two')\n",
    )
    .unwrap();
    let ran = run_line(&state, &mut p, &mut c, "#script load typo");
    let leftover = &ran.result.echo;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(
        ran.lua.echoes,
        [
            "one",
            "\x1b[90m[lua]\x1b[0m \x1b[31mtypo.lua:2: attempt to call a nil value (field 'ech')\x1b[0m",
        ]
    );
}

#[test]
fn a_script_loads_as_one_whatever_case_you_type() {
    let (_dir, state, scripts) = state_with_scripts();
    std::fs::write(
        scripts.join("Combat.lua"),
        "mud.trigger('hunger', 'You are hungry', function() mud.echo('eat') end)",
    )
    .unwrap();
    // Only a disk that ignores case opens the file for another case.
    if !scripts.join("combat.lua").exists() {
        return;
    }
    let mut p = Profile::default();
    let mut c = Connection::default();
    run_line(&state, &mut p, &mut c, "#script load Combat");
    let ran = run_line(&state, &mut p, &mut c, "#script load combat");
    assert_eq!(
        ran.result.echo,
        [format!("loaded {}", scripts.join("Combat.lua").display())]
    );
    assert_eq!(c.script.loaded_script_names(), ["Combat.lua"]);
    let fired = c.script.match_line("You are hungry.");
    let apply = crate::script::apply_actions(&mut p, &mut c, fired);
    assert_eq!(apply.echoes, ["eat"]);
}

#[test]
fn script_load_stays_inside_the_scripts_folder() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::default();
    state.app_data.set(dir.path().join("vosh")).unwrap();
    let scripts = dir.path().join("vosh").join("scripts");
    std::fs::create_dir_all(scripts.join("combat")).unwrap();
    // A file beside the app data folder, which no load may reach.
    std::fs::write(dir.path().join("outside.lua"), "mud.send('look')\n").unwrap();
    std::fs::write(
        scripts.join("combat").join("bash.lua"),
        "mud.echo('bash')\n",
    )
    .unwrap();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let outside = dir.path().join("outside.lua");
    for line in [
        "#script load ../../outside".to_string(),
        "#script load combat/../../../outside.lua".to_string(),
        format!("#script load {}", outside.display()),
    ] {
        let ran = run_line(&state, &mut p, &mut c, &line);
        assert_eq!(
            ran.result.echo,
            ["[Vosh loads scripts from your scripts folder only.]"],
            "{line}"
        );
        let leftover = &ran.lua.send_bytes;
        assert!(leftover.is_empty(), "{line}");
    }
    let leftover = &c.script.loaded_script_names();
    assert!(leftover.is_empty(), "{leftover:?}");
    // A folder inside the scripts folder is fine.
    let ran = run_line(&state, &mut p, &mut c, "#script load ./combat/bash");
    assert_eq!(ran.lua.echoes, ["bash"]);
    assert_eq!(c.script.loaded_script_names(), ["combat/bash.lua"]);
}

/// The `[lua]` line that says why Vosh did not run a line Lua asked for.
fn refused(why: &str) -> Vec<String> {
    vec![format!("\x1b[90m[lua]\x1b[0m \x1b[31m{why}\x1b[0m")]
}

#[test]
fn lua_cannot_blank_your_profile_through_mud_input() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    p.aliases
        .set(vosh_automation::alias::Alias::new("kk", "kick %1"));
    let ran = run_line(&state, &mut p, &mut c, "#lua mud.input('#profile reset')");
    assert_eq!(
        ran.lua.inputs,
        [(LineFrom::YourLua, "#profile reset".to_string())]
    );
    let ran = run_line_from(&state, &mut p, &mut c, "#profile reset", LineFrom::YourLua);
    assert_eq!(
        ran.result.echo,
        refused("Vosh runs #profile only when you type it.")
    );
    assert!(!ran.replaced);
    assert!(p.aliases.get("kk").is_some());
}

#[test]
fn mud_input_keeps_file_and_profile_commands_to_you() {
    let (_dir, state, scripts) = state_with_scripts();
    std::fs::write(scripts.join("combat.lua"), "mud.echo('loaded')").unwrap();
    let mut p = Profile::default();
    let mut c = Connection::default();
    for (line, command) in [
        ("#profile reset", "#profile"),
        ("  #profile load", "#profile"),
        ("#import-tintin combat.tt", "#import-tintin"),
        ("#script  load combat", "#script load"),
        ("#script reload", "#script reload"),
    ] {
        let ran = run_line_from(&state, &mut p, &mut c, line, LineFrom::YourLua);
        assert_eq!(
            ran.result.echo,
            refused(&format!("Vosh runs {command} only when you type it.")),
            "{line}"
        );
        let leftover = &ran.lua.echoes;
        assert!(leftover.is_empty(), "{line} {leftover:?}");
    }
    let leftover = &c.script.loaded_script_names();
    assert!(leftover.is_empty(), "{leftover:?}");
    let ran = run_line_from(&state, &mut p, &mut c, "#scripts", LineFrom::YourLua);
    assert_eq!(ran.result.echo, ["no scripts loaded"]);
    let ran = run_line_from(&state, &mut p, &mut c, "look", LineFrom::YourLua);
    assert_eq!(ran.result.bytes, b"look\r\n");
}

#[test]
fn a_script_that_asks_for_a_reload_cannot_run_one() {
    let (_dir, state, scripts) = state_with_scripts();
    std::fs::write(
        scripts.join("again.lua"),
        "mud.echo('ran') for i = 1, 50 do mud.input('#script reload') end",
    )
    .unwrap();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let ran = run_line(&state, &mut p, &mut c, "#script load again");
    assert_eq!(ran.lua.echoes, ["ran"]);
    assert_eq!(ran.lua.inputs.len(), 50);
    let (from, line) = &ran.lua.inputs[0];
    let ran = run_line_from(&state, &mut p, &mut c, line, *from);
    assert_eq!(
        ran.result.echo,
        refused("Vosh runs #script reload only when you type it.")
    );
    let leftover = &ran.lua.echoes;
    assert!(leftover.is_empty(), "the script ran again {leftover:?}");
}

#[test]
fn a_plugin_runs_no_slash_command_but_echo_through_mud_input() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let outcome = c.script.load_script(
        vosh_script::Owner::Plugin("helpers".into()),
        "@helpers/main.lua",
        "mud.input('#lua x = 1') mud.input('#alias a b') mud.input('#echo hello')",
    );
    let apply = crate::script::apply_actions(&mut p, &mut c, outcome);
    let mut echoes = Vec::new();
    for (from, line) in &apply.inputs {
        assert_eq!(*from, LineFrom::Plugin, "{line}");
        let ran = run_line_from(&state, &mut p, &mut c, line, *from);
        echoes.extend(ran.result.echo);
        echoes.extend(ran.lua.echoes);
    }
    let mut expected = refused("Vosh never runs #lua for a plugin.");
    expected.extend(refused("Vosh never runs #alias for a plugin."));
    expected.push("hello".to_string());
    assert_eq!(echoes, expected);
    // Neither your globals nor your aliases changed.
    let read = c.script.eval("mud.echo(tostring(x))", "=#lua");
    assert_eq!(read.actions, [vosh_script::Action::Echo("nil".into())]);
    assert!(p.aliases.get("a").is_none());
}

#[test]
fn a_quick_key_cannot_carry_lua_to_a_slash_command() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    p.aliases.set(Alias::new("kk", "kick %1"));
    // Lua cannot make the quick key.
    let ran = run_line_from(
        &state,
        &mut p,
        &mut c,
        "#qkey zq #profile",
        LineFrom::YourLua,
    );
    assert_eq!(
        ran.result.echo,
        refused("Vosh sets a quick key to a # command only when you type it.")
    );
    assert!(c.target.quick_keys.iter().all(|q| q.name != "zq"));
    // Nor can it run one you made, here or from a plugin.
    run_line(&state, &mut p, &mut c, "#qkey zq #profile");
    run_line_from(&state, &mut p, &mut c, "tar reset", LineFrom::YourLua);
    let ran = run_line_from(&state, &mut p, &mut c, "zq", LineFrom::YourLua);
    assert_eq!(
        ran.result.echo[1..],
        refused("Vosh runs #profile only when you type it.")
    );
    assert!(!ran.replaced);
    let ran = run_line_from(&state, &mut p, &mut c, "zq", LineFrom::Plugin);
    assert_eq!(
        ran.result.echo[1..],
        refused("Vosh never runs #profile for a plugin.")
    );
    assert!(!ran.replaced);
    assert!(p.aliases.get("kk").is_some());
    // A quick key that sends to the game is fine.
    let ran = run_line_from(&state, &mut p, &mut c, "#qkey zk kick", LineFrom::YourLua);
    assert_eq!(ran.result.echo, ["quick-key `zk` -> kick"]);
}

#[test]
fn lua_cannot_set_the_tick_command_to_a_slash_command() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let ran = run_line_from(
        &state,
        &mut p,
        &mut c,
        "#tick fire  #profile reset",
        LineFrom::YourLua,
    );
    assert_eq!(
        ran.result.echo,
        refused("Vosh sets the tick command to a # command only when you type it.")
    );
    assert_eq!(p.tick.config.auto_fire, None);
    let ran = run_line_from(
        &state,
        &mut p,
        &mut c,
        "#tick fire stand",
        LineFrom::YourLua,
    );
    assert_eq!(ran.result.echo, ["tick auto-fire set to: stand"]);
    assert_eq!(p.tick.config.auto_fire.as_deref(), Some("stand"));
}

#[test]
fn a_plugin_alias_lasts_for_the_session_and_is_never_saved() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    p.aliases.set(Alias::new("hl", "cast heal"));
    let healer = vosh_script::Owner::Plugin("healer".into());
    let outcome = c.script.load_script(
        healer.clone(),
        "@healer/main.lua",
        "mud.alias('hl', 'cast cure') mud.alias('bt', 'bash %1')",
    );
    let apply = crate::script::apply_actions(&mut p, &mut c, outcome);
    assert!(!apply.durable_changed);
    // It takes the place of your own alias of its name.
    assert_eq!(
        run_line(&state, &mut p, &mut c, "hl").result.bytes,
        b"cast cure\r\n"
    );
    // No profile file holds it, and laying one over the profile, as a
    // switch does, leaves it be.
    let config = ProfileConfig::from_profile(&p);
    let saved: Vec<(&str, &str)> = config
        .aliases
        .iter()
        .map(|a| (a.name.as_str(), a.expansion.as_str()))
        .collect();
    assert_eq!(saved, [("hl", "cast heal")]);
    config.apply_to(&mut p);
    assert_eq!(
        run_line(&state, &mut p, &mut c, "bt Orla").result.bytes,
        b"bash Orla\r\n"
    );
    assert_eq!(
        run_line(&state, &mut p, &mut c, "#aliases").result.echo,
        [
            "3 alias(es):",
            "    hl -> cast heal",
            "    bt -> bash %1 from plugin healer",
            "    hl -> cast cure from plugin healer",
        ]
    );
    // Turning the plugin off takes its aliases and gives you yours back.
    let outcome = c.script.unload(&healer);
    crate::script::apply_actions(&mut p, &mut c, outcome);
    assert_eq!(
        run_line(&state, &mut p, &mut c, "hl").result.bytes,
        b"cast heal\r\n"
    );
    assert_eq!(
        run_line(&state, &mut p, &mut c, "bt Orla").result.bytes,
        b"bt Orla\r\n"
    );
}

#[test]
fn scripts_lists_lua_triggers_by_name() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    run_line(
        &state,
        &mut p,
        &mut c,
        "#lua mud.trigger('zeta', 'z', function() end) \
         mud.trigger('alpha', 'a', function() end)",
    );
    let r = run_line(&state, &mut p, &mut c, "#scripts").result;
    assert_eq!(
        r.echo,
        [
            "no scripts loaded",
            "2 lua trigger(s):",
            "    [  0] alpha /a/ from #lua",
            "    [  0] zeta /z/ from #lua",
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
    let mut c = Connection::default();
    c.vars.set("target", "goblin");
    p.aliases.set(
        Alias::new("kk", "ignored")
            .with_script("mud.send('kick ' .. captures[1])\nmud.echo('kicked')"),
    );
    // The body runs where you typed the alias, with the words after
    // its name, so what it sends goes out between the commands
    // around it.
    let r = process_on(&mut p, &mut c, "look;kk $target;wave");
    assert_eq!(r.bytes, b"look\r\nkick goblin\r\nwave\r\n");
    assert_eq!(r.echo, ["kicked"]);
    let r = process_on(&mut p, &mut c, "kk dragon;wave");
    assert_eq!(r.bytes, b"kick dragon\r\nwave\r\n");
    assert_eq!(r.echo, ["kicked"]);
}

#[test]
fn a_lua_alias_body_reads_the_variables_as_they_are_now() {
    // No script is loaded and no Lua trigger is set, so nothing else
    // gives Lua the variables before the body runs.
    let mut p = Profile::default();
    let mut c = Connection::default();
    p.aliases
        .set(Alias::new("kt", "ignored").with_script("mud.send('kick ' .. mud.var('target'))"));
    let _ = process_on(&mut p, &mut c, "#var target goblin");
    assert_eq!(process_on(&mut p, &mut c, "kt").bytes, b"kick goblin\r\n");
    let _ = process_on(&mut p, &mut c, "#var target orc");
    assert_eq!(process_on(&mut p, &mut c, "kt").bytes, b"kick orc\r\n");
}

#[test]
fn what_else_a_lua_alias_body_asks_for_comes_back_with_the_line() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    p.aliases.set(Alias::new("later", "ignored").with_script(
        "mud.timer(1, function() end)\nmud.input('#echo again')\n\
         mud.set_prompt_var('mark', 'on')\nmud.send('now')",
    ));
    let ran = run_line(&state, &mut p, &mut c, "later;look");
    assert_eq!(ran.result.bytes, b"now\r\nlook\r\n");
    let leftover = &ran.lua.send_bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    let leftover = &ran.lua.echoes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(ran.lua.new_timers.len(), 1);
    assert_eq!(
        ran.lua.inputs,
        [(LineFrom::YourLua, "#echo again".to_string())]
    );
    assert!(ran.lua.prompt_vars_changed);
    assert!(!ran.lua.durable_changed);
}

#[test]
fn variables_substitute_before_alias_expansion() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    c.vars.set("target", "goblin");
    p.aliases.set(Alias::new("hit", "kick %0"));
    let r = process_on(&mut p, &mut c, "hit $target");
    assert_eq!(r.bytes, b"kick goblin\r\n");
}

#[test]
fn a_profile_variable_saves_while_a_session_variable_of_its_name_hides_it() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let ran = run_line(
        &state,
        &mut p,
        &mut c,
        "#lua mud.set_profile_var('home', 'Hollow')",
    );
    assert!(ran.lua.durable_changed);
    let _ = process_on(&mut p, &mut c, "#var home inn");
    assert_eq!(
        process_on(&mut p, &mut c, "recall $home").bytes,
        b"recall inn\r\n"
    );
    let saved = ProfileConfig::from_profile(&p).profile_vars;
    assert_eq!(saved.get("home").map(String::as_str), Some("Hollow"));
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

/// Run `line` as you type it, with your target and quick keys on `c`.
fn process_on(p: &mut Profile, c: &mut Connection, line: &str) -> InputResult {
    run_line(&AppState::default(), p, c, line).result
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
    let mut c = Connection::default();
    set_room_chars(&mut c, vec![rc("Bob", false), rc("ogre", true)]);
    let r = process_on(&mut p, &mut c, "tar 2");
    assert_eq!(c.target.name.as_deref(), Some("ogre"));
    assert_eq!(c.target.room_idx, Some(2));
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
    let mut c = Connection::default();
    set_room_chars(
        &mut c,
        vec![rc("The Baron Grisvald", true), rc("ogre", true)],
    );
    let _ = process_on(&mut p, &mut c, "tar gris");
    assert_eq!(c.target.name.as_deref(), Some("gris"));
    assert_eq!(c.target.room_idx, Some(1));
}

#[test]
fn tar_unknown_keeps_literal_with_no_idx() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    set_room_chars(&mut c, vec![rc("Bob", false)]);
    let _ = process_on(&mut p, &mut c, "tar Alice");
    assert_eq!(c.target.name.as_deref(), Some("Alice"));
    assert_eq!(c.target.room_idx, None);
}

#[test]
fn target_syncs_to_var_store_for_interpolation() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    set_room_chars(&mut c, vec![rc("Bob", false)]);
    let _ = process_on(&mut p, &mut c, "tar 1");
    // `${target}` should now interpolate to "Bob".
    let r = process_on(&mut p, &mut c, "cast 'bless' ${target}");
    assert_eq!(r.bytes, b"cast 'bless' Bob\r\n");
}

#[test]
fn tarn_cycles_forward_and_wraps() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    set_room_chars(&mut c, vec![rc("A", true), rc("B", true), rc("C", true)]);
    let _ = process_on(&mut p, &mut c, "tarn");
    assert_eq!(c.target.name.as_deref(), Some("A"));
    let _ = process_on(&mut p, &mut c, "tarn");
    assert_eq!(c.target.name.as_deref(), Some("B"));
    let _ = process_on(&mut p, &mut c, "tarn");
    let _ = process_on(&mut p, &mut c, "tarn");
    assert_eq!(c.target.name.as_deref(), Some("A"));
}

#[test]
fn tarclear_drops_target_and_var() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    set_room_chars(&mut c, vec![rc("Bob", false)]);
    let _ = process_on(&mut p, &mut c, "tar 1");
    let _ = process_on(&mut p, &mut c, "tarclear");
    assert!(c.target.name.is_none());
    assert!(c.vars.get("target").is_none());
}

#[test]
fn quick_key_expands_to_verb_plus_target() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    set_room_chars(&mut c, vec![rc("ogre", true)]);
    let _ = process_on(&mut p, &mut c, "tar 1");
    let _ = process_on(&mut p, &mut c, "#qkey gg kick");
    let r = process_on(&mut p, &mut c, "gg");
    assert_eq!(r.bytes, b"kick ogre\r\n");
}

#[test]
fn a_quick_key_echoes_like_a_typed_command() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    set_room_chars(&mut c, vec![rc("ogre", true)]);
    let _ = process_on(&mut p, &mut c, "tar 1");
    let _ = process_on(&mut p, &mut c, "#qkey gg kick");
    // The chevron is on by default, before the command in the
    // terminal's own color.
    assert_eq!(
        process_on(&mut p, &mut c, "gg").echo,
        vec!["\x1b[90m\u{203a} \x1b[0mkick ogre".to_string()]
    );
    // The Command color wraps the command, never the mark.
    p.ui.input_echo_color = Some("#88AAff".into());
    assert_eq!(
        process_on(&mut p, &mut c, "gg").echo,
        vec!["\x1b[90m\u{203a} \x1b[0m\x1b[38;2;136;170;255mkick ogre\x1b[0m".to_string()]
    );
    p.ui.input_echo_color = None;
    // The greater than sign, and your own text in the Mark color.
    crate::profile::ui::set_input_echo_mark(&mut p.ui, "gt".into());
    assert_eq!(
        process_on(&mut p, &mut c, "gg").echo,
        vec!["\x1b[90m> \x1b[0mkick ogre".to_string()]
    );
    crate::profile::ui::set_input_echo_mark(&mut p.ui, "own".into());
    p.ui.input_echo_mark_text = "you:".into();
    p.ui.input_echo_mark_color = Some("#c6a46a".into());
    assert_eq!(
        process_on(&mut p, &mut c, "gg").echo,
        vec!["\x1b[38;2;198;164;106myou: \x1b[0mkick ogre".to_string()]
    );
    // Dim sent commands dims the command and leaves the mark.
    p.ui.input_echo_dim = true;
    assert_eq!(
        process_on(&mut p, &mut c, "gg").echo,
        vec!["\x1b[38;2;198;164;106myou: \x1b[0m\x1b[2mkick ogre\x1b[0m".to_string()]
    );
    // With the mark off the command echoes bare.
    crate::profile::ui::set_input_echo_mark(&mut p.ui, "off".into());
    p.ui.input_echo_dim = false;
    assert_eq!(
        process_on(&mut p, &mut c, "gg").echo,
        vec!["kick ogre".to_string()]
    );
}

#[test]
fn a_typed_line_fires_a_quick_key_as_the_pipeline_reads_it() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    // A quick key with no verb set fires nothing.
    assert!(!fires_quick_key(&c, "gg"));
    let _ = process_on(&mut p, &mut c, "#qkey gg kick");
    for line in ["gg", "  gg", "gg now"] {
        assert!(fires_quick_key(&c, line), "{line}");
    }
    for line in ["#gg", "ggg", "look", "tar 1", ""] {
        assert!(!fires_quick_key(&c, line), "{line}");
    }
}

/// The cases in fixtures/input/echo-marks.json.
#[derive(serde::Deserialize)]
struct EchoMarks {
    cases: Vec<EchoMarkCase>,
}

#[derive(serde::Deserialize)]
struct EchoMarkCase {
    about: String,
    ui: crate::profile::ui::UiConfig,
    command: String,
    echo: String,
}

#[test]
fn each_mark_echoes_the_bytes_the_shared_cases_give() {
    let file: EchoMarks =
        serde_json::from_str(include_str!("../../../fixtures/input/echo-marks.json")).unwrap();
    assert!(!file.cases.is_empty());
    for case in file.cases {
        // Your own text as a file read or a Settings save keeps it.
        let mut ui = case.ui;
        ui.input_echo_mark_text =
            crate::profile::ui::coerce_input_echo_mark_text(ui.input_echo_mark_text);
        assert_eq!(
            command_echo(&case.command, &ui),
            case.echo,
            "{}",
            case.about
        );
    }
}

#[test]
fn a_command_color_that_does_not_read_leaves_the_command_plain() {
    let mut ui = crate::profile::ui::UiConfig::default();
    crate::profile::ui::set_input_echo_mark(&mut ui, "off".into());
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
    let mut c = Connection::default();
    set_room_chars(&mut c, vec![rc("The Baron Grisvald", true)]);
    let _ = process_on(&mut p, &mut c, "tar gris");
    let _ = process_on(&mut p, &mut c, "#qkey gg cast 'fireball'");
    let r = process_on(&mut p, &mut c, "gg");
    assert_eq!(r.bytes, b"cast 'fireball' gris\r\n");
}

#[test]
fn quick_key_without_target_errors() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    let _ = process_on(&mut p, &mut c, "#qkey gg kick");
    let r = process_on(&mut p, &mut c, "gg");
    let leftover = &r.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(r.echo.iter().any(|l| l.contains("no target")));
}

#[test]
fn alias_cannot_shadow_quick_key() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    let _ = process_on(&mut p, &mut c, "#qkey gg kick");
    let r = process_on(&mut p, &mut c, "#alias gg cast 'fireball'");
    assert!(r.echo.iter().any(|l| l.contains("quick-key")));
    assert!(p.aliases.get("gg").is_none());
}

#[test]
fn qkey_cannot_shadow_alias() {
    // Use a name that isn't a default quick-key slot so the alias
    // can register first, then verify qkey refuses to shadow it.
    let mut p = Profile::default();
    let mut c = Connection::default();
    let _ = process_on(&mut p, &mut c, "#alias kk kick");
    let r = process_on(&mut p, &mut c, "#qkey kk kick");
    assert!(r.echo.iter().any(|l| l.contains("alias")));
    assert!(c.target.quick_keys.iter().all(|q| q.name != "kk"));
}

#[test]
fn qkey_cannot_use_reserved_target_keyword() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    let r = process_on(&mut p, &mut c, "#qkey tar foo");
    assert!(r.echo.iter().any(|l| l.contains("target keyword")));
}

#[test]
fn default_quick_keys_are_present_but_empty() {
    let c = Connection::default();
    let names: Vec<&str> = c
        .target
        .quick_keys
        .iter()
        .map(|q| q.name.as_str())
        .collect();
    assert_eq!(names, ["gg", "xx", "zz", "tt"]);
    assert!(c.target.quick_keys.iter().all(|q| q.verb.is_empty()));
}

#[test]
fn record_captures_then_saves_alias() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let _ = run_line(&state, &mut p, &mut c, "#record buff");
    let _ = run_line(&state, &mut p, &mut c, "cast 'sanctuary' self");
    let _ = run_line(&state, &mut p, &mut c, "cast 'haste' self");
    let _ = run_line(&state, &mut p, &mut c, "cast 'bless' self");
    let _ = run_line(&state, &mut p, &mut c, "#endrec");
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
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let mut buff = Alias::new("buff", "cast 'armor' self");
    buff.group = Some("buffs".into());
    p.aliases.set(buff);
    let _ = run_line(&state, &mut p, &mut c, "#record buff");
    let _ = run_line(&state, &mut p, &mut c, "cast 'haste' self");
    let _ = run_line(&state, &mut p, &mut c, "#endrec");
    let buff = p.aliases.get("buff").unwrap();
    assert_eq!(buff.expansion, "cast 'haste' self");
    assert_eq!(buff.group.as_deref(), Some("buffs"));
}

#[test]
fn record_skips_slash_lines() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let _ = run_line(&state, &mut p, &mut c, "#record probe");
    let _ = run_line(&state, &mut p, &mut c, "look");
    // A slash command shouldn't be captured.
    let _ = run_line(&state, &mut p, &mut c, "#aliases");
    let _ = run_line(&state, &mut p, &mut c, "score");
    let _ = run_line(&state, &mut p, &mut c, "#endrec");
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
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let _ = run_line(&state, &mut p, &mut c, "#record nope");
    let _ = run_line(&state, &mut p, &mut c, "kill rabbit");
    let _ = run_line(&state, &mut p, &mut c, "#record cancel");
    assert!(c.recording_macro.is_none());
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

/// A profile with `ds` in the groups Maren and Tolliver.
fn ds_in_two_groups() -> Profile {
    let mut p = Profile::default();
    for (expansion, group) in [("look", "Maren"), ("ponder", "Tolliver")] {
        p.aliases.set(Alias {
            group: Some(group.into()),
            ..Alias::new("ds", expansion)
        });
    }
    p
}

#[test]
fn unalias_asks_which_group_when_two_hold_the_name() {
    let mut p = ds_in_two_groups();
    let r = process(&mut p, "#unalias ds");
    assert!(
        r.echo.iter().any(|l| l.contains(
            "ds is in Maren and Tolliver, so name the group too, like #unalias ds Maren"
        )),
        "{:?}",
        r.echo
    );
    assert_eq!(p.aliases.named("ds").len(), 2);
    let r = process(&mut p, "#unalias ds Orla");
    assert!(
        r.echo
            .iter()
            .any(|l| l.contains("alias ds in Orla not found")),
        "{:?}",
        r.echo
    );
    let r = process(&mut p, "#unalias ds Maren");
    assert!(
        r.echo.iter().any(|l| l == "alias ds in Maren removed"),
        "{:?}",
        r.echo
    );
    assert_eq!(process(&mut p, "ds").bytes, b"ponder\r\n");
    // With one left, the name alone is enough.
    let r = process(&mut p, "#unalias ds");
    assert!(
        r.echo.iter().any(|l| l == "alias ds in Tolliver removed"),
        "{:?}",
        r.echo
    );
    let leftover = p.aliases.named("ds");
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn alias_again_replaces_the_one_that_fires_in_its_group() {
    let mut p = ds_in_two_groups();
    p.aliases.set_group_enabled("Maren", false);
    let r = process(&mut p, "#alias ds glance");
    assert!(
        r.echo.iter().any(|l| l == "alias ds in Tolliver set"),
        "{:?}",
        r.echo
    );
    assert_eq!(
        p.aliases.get_in(Some("Tolliver"), "ds").unwrap().expansion,
        "glance"
    );
    assert_eq!(
        p.aliases.get_in(Some("Maren"), "ds").unwrap().expansion,
        "look"
    );
    let r = process(&mut p, "#aliases");
    assert!(
        r.echo.iter().any(|l| l.contains("ds in Maren -> look")),
        "{:?}",
        r.echo
    );
}

#[test]
fn slash_var_set_and_show() {
    let mut p = Profile::default();
    let mut c = Connection::default();
    let r = process_on(&mut p, &mut c, "#var hp 100");
    assert!(r.echo.iter().any(|l| l == "var hp set"));
    let r = process_on(&mut p, &mut c, "#var hp");
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
fn slash_commands_list_every_name_the_dispatcher_runs() {
    for name in SLASH_COMMANDS {
        let mut p = Profile::default();
        let r = process(&mut p, &format!("#{name}"));
        assert!(
            !r.echo.iter().any(|l| l.contains("unknown slash command")),
            "#{name}: {:?}",
            r.echo
        );
    }
}

#[test]
fn slash_a_word_outside_the_list_is_unknown() {
    assert!(!SLASH_COMMANDS.contains(&"walkies"));
    let mut p = Profile::default();
    let r = process(&mut p, "#walkies");
    assert!(r.echo.iter().any(|l| l.contains("unknown slash command")));
}

#[test]
fn slash_commands_list_every_arm_of_the_dispatcher() {
    // Each `"name" =>` arm of the match, plus `#walk`, which runs first.
    let source = include_str!("slash.rs");
    let start = source.find("match cmd {").expect("the dispatcher match");
    let end = start + source[start..].find("other =>").expect("the unknown arm");
    let mut arms: Vec<&str> = vec!["walk"];
    for line in source[start..end].lines() {
        let Some((names, _)) = line.trim().split_once("=>") else {
            continue;
        };
        for name in names.split('|') {
            if let Some(name) = name
                .trim()
                .strip_prefix('"')
                .and_then(|n| n.strip_suffix('"'))
            {
                if !name.is_empty() {
                    arms.push(name);
                }
            }
        }
    }
    let mut listed = SLASH_COMMANDS.to_vec();
    arms.sort_unstable();
    listed.sort_unstable();
    assert_eq!(arms, listed);
}

#[test]
fn alias_recursion_returns_error_echo_not_panic() {
    let mut p = Profile::default();
    p.aliases.set(Alias::new("loop", "loop"));
    let r = process(&mut p, "loop");
    let leftover = &r.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(
        r.echo
            .iter()
            .any(|l| l.contains("alias loop calls itself, so Vosh stopped it after 16 steps")),
        "{:?}",
        r.echo
    );
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
fn the_summary_says_a_reload_reads_the_files_again() {
    assert!(HELP_TEXT
        .lines()
        .any(|l| l.trim_start().starts_with("#script reload ")
            && l.contains("read every loaded script again and run it")));
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
