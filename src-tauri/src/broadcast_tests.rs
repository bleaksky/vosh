//! Every event the backend sends the windows reaches each listener once,
//! whichever windows are open. Tauri hands one emit to every listener in
//! every window, so a helper that emitted once per open window made each
//! listener hear the event once per open window, twice with Settings
//! open.
//!
//! The listeners here are Rust listeners, one per window. Tauri hands
//! each emit to them and to the page listeners alike, once each, so they
//! count what the pages hear.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, EventId, Listener, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::{AppState, SharedState};
use crate::characters::{PROFILE_CHANGED_EVENT, SESSION_IDENTITY_EVENT};
use crate::exit_flush::FLUSH_REQUEST_EVENT;
use crate::input::LineEffects;
use crate::list_events::{
    broadcast_list_changes, ListChanges, ListRevisions, ALIASES_CHANGED, MACRO_GROUPS_CHANGED,
    PROMPT_CONFIG_CHANGED, TRIGGERS_CHANGED,
};
use crate::profile::{Macro, Profile};

/// The main window, and Settings and Help open beside it.
const WINDOWS: [&str; 3] = ["main", "settings", "help"];

/// A mock app with the main, Settings, and Help windows open and the app
/// state managed, the way the app runs while you have both open.
fn app_with_settings_open() -> App<MockRuntime> {
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    app.manage::<SharedState>(Arc::new(AppState::default()));
    for label in WINDOWS {
        WebviewWindowBuilder::new(&app, label, WebviewUrl::default())
            .build()
            .unwrap();
    }
    app
}

