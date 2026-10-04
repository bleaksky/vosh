//! What a line from a timer, a tick, an alias script or a trigger body
//! asks of the session and your profile.

use crate::app::state::AppState;
use crate::input::LineEffects;
use crate::profile::live::Profile;

#[test]
fn a_timer_command_that_edits_the_profile_marks_it_dirty() {
    let state = AppState::default();
    let mut p = Profile::default();
    let run = super::run_fired_locked(&state, &mut p, "#alias greet wave", None);
    assert!(run.effects.dirty);
    assert!(run.apply.lists.aliases);
    assert!(p.aliases.get("greet").is_some());

    let run = super::run_fired_locked(&state, &mut p, "#trigger flee {^You flee} send look", None);
    assert!(run.effects.dirty);
    assert!(run.apply.lists.triggers);

    // A plain command leaves the saved profile alone.
    let run = super::run_fired_locked(&state, &mut p, "greet", None);
    assert_eq!(run.effects, LineEffects::default());
    assert_eq!(run.apply.send_bytes, b"wave\r\n");
}

#[test]
fn a_script_alias_body_hands_on_all_it_asks_for() {
    let state = AppState::default();
    let mut p = Profile::default();
    p.aliases.set(
        vosh_automation::alias::Alias::new("kk", "ignored").with_script(
            "mud.echo('ready') mud.send(captures[1]) mud.timer(1, function() end) \
         mud.input('look') mud.set_prompt_var('mark', 'on')",
        ),
    );
    let ran = crate::input::run_line(&state, &mut p, "stand;kk orc");
    let apply = super::line_script_result(ran);
    // What the body sends goes out where you typed the alias, and all
    // else it asks for comes with the line.
    assert_eq!(apply.send_bytes, b"stand\r\norc\r\n");
    assert_eq!(apply.echoes, ["ready"]);
    assert_eq!(apply.new_timers.len(), 1);
    assert_eq!(apply.inputs, ["look"]);
    assert!(apply.prompt_vars_changed);
}

#[test]
fn a_timer_command_runs_the_body_of_a_lua_alias() {
    let state = AppState::default();
    let mut p = Profile::default();
    p.aliases.set(
        vosh_automation::alias::Alias::new("kk", "ignored")
            .with_script("mud.send('kick ' .. captures[1])\nmud.echo('kicked')"),
    );
    let run = super::run_fired_locked(&state, &mut p, "kk dragon", None);
    assert_eq!(run.apply.send_bytes, b"kick dragon\r\n");
    assert_eq!(run.apply.echoes, ["kicked"]);
    assert_eq!(run.effects, LineEffects::default());
    // What the body sends goes out where the command names the alias.
    let run = super::run_fired_locked(&state, &mut p, "kk dragon;wave", None);
    assert_eq!(run.apply.send_bytes, b"kick dragon\r\nwave\r\n");
    let run = super::run_fired_locked(&state, &mut p, "wave;kk dragon;bow", None);
    assert_eq!(run.apply.send_bytes, b"wave\r\nkick dragon\r\nbow\r\n");
}

#[test]
fn a_trigger_body_reads_the_whole_match_then_each_group() {
    // Unlike an alias body, captures[1] is the whole match, so a
    // trigger that reads hp from captures[2] keeps reading it.
    let mut p = Profile::default();
    p.triggers
        .set(vosh_automation::trigger::Trigger::new(
            "says",
            r"^(\w+) says (\w+)",
            vosh_automation::trigger::TriggerAction::Script {
                body: "mud.send(captures[1] .. '|' .. captures[2] .. '|' .. captures[3])".into(),
            },
        ))
        .expect("the trigger compiles");
    let result = vosh_automation::trigger::process(&p.triggers, b"Bob says hi");
    assert_eq!(
        super::run_trigger_scripts(&mut p, &result).actions,
        [vosh_script::Action::Send("Bob says hi|Bob|hi".into())]
    );
}

#[test]
fn a_timer_group_line_reports_a_macro_group_that_turned() {
    // The command line keeps its own map of the macro keys that fire,
    // so a timer or tick `#group` line that turns a macro group off
    // has to reach it, or the keys go on firing.
    let state = AppState::default();
    let mut p = Profile::default();
    p.macros.push(crate::profile::live::Macro {
        key: "F1".into(),
        command: "kick".into(),
        group: Some("combat".into()),
        enabled: true,
    });
    let run = super::run_fired_locked(&state, &mut p, "#group combat off", None);
    assert!(run.apply.lists.macro_groups);
    assert!(p.disabled_macro_groups.contains("combat"));
    // Off already, so nothing turned.
    let run = super::run_fired_locked(&state, &mut p, "#group combat off", None);
    assert!(!run.apply.lists.macro_groups);
}

#[test]
fn tick_and_lua_lines_note_what_they_ask_of_the_profile() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut effects = LineEffects::default();
    let _ = super::run_and_note_line(&state, &mut p, "#alias greet wave", &mut effects, None);
    assert!(effects.dirty);
    assert!(p.aliases.get("greet").is_some());

    // A reset from a timer or a script keeps the blanked profile off
    // the disk, as it does when you type it.
    let _ = super::run_and_note_line(&state, &mut p, "#profile reset", &mut effects, None);
    assert_eq!(
        effects,
        LineEffects {
            replaced: true,
            dirty: false,
            tick_changed: false,
        }
    );
    assert!(p.aliases.get("greet").is_none());
}

#[test]
fn a_reset_from_a_timer_turns_away_a_config_save_read_before_it() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut effects = LineEffects::default();
    let before = state.ui_config_generation();
    let _ = super::run_and_note_line(&state, &mut p, "#profile reset", &mut effects, None);
    assert!(state.ui_config_generation() > before);
}

