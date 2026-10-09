//! What a line from a timer, a tick, an alias script or a trigger body
//! asks of the session and your profile.

use crate::app::state::AppState;
use crate::input::{LineEffects, LineFrom};
use crate::profile::live::Profile;
use crate::session::connection::Connection;

#[test]
fn a_timer_command_that_edits_the_profile_marks_it_dirty() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let run = super::run_fired_locked(&state, &mut p, &mut c, "#alias greet wave", None);
    assert!(run.effects.dirty);
    assert!(run.apply.lists.aliases);
    assert!(p.aliases.get("greet").is_some());

    let run = super::run_fired_locked(
        &state,
        &mut p,
        &mut c,
        "#trigger flee {^You flee} send look",
        None,
    );
    assert!(run.effects.dirty);
    assert!(run.apply.lists.triggers);

    // A plain command leaves the saved profile alone.
    let run = super::run_fired_locked(&state, &mut p, &mut c, "greet", None);
    assert_eq!(run.effects, LineEffects::default());
    assert_eq!(run.apply.send_bytes, b"wave\r\n");
}

#[test]
fn a_script_alias_body_hands_on_all_it_asks_for() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    p.aliases.set(
        vosh_automation::alias::Alias::new("kk", "ignored").with_script(
            "mud.echo('ready') mud.send(captures[1]) mud.timer(1, function() end) \
         mud.input('look') mud.set_prompt_var('mark', 'on')",
        ),
    );
    let ran = crate::input::run_line(&state, &mut p, &mut c, "stand;kk orc");
    let apply = super::line_script_result(ran);
    // What the body sends goes out where you typed the alias, and all
    // else it asks for comes with the line.
    assert_eq!(apply.send_bytes, b"stand\r\norc\r\n");
    assert_eq!(apply.echoes, ["ready"]);
    assert_eq!(apply.new_timers.len(), 1);
    assert_eq!(apply.inputs, [(LineFrom::YourLua, "look".to_string())]);
    assert!(apply.prompt_vars_changed);
}

#[test]
fn a_timer_command_runs_the_body_of_a_lua_alias() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    p.aliases.set(
        vosh_automation::alias::Alias::new("kk", "ignored")
            .with_script("mud.send('kick ' .. captures[1])\nmud.echo('kicked')"),
    );
    let run = super::run_fired_locked(&state, &mut p, &mut c, "kk dragon", None);
    assert_eq!(run.apply.send_bytes, b"kick dragon\r\n");
    assert_eq!(run.apply.echoes, ["kicked"]);
    assert_eq!(run.effects, LineEffects::default());
    // What the body sends goes out where the command names the alias.
    let run = super::run_fired_locked(&state, &mut p, &mut c, "kk dragon;wave", None);
    assert_eq!(run.apply.send_bytes, b"kick dragon\r\nwave\r\n");
    let run = super::run_fired_locked(&state, &mut p, &mut c, "wave;kk dragon;bow", None);
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
    let mut c = Connection::default();
    let result = vosh_automation::trigger::process(&p.triggers, b"Bob says hi", c.stop_key);
    assert_eq!(
        super::run_trigger_scripts(&mut p, &mut c, &result).actions,
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
    let mut c = Connection::default();
    p.macros.push(crate::profile::live::Macro {
        key: "F1".into(),
        command: "kick".into(),
        group: Some("combat".into()),
        enabled: true,
        preset: None,
    });
    let run = super::run_fired_locked(&state, &mut p, &mut c, "#group combat off", None);
    assert!(run.apply.lists.macro_groups);
    assert!(p.disabled_macro_groups.contains("combat"));
    // Off already, so nothing turned.
    let run = super::run_fired_locked(&state, &mut p, &mut c, "#group combat off", None);
    assert!(!run.apply.lists.macro_groups);
}

