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

use crate::app::events::{
    broadcast_list_changes, ListChanges, ListRevisions, ALIASES_CHANGED, FLUSH_PENDING_WRITES,
    GROUPS_CHANGED, MACRO_GROUPS_CHANGED, PROFILE_CHANGED, PROMPT_CONFIG_CHANGED,
    SESSION_IDENTITY_CHANGED, TRIGGERS_CHANGED,
};
use crate::app::state::{AppState, SharedState};
use crate::input::LineEffects;
use crate::profile::live::{Macro, Profile};
use crate::profile::open::OpenProfile;
use crate::session::connection::Connection;

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
    crate::app::events::broadcast(handle, "vosh://any-event", &"payload");
    listening.finish("broadcast", &mut heard, &mut want);

    let listening = Heard::listen(
        &app,
        &[
            TRIGGERS_CHANGED,
            ALIASES_CHANGED,
            PROMPT_CONFIG_CHANGED,
            MACRO_GROUPS_CHANGED,
            GROUPS_CHANGED,
        ],
    );
    broadcast_list_changes(
        handle,
        &state.selected_session().profile(),
        ListChanges {
            triggers: true,
            aliases: true,
            prompt: true,
            macro_groups: true,
            groups: true,
        },
    );
    listening.finish("broadcast_list_changes", &mut heard, &mut want);

    let listening = Heard::listen(&app, &[PROFILE_CHANGED]);
    crate::profile::inactive::broadcast_profile_changed(handle, "Ilsabet");
    listening.finish("broadcast_profile_changed", &mut heard, &mut want);

    let listening = Heard::listen(&app, &[crate::app::events::MIGRATION_APPLIED]);
    crate::loadouts::wizard::apply::announce_migration_applied(handle);
    listening.finish("announce_migration_applied", &mut heard, &mut want);

    tauri::async_runtime::block_on(async {
        let listening = Heard::listen(&app, &[SESSION_IDENTITY_CHANGED]);
        crate::session::identity::broadcast_session_identity(
            handle,
            &state,
            &state.selected_session(),
        )
        .await;
        listening.finish("broadcast_session_identity", &mut heard, &mut want);

        // A profile switch, an import, and `#profile load` and `reset`
        // send these.
        let replaced: Vec<&'static str> =
            crate::app::events::profile_ui_events(&state, &Profile::default())
                .events()
                .into_iter()
                .map(|(event, _)| event)
                .collect();
        let listening = Heard::listen(&app, &replaced);
        crate::app::events::broadcast_profile_ui(handle, &state).await;
        listening.finish("broadcast_profile_ui", &mut heard, &mut want);

        // A `#tick` command.
        let listening = Heard::listen(&app, &[crate::app::events::TICK_CONFIG_CHANGED]);
        let tick = LineEffects {
            tick_before: Some(crate::tick::TickConfig::default()),
            ..LineEffects::default()
        };
        let session = state.selected_session();
        let open = session.profile();
        crate::disk::save::settle_line_effects(handle, &session, &open, tick, None).await;
        listening.finish("settle_line_effects", &mut heard, &mut want);

        // Each window answers the quit request the way its page does, so
        // the quit does not wait out the time limit.
        for window in app.webview_windows().into_values() {
            let answer = window.clone();
            window.listen(FLUSH_PENDING_WRITES, move |_| {
                crate::ipc::windows::pending_writes_flushed(answer.clone());
            });
        }
        let listening = Heard::listen(&app, &[FLUSH_PENDING_WRITES]);
        crate::app::exit::ask_windows_to_flush(handle).await;
        listening.finish("ask_windows_to_flush", &mut heard, &mut want);
    });

    assert_eq!(heard, want);
}

#[test]
fn the_prompt_table_event_names_the_profile_in_front() {
    let app = app_with_settings_open();
    let handle = app.handle();
    let payloads = Arc::new(Mutex::new(Vec::new()));
    let heard = payloads.clone();
    let id = app.listen_any(PROMPT_CONFIG_CHANGED, move |event| {
        heard.lock().unwrap().push(event.payload().to_string());
    });
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let front = state.selected_session().profile();
    // Before any profile loads it names none.
    crate::app::events::broadcast_list_changes(handle, &front, ListChanges::PROMPT);
    tauri::async_runtime::block_on(state.selected_profile()).set_name("Second");
    crate::app::events::broadcast_prompt_config_changed(handle, &front);
    // A profile no selected session plays stays behind, and tells no
    // window.
    let behind = state.add_open_profile("Third", Profile::default());
    crate::app::events::broadcast_list_changes(handle, &behind, ListChanges::PROMPT);
    crate::app::events::broadcast_prompt_config_changed(handle, &behind);
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
        preset: None,
    }
}

