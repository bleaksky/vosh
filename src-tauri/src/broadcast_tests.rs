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
use crate::list_events::{ListChanges, ALIASES_CHANGED, PROMPT_CONFIG_CHANGED, TRIGGERS_CHANGED};
use crate::profile::Profile;

/// The main window, and Settings open beside it.
const WINDOWS: [&str; 2] = ["main", "settings"];

/// A mock app with the main and Settings windows open and the app state
/// managed, the way the app runs while you have Settings open.
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
        &[TRIGGERS_CHANGED, ALIASES_CHANGED, PROMPT_CONFIG_CHANGED],
    );
    crate::list_events::broadcast_list_changes(
        handle,
        ListChanges {
            triggers: true,
            aliases: true,
            prompt: true,
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
