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
    search_page(
        &state,
        pattern,
        case_sensitive,
        max_results,
        scope,
        session,
        before_line_id,
        with_total,
    )
    .await
}

/// The search behind [`logs_search_page`], which the tests call too.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn search_page(
    state: &SharedState,
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
        scope: scope.resolve(state, session)?,
        before_line_id,
    };
    let shared = state.clone();
    read_logs(state, move |store| {
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

/// Save the lines in `scope` to your Downloads folder as `<name>.txt`,
/// or with `with_ansi` as `<name>.log` with the game's colors, adding
/// ` (2)` and on when that file is there. Returns the file's name.
#[tauri::command]
pub(crate) async fn logs_save<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, SharedState>,
    scope: LogScope,
    session: Option<SessionId>,
    with_ansi: bool,
    name: String,
) -> Result<String, String> {
    let scope = scope.resolve(&state, session)?;
    let downloads = crate::ipc::downloads_dir(&app)?;
    read_logs(&state, move |store| {
        save(store, &scope, with_ansi, &name, &downloads)
    })
    .await
    .ok_or_else(|| "Vosh has not opened your logs yet.".to_string())?
}

/// Write the lines in `scope` to a new file in `dir`, see [`logs_save`].
fn save(
    store: &LogStore,
    scope: &Scope,
    with_ansi: bool,
    name: &str,
    dir: &std::path::Path,
) -> Result<String, String> {
    let ext = if with_ansi { "log" } else { "txt" };
    let path = crate::disk::paths::export_path(dir, name, ext);
    let could_not = |e: &dyn std::fmt::Display| {
        tracing::warn!(path = %path.display(), error = %e, "could not save a log");
        "Vosh could not save the log in your Downloads folder.".to_string()
    };
    // Made new, so a file that came since the look stays as it is.
    let file = std::fs::File::create_new(&path).map_err(|e| could_not(&e))?;
    let mut out = std::io::BufWriter::new(file);
    let written = store
        .export_scope(scope, with_ansi, &mut out)
        .and_then(|_| std::io::Write::flush(&mut out).map_err(Into::into));
    if let Err(e) = written {
        drop(out);
        let _ = std::fs::remove_file(&path);
        return Err(could_not(&e));
    }
    Ok(path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default())
}

/// How many days Vosh keeps a log, or None to keep it forever.
#[tauri::command]
pub(crate) async fn logs_keep_get(state: State<'_, SharedState>) -> Result<Option<u32>, String> {
    Ok(state.loaded_profile_set().await?.keep_logs_days())
}

/// Keep logs for `days`, one of 365, 90 and 30, or forever with None,
/// then delete the logs past the new span (D34).
#[tauri::command]
pub(crate) async fn logs_keep_set(
    state: State<'_, SharedState>,
    days: Option<u32>,
) -> Result<(), String> {
    if days.is_some_and(|d| !crate::logs::retention::KEEP_DAYS.contains(&d)) {
        return Err("Vosh keeps logs for a year, 90 days, 30 days or forever.".to_string());
    }
    state
        .loaded_profile_set()
        .await?
        .set_keep_logs_days(days)
        .map_err(|e| e.to_string())?;
    let shared: SharedState = state.inner().clone();
    tauri::async_runtime::spawn(async move { crate::logs::retention::run(&shared).await });
    Ok(())
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

    #[test]
    fn save_writes_a_new_file_each_time() {
        let dir = std::env::temp_dir().join(format!(
            "vosh-logs-save-{}-{}",
            std::process::id(),
            crate::session::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = LogStore::in_memory().unwrap();
        let id = store.start_session("h", 1, 0).unwrap();
        store
            .append_raw(id, 1, b"\x1b[33mMaren walks in.\x1b[0m")
            .unwrap();
        let scope = Scope::log(id);
        let first = save(&store, &scope, false, "Vosh log, last 7 days", &dir).unwrap();
        assert_eq!(first, "Vosh log, last 7 days.txt");
        assert_eq!(
            std::fs::read(dir.join(&first)).unwrap(),
            b"Maren walks in.\n"
        );
        let colored = save(&store, &scope, true, "Vosh log, last 7 days", &dir).unwrap();
        assert_eq!(colored, "Vosh log, last 7 days.log");
        assert_eq!(
            std::fs::read(dir.join(&colored)).unwrap(),
            b"\x1b[33mMaren walks in.\x1b[0m\n"
        );
        let again = save(&store, &scope, false, "Vosh log, last 7 days", &dir).unwrap();
        assert_eq!(again, "Vosh log, last 7 days (2).txt");
        let _ = std::fs::remove_dir_all(&dir);
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