/// The profile in front in the app of `handle`.
fn in_front(handle: &tauri::AppHandle<MockRuntime>) -> Arc<OpenProfile> {
    handle.state::<SharedState>().selected_session().profile()
}

/// Run `line` the way typed input, timer and tick commands and
/// `mud.input` lines run, on the profile in front, and tell the windows
/// what it changed.
fn run_and_tell(handle: &tauri::AppHandle<MockRuntime>, p: &mut Profile, line: &str) {
    let mut c = Connection::default();
    let before = ListRevisions::of(p, &c);
    let _ = crate::input::run_line(&AppState::default(), p, &mut c, line);
    broadcast_list_changes(handle, &in_front(handle), ListChanges::since(before, p, &c));
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
        ..vosh_script::ScriptOutcome::default()
    };
    let apply = crate::script::apply_actions(p, &mut Connection::default(), outcome);
    broadcast_list_changes(handle, &in_front(handle), apply.lists);
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
    let mut wave = vosh_automation::alias::Alias::new("greet", "wave");
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
    use crate::loadouts::set::{Loadout, LoadoutSet};
    let app = app_with_settings_open();
    let handle = app.handle();
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let dir = tempfile::tempdir().unwrap();
    state.app_data.set(dir.path().to_path_buf()).unwrap();
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
            ..Default::default()
        });
        state.selected_profile().await.macros = vec![
            grouped_macro("F1", "kick", "combat"),
            grouped_macro("F2", "north", "travel"),
        ];
        let switch = |active: &[&str]| {
            let active = active.iter().copied().map(String::from).collect();
            crate::loadouts::set::set_active_loadouts(handle, active, None)
        };
        let off = || async {
            let p = state.selected_profile().await;
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
    assert!(crate::disk::paths::loadouts_path(dir.path()).exists());
}

#[test]
fn a_group_switch_tells_every_window_once() {
    use crate::script::GroupList;
    let app = app_with_settings_open();
    let handle = app.handle();
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let mut heard = Report::new();
    let mut want = Report::new();
    tauri::async_runtime::block_on(async {
        state.selected_profile().await.macros = vec![grouped_macro("F1", "kick", "combat")];
        let turn = |enabled| {
            crate::ipc::automation::groups_set_enabled(
                handle.clone(),
                app.state::<SharedState>(),
                GroupList::Macros,
                "combat".into(),
                enabled,
                None,
            )
        };
        // Settings follows the switch, and the command line its keys.
        let listening = Heard::listen(&app, &[GROUPS_CHANGED, MACRO_GROUPS_CHANGED]);
        turn(false).await.unwrap();
        listening.finish("groups_set_enabled", &mut heard, &mut want);
        // Off already, so nothing turned.
        let listening = Heard::listen(&app, &[GROUPS_CHANGED, MACRO_GROUPS_CHANGED]);
        turn(false).await.unwrap();
        listening.finish_unheard("groups_set_enabled again", &mut heard, &mut want);
    });
    assert_eq!(heard, want);
}

/// `wait_full`, a plugin whose loop never ends, which never returns
/// while you are hurt.
const WAIT_FULL: &str = "mud.on_gmcp('Char.Vitals', function(data)
  while data.hp < data.maxhp do end
  mud.send('stand')
end)
";