#[test]
fn tick_and_lua_lines_note_what_they_ask_of_the_profile() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let mut effects = LineEffects::default();
    let _ = super::run_and_note_line(
        &state,
        &mut p,
        &mut c,
        LineFrom::You,
        "#alias greet wave",
        &mut effects,
        None,
    );
    assert!(effects.dirty);
    assert!(p.aliases.get("greet").is_some());

    // A reset from a timer or a script keeps the blanked profile off
    // the disk, as it does when you type it.
    let _ = super::run_and_note_line(
        &state,
        &mut p,
        &mut c,
        LineFrom::You,
        "#profile reset",
        &mut effects,
        None,
    );
    assert_eq!(
        effects,
        LineEffects {
            replaced: true,
            dirty: false,
            tick_before: None,
            prompt: None,
        }
    );
    assert!(p.aliases.get("greet").is_none());
}

#[test]
fn a_reset_from_a_timer_turns_away_a_pane_layout_write_from_before_it() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let mut effects = LineEffects::default();
    // pane_layout_set refuses a tree edited at any generation but the
    // current one.
    let before = state.panes_generation();
    let _ = super::run_and_note_line(
        &state,
        &mut p,
        &mut c,
        LineFrom::You,
        "#profile reset",
        &mut effects,
        None,
    );
    assert_ne!(state.panes_generation(), before);
}

#[test]
fn a_tick_command_from_a_timer_notes_the_tick_change() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let run = super::run_fired_locked(&state, &mut p, &mut c, "#tick warn at 10", None);
    assert!(run.effects.tick_before.is_some());
    assert_eq!(p.tick.config.warn_at_secs, Some(10));
    let run = super::run_fired_locked(&state, &mut p, &mut c, "#tick", None);
    assert!(run.effects.tick_before.is_none());
}

#[test]
fn a_prompt_command_notes_the_table_it_left_and_a_reset_notes_none() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    let run = super::run_fired_locked(&state, &mut p, &mut c, "#prompt default", None);
    assert_eq!(run.effects.prompt.as_ref(), Some(c.prompt.config()));
    let run = super::run_fired_locked(&state, &mut p, &mut c, "#prompt", None);
    assert_eq!(run.effects.prompt, None);
    // A reset hands its connection a whole table, which the other
    // sessions on the profile do not take as a choice.
    let run = super::run_fired_locked(&state, &mut p, &mut c, "#profile reset", None);
    assert!(run.effects.replaced);
    assert_eq!(run.effects.prompt, None);
}

#[test]
fn a_timer_command_says_what_it_changed_outside_the_text() {
    let state = AppState::default();
    let mut p = Profile::default();
    let mut c = Connection::default();
    // A plain command changes neither, so nothing goes out.
    let run = super::run_fired_locked(&state, &mut p, &mut c, "look", None);
    assert_eq!(run.shown, super::ShownChanges::default());

    let run = super::run_fired_locked(&state, &mut p, &mut c, "tar goblin", None);
    let target = run.shown.target.expect("the new target");
    assert_eq!(target.name.as_deref(), Some("goblin"));
    assert!(!run.shown.repaint);
    // The same target again changes nothing.
    let run = super::run_fired_locked(&state, &mut p, &mut c, "tar goblin", None);
    assert_eq!(run.shown, super::ShownChanges::default());

    let run = super::run_fired_locked(&state, &mut p, &mut c, "#prompt default", None);
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
    let mut c = Connection::default();
    p.ui.theme = "night-ink".into();
    p.ui.font_family = "Iosevka".into();
    p.ui.keep_last_command = true;
    let _ = super::run_fired_locked(&state, &mut p, &mut c, "#alias greet wave", None);

    let run = super::run_fired_locked(&state, &mut p, &mut c, "#profile reset", Some(&layer));

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
    let mut c = Connection::default();
    let result = vosh_automation::trigger::process(&p.triggers, b"You are hungry.", c.stop_key);
    let outcome = super::run_trigger_scripts(&mut p, &mut c, &result);
    assert!(p.triggers.is_stopped("hunger", c.stop_key));
    let apply = crate::script::apply_actions(&mut p, &mut c, outcome);
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
    let result = vosh_automation::trigger::process(&p.triggers, b"You are hungry.", c.stop_key);
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
    let mut c = Connection::default();
    let result = vosh_automation::trigger::process(&p.triggers, b"The day has begun.", c.stop_key);
    let outcome = super::run_trigger_scripts(&mut p, &mut c, &result);
    crate::script::apply_actions(&mut p, &mut c, outcome);
    let msg = vosh_protocol::gmcp::Message {
        package: "World.Time".into(),
        data: serde_json::json!({}),
    };
    let (_, apply) = super::gmcp_step(&mut p, &mut c, &msg, tokio::time::Instant::now());
    assert!(p.triggers.is_stopped("day", c.stop_key));
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
    let mut c = Connection::default();
    p.aliases.set(
        vosh_automation::alias::Alias::new("heal", "ignored")
            .with_script("mud.send('cast heal') while true do end"),
    );
    // The second heal of the line runs nothing once the first stopped.
    let ran = crate::input::run_line(&state, &mut p, &mut c, "heal;heal");
    let apply = super::line_script_result(ran);
    let leftover = &apply.send_bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(
        apply.echoes,
        [lua_error(
            "Vosh stopped the Lua in alias heal after 100 ms. heal stays off until you save it or restart Vosh."
        )]
    );
    assert!(p.aliases.is_stopped(None, "heal", c.stop_key));
    // Typed again, it passes through, as an alias you turned off does.
    let ran = crate::input::run_line(&state, &mut p, &mut c, "heal");
    assert_eq!(super::line_script_result(ran).send_bytes, b"heal\r\n");
}

