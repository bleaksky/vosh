//! The commands for the log view in Settings. It lists your saved
//! sessions, searches them a page at a time and exports one. Each reads
//! on its own connection to the log when that opened, on the blocking
//! pool, so a long search never holds up the live session or the async
//! workers.

use std::sync::atomic::Ordering;

use serde::Deserialize;
use tauri::State;
use vosh_log::{LogStore, Scope, SearchOptions, SearchPage, SessionRow};

use crate::app::state::{AppState, SharedState};
use crate::sessions::SessionId;

/// Which logs the view reads, as the page sends it.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct LogScope {
    /// One log, picked in the view.
    log: Option<i64>,
    /// The logs the session opened since Vosh started, This session in
    /// the view.
    this_session: bool,
    /// Only the logs of connections to this host and port.
    host: Option<String>,
    port: Option<u16>,
    /// Only the lines at or after this time, in Unix ms.
    since_ms: Option<i64>,
    /// Leave out logs of connections to this machine.
    hide_local: bool,
}

impl LogScope {
    /// The log store's scope, with This session read from `session`, or
    /// the selected session when it names none.
    fn resolve(self, state: &AppState, session: Option<SessionId>) -> Result<Scope, String> {
        let logs = if let Some(log) = self.log {
            Some(vec![log])
        } else if self.this_session {
            Some(state.session(session)?.logs())
        } else {
            None
        };
        Ok(Scope {
            logs,
            world: self.host.zip(self.port),
            since_ms: self.since_ms,
            hide_local: self.hide_local,
        })
    }
}

/// Run `read` on the blocking pool, on the log store's read connection,
/// or on the writer when the read connection did not open. None when
/// neither is open.
async fn read_logs<T: Send + 'static>(
    state: &SharedState,
    read: impl FnOnce(&LogStore) -> T + Send + 'static,
) -> Option<T> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        {
            let guard = state.log_reader.blocking_lock();
            if let Some(store) = guard.as_ref() {
                return Some(read(store));
            }
        }
        let guard = state.logs.blocking_lock();
        guard.as_ref().map(read)
    })
    .await
    .ok()
    .flatten()
}

/// The logs in `scope`, newest first. A zero limit lists every one.
#[tauri::command]
pub(crate) async fn logs_list_sessions(
    state: State<'_, SharedState>,
    limit: usize,
    scope: LogScope,
    session: Option<SessionId>,
) -> Result<Vec<SessionRow>, String> {
    let scope = scope.resolve(&state, session)?;
    read_logs(&state, move |store| store.list_sessions(limit, &scope))
        .await
        .unwrap_or_else(|| Ok(Vec::new()))
        .map_err(|e| e.to_string())
}

/// One page of the Settings log view: the newest `max_results` matches
/// in `scope` older than `before_line_id`, oldest first, and with
/// `with_total` the number of lines in that scope that match. A pattern
/// the regex engine cannot read comes back as an error starting
/// `regex:`. Each call replaces the one before, which stops reading and
/// comes back as an error starting `stopped:`, which the page drops.
#[tauri::command]
pub(crate) async fn logs_search_page(
    state: State<'_, SharedState>,
    pattern: String,
    case_sensitive: bool,
    max_results: usize,
    scope: LogScope,
    session: Option<SessionId>,
    before_line_id: Option<i64>,
    with_total: bool,
) -> Result<SearchPage, String> {
    let ticket = state.log_searches.fetch_add(1, Ordering::AcqRel) + 1;
    let opts = SearchOptions {
        case_sensitive,
        max_results,
        scope: scope.resolve(&state, session)?,
        before_line_id,
    };
    let shared: SharedState = state.inner().clone();
    read_logs(&state, move |store| {
        let replaced = || shared.log_searches.load(Ordering::Acquire) != ticket;
        store.search_page_until(&pattern, &opts, with_total, &replaced)
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
    read_logs(&state, move |store| {
        store.export_session(session_id, with_ansi)
    })
    .await
    .ok_or_else(|| "log store not ready".to_string())?
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_session_reads_the_logs_the_session_opened() {
        let state = AppState::default();
        let session = state.selected_session();
        session.note_log(4);
        session.note_log(9);
        let scope = LogScope {
            this_session: true,
            host: Some("play.theforsakenlands.com".into()),
            port: Some(9009),
            ..LogScope::default()
        }
        .resolve(&state, None)
        .unwrap();
        assert_eq!(scope.logs, Some(vec![4, 9]));
        assert_eq!(
            scope.world,
            Some(("play.theforsakenlands.com".to_string(), 9009))
        );
        // A picked log wins, and a host with no port names no world.
        let scope = LogScope {
            log: Some(2),
            this_session: true,
            host: Some("h".into()),
            ..LogScope::default()
        }
        .resolve(&state, None)
        .unwrap();
        assert_eq!((scope.logs, scope.world), (Some(vec![2]), None));
    }

    #[tokio::test]
    async fn a_new_search_stops_the_one_before() {
        let state: SharedState = std::sync::Arc::new(AppState::default());
        let mut store = LogStore::in_memory().unwrap();
        let id = store.start_session("h", 1, 0).unwrap();
        store.append(id, 1, "Orla waves", None).unwrap();
        *state.log_reader.lock().await = Some(store);
        // The first search reads only while its count is the newest.
        let ticket = state.log_searches.fetch_add(1, Ordering::AcqRel) + 1;
        state.log_searches.fetch_add(1, Ordering::AcqRel);
        let shared = state.clone();
        let page = read_logs(&state, move |store| {
            let replaced = || shared.log_searches.load(Ordering::Acquire) != ticket;
            store.search_page_until("Orla", &SearchOptions::default(), true, &replaced)
        })
        .await
        .unwrap();
        assert!(matches!(page, Err(vosh_log::LogError::Stopped)));
    }
}