#[test]
fn every_change_to_the_plugins_tells_every_window_once() {
    use crate::app::events::PLUGINS_CHANGED;
    use crate::ipc::scripts;
    // The commands print in the terminal, which feeds the shared grid.
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    let app = app_with_settings_open();
    let handle = app.handle();
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let dir = tempfile::tempdir().unwrap();
    state.app_data.set(dir.path().to_path_buf()).unwrap();
    let name = || "wait_full".to_string();
    let mut heard = Report::new();
    let mut want = Report::new();
    tauri::async_runtime::block_on(async {
        let listening = Heard::listen(&app, &[PLUGINS_CHANGED]);
        scripts::plugin_create(handle.clone(), app.state(), name(), None)
            .await
            .unwrap();
        listening.finish("plugin_create", &mut heard, &mut want);

        let manifest = scripts::plugin_read(app.state(), name(), None)
            .await
            .unwrap()
            .manifest;
        let listening = Heard::listen(&app, &[PLUGINS_CHANGED]);
        scripts::plugin_save(
            handle.clone(),
            app.state(),
            name(),
            manifest,
            WAIT_FULL.into(),
            None,
        )
        .await
        .unwrap();
        listening.finish("plugin_save", &mut heard, &mut want);

        for on in [false, true] {
            let listening = Heard::listen(&app, &[PLUGINS_CHANGED]);
            scripts::plugin_set_enabled(handle.clone(), app.state(), name(), on, None)
                .await
                .unwrap();
            let helper = if on {
                "plugin_set_enabled on"
            } else {
                "plugin_set_enabled off"
            };
            listening.finish(helper, &mut heard, &mut want);
        }

        let listening = Heard::listen(&app, &[PLUGINS_CHANGED]);
        scripts::plugin_reload(handle.clone(), app.state(), name(), None)
            .await
            .unwrap();
        listening.finish("plugin_reload", &mut heard, &mut want);

        // A packet that finds you hurt runs the loop until Vosh stops it,
        // on the path every step's Lua takes.
        let session = state.selected_session();
        let listening = Heard::listen(&app, &[PLUGINS_CHANGED]);
        let apply = {
            let mut p = session.lock_profile().await;
            let mut c = session.connection.lock();
            let hurt = serde_json::json!({"hp": 186, "maxhp": 1020});
            let stopped = c.script.dispatch_gmcp("Char.Vitals", &hurt);
            crate::script::apply_actions(&mut p, &mut c, stopped).ran_under(p.open())
        };
        assert!(apply.plugin_stopped);
        crate::session::effects::collect_script_result(handle, &session, apply).await;
        listening.finish("a plugin stop", &mut heard, &mut want);

        // A profile switch turns the session's plugins over.
        let listening = Heard::listen(&app, &[PLUGINS_CHANGED]);
        crate::app::plugins::follow_profile(
            handle,
            &session,
            crate::script::ApplyResult::default(),
        )
        .await;
        listening.finish("follow_profile", &mut heard, &mut want);

        // Install puts back the plugin you exported, which turns it off
        // in every profile first, and Remove deletes it.
        let set = crate::profile::set::ProfileSet::load_or_migrate(dir.path().to_path_buf());
        state.set_profiles(set.unwrap()).await;
        let plugins = crate::disk::paths::plugins_dir(dir.path());
        let exported = crate::app::plugins::archive::export(&plugins, &name(), dir.path()).unwrap();
        let bytes = std::fs::read(exported).unwrap();
        let listening = Heard::listen(&app, &[PLUGINS_CHANGED]);
        scripts::plugin_install(
            handle.clone(),
            app.state(),
            "wait_full.zip".into(),
            Some(bytes),
            None,
            None,
        )
        .await
        .unwrap();
        listening.finish("plugin_install", &mut heard, &mut want);

        let listening = Heard::listen(&app, &[PLUGINS_CHANGED]);
        scripts::plugin_remove(handle.clone(), app.state(), name(), None)
            .await
            .unwrap();
        listening.finish("plugin_remove", &mut heard, &mut want);
    });
    assert_eq!(heard, want);
}

#[test]
fn an_import_tells_every_window_once() {
    use crate::app::events::{MACROS_CHANGED, PROFILES_CHANGED};
    use crate::import::vosh::apply::{apply_import, AddAs};
    use crate::profile::set::{ProfileSet, DEFAULT_PROFILE_NAME};
    let app = app_with_settings_open();
    let handle = app.handle();
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let dir = tempfile::tempdir().unwrap();
    state.app_data.set(dir.path().to_path_buf()).unwrap();
    let export = include_str!("../../../fixtures/config/export.full.toml");
    let mut heard = Report::new();
    let mut want = Report::new();
    tauri::async_runtime::block_on(async {
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        state.set_profiles(set).await;

        // A new profile changes the list and that profile.
        let listening = Heard::listen(&app, &[PROFILES_CHANGED, PROFILE_CHANGED]);
        apply_import(
            handle,
            &state,
            "Healer profile.toml",
            export,
            AddAs::New,
            "Healer",
            &[],
        )
        .await
        .unwrap();
        listening.finish("an import as a new profile", &mut heard, &mut want);

        // A replace of the profile the selected session plays hands every
        // window its settings and its lists, as `#profile load` does.
        let mut events: Vec<&'static str> =
            crate::app::events::profile_ui_events(&state, &Profile::default())
                .events()
                .into_iter()
                .map(|(event, _)| event)
                .collect();
        events.extend([
            TRIGGERS_CHANGED,
            ALIASES_CHANGED,
            PROMPT_CONFIG_CHANGED,
            MACROS_CHANGED,
            PROFILES_CHANGED,
            PROFILE_CHANGED,
        ]);
        let listening = Heard::listen(&app, &events);
        apply_import(
            handle,
            &state,
            "Healer profile.toml",
            export,
            AddAs::Replace,
            DEFAULT_PROFILE_NAME,
            &[],
        )
        .await
        .unwrap();
        listening.finish("an import over the selected profile", &mut heard, &mut want);
    });
    assert_eq!(heard, want);
}