#[test]
fn one_result_runs_100_mud_input_lines_in_all_its_rounds() {
    let lines = |n: usize| -> Vec<(LineFrom, String)> {
        (0..n)
            .map(|_| (LineFrom::YourLua, "#lua fan()".to_string()))
            .collect()
    };
    let mut budget = super::InputBudget::new();
    let (run, said) = budget.take(lines(60));
    assert_eq!(run.len(), 60);
    assert!(said.is_empty(), "{said:?}");
    let (run, said) = budget.take(lines(100));
    assert_eq!(run.len(), 40);
    assert_eq!(
        said,
        ["\x1b[90m[lua]\x1b[0m \x1b[31mVosh ran 100 lines from mud.input and dropped the rest.\x1b[0m"]
    );
    // Lines that each asked for 100 more run none, and Vosh says so once.
    let (run, said) = budget.take(lines(40 * 100));
    assert!(run.is_empty(), "{run:?}");
    assert!(said.is_empty(), "{said:?}");
}

#[test]
fn a_timer_in_a_group_that_is_off_waits_and_starts_fresh_when_it_comes_back() {
    use std::collections::HashMap;
    use std::time::Duration;

    use tokio::time::Instant;

    use crate::session::conn::due_settings_timers;

    let mut p = Profile::default();
    for (id, group) in [(1, Some("upkeep")), (2, None)] {
        p.timers.push(crate::profile::live::Timer {
            id,
            name: String::new(),
            interval_secs: 10,
            command: format!("timer {id}"),
            enabled: true,
            group: group.map(Into::into),
        });
    }
    let mut next = HashMap::new();
    let t0 = Instant::now();
    // The first look seeds each deadline one interval out.
    let leftover = &due_settings_timers(&p, &mut next, t0);
    assert!(leftover.is_empty(), "{leftover:?}");
    let secs = |n| t0 + Duration::from_secs(n);
    assert_eq!(
        due_settings_timers(&p, &mut next, secs(10)),
        ["timer 1", "timer 2"]
    );
    p.disabled_timer_groups.insert("upkeep".into());
    assert_eq!(due_settings_timers(&p, &mut next, secs(20)), ["timer 2"]);
    assert!(!next.contains_key(&1));
    // Back on, it waits a whole interval from now rather than firing
    // for the slots it missed.
    p.disabled_timer_groups.clear();
    assert_eq!(
        due_settings_timers(&p, &mut next, secs(25)),
        Vec::<String>::new()
    );
    assert_eq!(due_settings_timers(&p, &mut next, secs(30)), ["timer 2"]);
    assert_eq!(due_settings_timers(&p, &mut next, secs(35)), ["timer 1"]);
}
