//! Every line a command prints in the terminal reaches the native
//! renderer's grid as well as xterm. On macOS the native renderer draws
//! the terminal, and a slash command's echo, the `#logs` reply, and the
//! `[not connected]` line used to go to xterm alone, so they never
//! showed there.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};

use crate::app::state::{AppState, SharedState};
use crate::logs::forget_passwords::{self, Outcome};
use crate::profile::live::Profile;
use crate::term_grid;

/// A mock app with the app state managed, no session open, and no
/// session log.
fn app() -> App<MockRuntime> {
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    app.manage::<SharedState>(Arc::new(AppState::default()));
    app
}

/// Where each row on the native grid's screen that shows `text` sits.
fn rows_showing(rows: &[String], text: &str) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.contains(text))
        .map(|(i, _)| i)
        .collect()
}

/// The native grid's screen once a row shows `text`, or after five
/// seconds. The `#logs` reply comes from a task of its own.
fn screen_once_it_shows(text: &str) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let rows = term_grid::shared_screen_rows_for_test();
        if !rows_showing(&rows, text).is_empty() || Instant::now() >= deadline {
            return rows;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn slash_command_echoes_and_the_logs_reply_reach_the_native_grid() {
    let _shared = term_grid::lock_shared_grid_for_test();
    // Tall and wide enough that nothing wraps or scrolls off.
    term_grid::blank_shared_grid_for_test(160, 200);
    let app = app();
    let handle = app.handle();

    // `#help` through session_send_input would mark the profile to
    // save, and a save from a mock app writes outside the test. So the
    // echo the input pipeline gives for it goes through the same helper
    // session_send_input prints echoes with.
    let help = crate::input::process(&mut Profile::default(), "#help").echo;
    assert_eq!(help.first().map(String::as_str), Some("slash commands:"));
    crate::output::echo_lines(handle, &help);

    tauri::async_runtime::block_on(async {
        // `#logs` runs before anything that could save, and `look` is not
        // a slash command, so neither marks the profile to save.
        for line in ["#logs", "#logs forget-passwords", "look"] {
            crate::ipc::session::session_send_input(handle.clone(), app.state(), line.to_string())
                .await
                .unwrap();
        }
        crate::ipc::session::session_send_masked(handle.clone(), app.state(), "secret".to_string())
            .await
            .unwrap();
    });

    let reply = forget_passwords::message(&Outcome::NoLog);
    let rows = screen_once_it_shows(&reply);
    let help_rows = rows_showing(&rows, "slash commands:");
    let usage_rows = rows_showing(&rows, forget_passwords::USAGE);
    let not_connected_rows = rows_showing(&rows, "[not connected]");
    let reply_rows = rows_showing(&rows, &reply);
    assert_eq!(help_rows.len(), 1, "the #help echo, in {rows:#?}");
    assert_eq!(usage_rows.len(), 1, "the #logs usage, in {rows:#?}");
    assert_eq!(reply_rows.len(), 1, "the #logs reply, in {rows:#?}");
    assert_eq!(
        not_connected_rows.len(),
        2,
        "a [not connected] line for look and one for the masked line, in {rows:#?}"
    );
    // The lines land in the order the commands printed them. The reply
    // to `#logs forget-passwords` comes from its own task, so it lands
    // after the usage line and may land before or after the rest.
    assert!(help_rows[0] < usage_rows[0], "{rows:#?}");
    assert!(usage_rows[0] < not_connected_rows[0], "{rows:#?}");
    assert!(usage_rows[0] < reply_rows[0], "{rows:#?}");
}
