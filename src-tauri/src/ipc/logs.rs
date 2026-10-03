//! The commands for the log view in Settings. It lists your saved
//! sessions, searches them a page at a time and exports one. Each reads
//! on its own connection to the log when that opened, so a long search
//! never holds up the live session.

use tauri::State;
use vosh_log::{SearchOptions, SearchPage, SessionRow};

use crate::app::state::{AppState, SharedState};

/// Run `read` on the log store's read connection, or on the writer when
/// the read connection did not open. None when neither is open.
async fn read_logs<T>(state: &AppState, read: impl FnOnce(&vosh_log::LogStore) -> T) -> Option<T> {
    {
        let guard = state.log_reader.lock().await;
        if let Some(store) = guard.as_ref() {
            return Some(read(store));
        }
    }
    let guard = state.logs.lock().await;
    guard.as_ref().map(read)
}

#[tauri::command]
pub(crate) async fn logs_list_sessions(
    state: State<'_, SharedState>,
    limit: usize,
    hide_local: Option<bool>,
) -> Result<Vec<SessionRow>, String> {
    read_logs(&state, |store| {
        store.list_sessions(limit, hide_local.unwrap_or(false))
    })
    .await
    .unwrap_or_else(|| Ok(Vec::new()))
    .map_err(|e| e.to_string())
}

/// One page of the Settings log view: the newest `max_results` matches
/// older than `before_line_id`, oldest first, and with `with_total` the
/// number of lines in that scope that match. The view leaves out
/// sessions to this machine with `hide_local`. A pattern the regex
/// engine cannot read comes back as an error starting `regex:`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn logs_search_page(
    state: State<'_, SharedState>,
    pattern: String,
    case_sensitive: bool,
    max_results: usize,
    session_id: Option<i64>,
    before_line_id: Option<i64>,
    hide_local: bool,
    with_total: bool,
) -> Result<SearchPage, String> {
    let opts = SearchOptions {
        case_sensitive,
        max_results,
        session_id,
        before_line_id,
        hide_local,
    };
    read_logs(&state, |store| {
        store.search_page(&pattern, &opts, with_total)
    })
    .await
    .unwrap_or_else(|| {
        Ok(SearchPage {
            hits: Vec::new(),
            total: with_total.then_some(0),
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn logs_export(
    state: State<'_, SharedState>,
    session_id: i64,
    with_ansi: bool,
) -> Result<String, String> {
    read_logs(&state, |store| store.export_session(session_id, with_ansi))
        .await
        .ok_or_else(|| "log store not ready".to_string())?
        .map_err(|e| e.to_string())
}