/// How many times each window heard each event.
type Counts = BTreeMap<(String, &'static str), usize>;

/// Each helper, with what each window heard of each event it sends.
type Report = BTreeMap<&'static str, Counts>;

/// Listeners for a few events in every open window.
struct Heard {
    events: Vec<&'static str>,
    counts: Arc<Mutex<Counts>>,
    listeners: Vec<(WebviewWindow<MockRuntime>, EventId)>,
}

impl Heard {
    /// Listen for `events` in every open window.
    fn listen(app: &App<MockRuntime>, events: &[&'static str]) -> Self {
        let counts = Arc::new(Mutex::new(Counts::new()));
        let mut listeners = Vec::new();
        for (label, window) in app.webview_windows() {
            for &event in events {
                let counts = counts.clone();
                let label = label.clone();
                let id = window.listen(event, move |_| {
                    *counts
                        .lock()
                        .unwrap()
                        .entry((label.clone(), event))
                        .or_insert(0) += 1;
                });
                listeners.push((window.clone(), id));
            }
        }
        Self {
            events: events.to_vec(),
            counts,
            listeners,
        }
    }

    /// Stop listening, and note under `helper` what each window heard
    /// in `heard`, and once for each event in every window in `want`.
    fn finish(self, helper: &'static str, heard: &mut Report, want: &mut Report) {
        for (window, id) in self.listeners {
            window.unlisten(id);
        }
        heard.insert(helper, self.counts.lock().unwrap().clone());
        let once = WINDOWS
            .into_iter()
            .flat_map(|window| {
                self.events
                    .iter()
                    .map(move |&event| ((window.to_string(), event), 1))
            })
            .collect();
        want.insert(helper, once);
    }

    /// Stop listening, and note under `helper` what each window heard in
    /// `heard`, and that no window should hear any of it in `want`.
    fn finish_unheard(self, helper: &'static str, heard: &mut Report, want: &mut Report) {
        for (window, id) in self.listeners {
            window.unlisten(id);
        }
        heard.insert(helper, self.counts.lock().unwrap().clone());
        want.insert(helper, Counts::new());
    }
}

#[test]
fn every_event_reaches_each_listener_once_with_settings_open() {
    let app = app_with_settings_open();
    let handle = app.handle();
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let mut heard = Report::new();
    let mut want = Report::new();

    let listening = Heard::listen(&app, &["vosh://any-event"]);
    super::broadcast(handle, "vosh://any-event", &"payload");
    listening.finish("broadcast", &mut heard, &mut want);

    let listening = Heard::listen(
        &app,
        &[
            TRIGGERS_CHANGED,
            ALIASES_CHANGED,
            PROMPT_CONFIG_CHANGED,
            MACRO_GROUPS_CHANGED,
        ],
    );
    broadcast_list_changes(
        handle,
        ListChanges {
            triggers: true,
            aliases: true,
            prompt: true,
            macro_groups: true,
        },
    );
    listening.finish("broadcast_list_changes", &mut heard, &mut want);

    let listening = Heard::listen(&app, &[PROFILE_CHANGED_EVENT]);
    crate::characters::broadcast_profile_changed(handle, "Erelei");
    listening.finish("broadcast_profile_changed", &mut heard, &mut want);

    let listening = Heard::listen(&app, &[super::MIGRATION_APPLIED_EVENT]);
    super::announce_migration_applied(handle);
    listening.finish("announce_migration_applied", &mut heard, &mut want);

    tauri::async_runtime::block_on(async {
        let listening = Heard::listen(&app, &[SESSION_IDENTITY_EVENT]);
        crate::characters::broadcast_session_identity(handle, &state).await;
        listening.finish("broadcast_session_identity", &mut heard, &mut want);

        // A profile switch, an import, and `#profile load` and `reset`
        // send these.
        let replaced: Vec<&'static str> = super::profile_ui_events(&Profile::default())
            .events()
            .into_iter()
            .map(|(event, _)| event)
            .collect();
        let listening = Heard::listen(&app, &replaced);
        super::broadcast_profile_ui(handle, &state).await;
        listening.finish("broadcast_profile_ui", &mut heard, &mut want);

        // A `#tick` command.
        let listening = Heard::listen(&app, &[super::TICK_CONFIG_CHANGED_EVENT]);
        let tick = LineEffects {
            tick_changed: true,
            ..LineEffects::default()
        };
        super::settle_line_effects(handle, tick).await;
        listening.finish("settle_line_effects", &mut heard, &mut want);

        // Each window answers the quit request the way its page does, so
        // the quit does not wait out the time limit.
        for window in app.webview_windows().into_values() {
            let answer = window.clone();
            window.listen(FLUSH_REQUEST_EVENT, move |_| {
                crate::exit_flush::pending_writes_flushed(answer.clone());
            });
        }
        let listening = Heard::listen(&app, &[FLUSH_REQUEST_EVENT]);
        crate::exit_flush::ask_windows_to_flush(handle).await;
        listening.finish("ask_windows_to_flush", &mut heard, &mut want);
    });

    assert_eq!(heard, want);
}

#[test]
fn the_prompt_table_event_names_the_active_profile() {
    let app = app_with_settings_open();
    let handle = app.handle();
    let payloads = Arc::new(Mutex::new(Vec::new()));
    let heard = payloads.clone();
    let id = app.listen_any(PROMPT_CONFIG_CHANGED, move |event| {
        heard.lock().unwrap().push(event.payload().to_string());
    });
    // Before any profile loads it names none.
    crate::list_events::broadcast_list_changes(handle, ListChanges::PROMPT);
    let state: SharedState = app.state::<SharedState>().inner().clone();
    state.note_active_profile("Second");
    crate::list_events::broadcast_prompt_config_changed(handle);
    app.unlisten(id);
    assert_eq!(
        *payloads.lock().unwrap(),
        [r#"{"profile":null}"#, r#"{"profile":"Second"}"#]
    );
    assert_eq!(state.active_profile().as_deref(), Some("Second"));
}

/// A macro on `key` that sends `command`, in `group`.
fn grouped_macro(key: &str, command: &str, group: &str) -> Macro {
    Macro {
        key: key.into(),
        command: command.into(),
        group: Some(group.into()),
        enabled: true,
    }
}

/// Run `line` the way typed input, timer and tick commands and
/// `mud.input` lines run, and tell the windows what it changed.
fn run_and_tell(handle: &tauri::AppHandle<MockRuntime>, p: &mut Profile, line: &str) {
    let before = ListRevisions::of(p);
    let _ = crate::input::process(p, line);
    broadcast_list_changes(handle, ListChanges::since(before, p));
}

/// Apply a Lua `mud.set_group_enabled` the way a trigger, a timer, a
/// GMCP handler, a script alias or a plugin load applies what its Lua
/// asked, and tell the windows what it changed.
fn toggle_from_lua(
    handle: &tauri::AppHandle<MockRuntime>,
    p: &mut Profile,
    name: &str,
    enabled: bool,
) {
    let outcome = vosh_script::ScriptOutcome {
        actions: vec![vosh_script::Action::SetGroupEnabled {
            name: name.into(),
            enabled,
        }],
    };
    let apply = crate::script_state::apply_actions(p, outcome);
    broadcast_list_changes(handle, apply.lists);
}

#[test]
fn a_group_toggle_tells_the_command_line_when_a_macro_group_turned() {
    // The command line keeps its own map of the macro keys that fire, so
    // a toggle that turns a macro group off has to reach it, or the keys
    // go on firing.
    let app = app_with_settings_open();
    let handle = app.handle();
    let mut p = Profile {
        macros: vec![grouped_macro("F1", "kick", "combat")],
        ..Profile::default()
    };
    let mut wave = vosh_alias::Alias::new("greet", "wave");
    wave.group = Some("social".into());
    p.aliases.set(wave);
    let mut heard = Report::new();
    let mut want = Report::new();

    let listening = Heard::listen(&app, &[MACRO_GROUPS_CHANGED]);
    run_and_tell(handle, &mut p, "#group combat off");
    listening.finish("#group combat off", &mut heard, &mut want);
    assert!(p.disabled_macro_groups.contains("combat"));

    // Off already, so nothing changed.
    let listening = Heard::listen(&app, &[MACRO_GROUPS_CHANGED]);
    run_and_tell(handle, &mut p, "#group combat off");
    listening.finish_unheard("#group combat off again", &mut heard, &mut want);

    // A `#lua` line runs the toggle inside the line.
    let listening = Heard::listen(&app, &[MACRO_GROUPS_CHANGED]);
    run_and_tell(handle, &mut p, "#lua mud.set_group_enabled('combat', true)");
    listening.finish("#lua mud.set_group_enabled", &mut heard, &mut want);
    assert!(p.disabled_macro_groups.is_empty());

    // Lua that runs outside any line.
    let listening = Heard::listen(&app, &[MACRO_GROUPS_CHANGED]);
    toggle_from_lua(handle, &mut p, "combat", false);
    listening.finish("mud.set_group_enabled", &mut heard, &mut want);
    assert!(p.disabled_macro_groups.contains("combat"));

    // A group no macro is in leaves the command line alone.
    let listening = Heard::listen(&app, &[MACRO_GROUPS_CHANGED]);
    run_and_tell(handle, &mut p, "#group social off");
    toggle_from_lua(handle, &mut p, "social", true);
    listening.finish_unheard("a group with no macro in it", &mut heard, &mut want);

    assert_eq!(heard, want);
}

#[test]
fn a_loadout_switch_tells_the_command_line_when_a_macro_group_turned() {
    use crate::loadout::{Loadout, LoadoutSet};
    let app = app_with_settings_open();
    let handle = app.handle();
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let dir = tempfile::tempdir().unwrap();
    let mut fight = Loadout::empty("fight");
    fight.enabled_groups = vec!["combat".into()];
    let mut walk = Loadout::empty("walk");
    walk.enabled_groups = vec!["travel".into()];
    let mut heard = Report::new();
    let mut want = Report::new();

    tauri::async_runtime::block_on(async {
        *state.loadout_set.lock().await = Some(LoadoutSet {
            active: Vec::new(),
            dormant: false,
            loadouts: vec![fight, walk],
        });
        state.profile.lock().await.macros = vec![
            grouped_macro("F1", "kick", "combat"),
            grouped_macro("F2", "north", "travel"),
        ];
        let switch = |active: &[&str]| {
            let active = active.iter().copied().map(String::from).collect();
            super::set_active_loadouts(handle, dir.path(), active)
        };
        let off = || async {
            let p = state.profile.lock().await;
            p.disabled_macro_groups.iter().cloned().collect::<Vec<_>>()
        };

        let listening = Heard::listen(&app, &[MACRO_GROUPS_CHANGED]);
        switch(&["fight"]).await.unwrap();
        listening.finish("fight turns travel off", &mut heard, &mut want);
        assert_eq!(off().await, ["travel"]);

        // The same loadouts again change nothing.
        let listening = Heard::listen(&app, &[MACRO_GROUPS_CHANGED]);
        switch(&["fight"]).await.unwrap();
        listening.finish_unheard("fight again", &mut heard, &mut want);

        let listening = Heard::listen(&app, &[MACRO_GROUPS_CHANGED]);
        switch(&["walk"]).await.unwrap();
        listening.finish("walk turns combat off", &mut heard, &mut want);
        assert_eq!(off().await, ["combat"]);

        // With none active the catalog sleeps, every group off.
        let listening = Heard::listen(&app, &[MACRO_GROUPS_CHANGED]);
        switch(&[]).await.unwrap();
        listening.finish("none active", &mut heard, &mut want);
        assert_eq!(off().await, ["combat", "travel"]);
    });

    assert_eq!(heard, want);
    // The switch saved loadouts.toml in the scratch folder.
    assert!(crate::loadout_store::loadouts_path(dir.path()).exists());
}