#[test]
fn a_tick_command_from_a_timer_notes_the_tick_change() {
    let state = AppState::default();
    let mut p = Profile::default();
    let run = super::run_fired_locked(&state, &mut p, "#tick warn at 10", None);
    assert!(run.effects.tick_changed);
    assert_eq!(p.tick.config.warn_at_secs, Some(10));
    let run = super::run_fired_locked(&state, &mut p, "#tick", None);
    assert!(!run.effects.tick_changed);
}

#[test]
fn a_timer_command_says_what_it_changed_outside_the_text() {
    let state = AppState::default();
    let mut p = Profile::default();
    // A plain command changes neither, so nothing goes out.
    let run = super::run_fired_locked(&state, &mut p, "look", None);
    assert_eq!(run.shown, super::ShownChanges::default());

    let run = super::run_fired_locked(&state, &mut p, "tar goblin", None);
    let target = run.shown.target.expect("the new target");
    assert_eq!(target.name.as_deref(), Some("goblin"));
    assert!(!run.shown.repaint);
    // The same target again changes nothing.
    let run = super::run_fired_locked(&state, &mut p, "tar goblin", None);
    assert_eq!(run.shown, super::ShownChanges::default());

    let run = super::run_fired_locked(&state, &mut p, "#prompt default", None);
    assert!(run.shown.repaint);
    assert!(run.shown.target.is_none());
}

#[test]
fn a_timer_reset_keeps_the_shared_settings() {
    let state = AppState::default();
    // No global.toml yet, so the live shared values are the ones to
    // keep, as they are for a reset you type.
    let dir = tempfile::tempdir().unwrap();
    let layer = crate::profile::shared::SharedLayer::read(
        &dir.path().join("global.toml"),
        crate::profile::shared::ScopeConfig::default(),
    );
    let mut p = Profile::default();
    p.ui.theme = "night-ink".into();
    p.ui.font_family = "Iosevka".into();
    p.ui.keep_last_command = true;
    let _ = super::run_fired_locked(&state, &mut p, "#alias greet wave", None);

    let run = super::run_fired_locked(&state, &mut p, "#profile reset", Some(&layer));

    assert!(run.effects.replaced);
    assert!(p.aliases.get("greet").is_none());
    assert_eq!(p.ui.theme, "night-ink");
    assert_eq!(p.ui.font_family, "Iosevka");
    assert!(p.ui.keep_last_command);
}

/// The terminal line for a Lua error or stop, as the apply prints it.
fn lua_error(text: &str) -> String {
    format!("\x1b[90m[lua]\x1b[0m \x1b[31m{text}\x1b[0m")
}

#[test]
fn a_trigger_body_that_runs_away_turns_its_trigger_off() {
    let mut p = Profile::default();
    p.triggers
        .set(vosh_automation::trigger::Trigger::new(
            "hunger",
            "^You are hungry",
            vosh_automation::trigger::TriggerAction::Script {
                body: "mud.send('eat') while true do end".into(),
            },
        ))
        .expect("the trigger compiles");
    let result = vosh_automation::trigger::process(&p.triggers, b"You are hungry.");
    let outcome = super::run_trigger_scripts(&mut p, &result);
    assert!(p.triggers.is_stopped("hunger"));
    let apply = crate::script::apply_actions(&mut p, outcome);
    // A stopped body sends nothing it queued.
    let leftover = &apply.send_bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(
        apply.echoes,
        [lua_error(
            "Vosh stopped the Lua in trigger hunger after 100 ms. hunger stays off until you save it or restart Vosh."
        )]
    );
    // It matches nothing until you save it again.
    let result = vosh_automation::trigger::process(&p.triggers, b"You are hungry.");
    let leftover = &result.scripts;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn a_function_a_trigger_body_left_behind_turns_its_trigger_off_too() {
    let mut p = Profile::default();
    p.triggers
        .set(vosh_automation::trigger::Trigger::new(
            "day",
            "^The day has begun",
            vosh_automation::trigger::TriggerAction::Script {
                body: "mud.on_gmcp('World.Time', function() while true do end end)".into(),
            },
        ))
        .expect("the trigger compiles");
    let result = vosh_automation::trigger::process(&p.triggers, b"The day has begun.");
    let outcome = super::run_trigger_scripts(&mut p, &result);
    crate::script::apply_actions(&mut p, outcome);
    let msg = vosh_protocol::gmcp::Message {
        package: "World.Time".into(),
        data: serde_json::json!({}),
    };
    let (_, apply) = super::gmcp_step(&mut p, &msg, tokio::time::Instant::now());
    assert!(p.triggers.is_stopped("day"));
    assert_eq!(
        apply.echoes,
        [lua_error(
            "Vosh stopped the Lua in trigger day after 100 ms. day stays off until you save it or restart Vosh."
        )]
    );
}

#[test]
fn an_alias_body_that_runs_away_turns_its_alias_off() {
    let state = AppState::default();
    let mut p = Profile::default();
    p.aliases.set(
        vosh_automation::alias::Alias::new("heal", "ignored")
            .with_script("mud.send('cast heal') while true do end"),
    );
    // The second heal of the line runs nothing once the first stopped.
    let ran = crate::input::run_line(&state, &mut p, "heal;heal");
    let apply = super::line_script_result(ran);
    let leftover = &apply.send_bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(
        apply.echoes,
        [lua_error(
            "Vosh stopped the Lua in alias heal after 100 ms. heal stays off until you save it or restart Vosh."
        )]
    );
    assert!(p.aliases.is_stopped("heal"));
    // Typed again, it passes through, as an alias you turned off does.
    let ran = crate::input::run_line(&state, &mut p, "heal");
    assert_eq!(super::line_script_result(ran).send_bytes, b"heal\r\n");
}
