//! The commands for Get started, the short list a new install opens on.
//! `profiles.toml` keeps where you are in it once
//! for the whole install, and Help opens it again in the main window.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::app::events::GET_STARTED_OPEN;
use crate::app::state::SharedState;
use crate::profile::set::GetStarted;

/// Where you are in Get started, as the page reads it.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetStartedState {
    /// The card opens at launch.
    pub at_launch: bool,
    /// The steps you finished, by id.
    pub done: Vec<String>,
}

impl From<&GetStarted> for GetStartedState {
    fn from(kept: &GetStarted) -> Self {
        Self {
            at_launch: kept.at_launch,
            done: kept.done.clone(),
        }
    }
}

/// Where you are in Get started, or None when it never opened.
#[tauri::command]
pub(crate) async fn get_started_get(
    state: State<'_, SharedState>,
) -> Result<Option<GetStartedState>, String> {
    let set = state.loaded_profile_set().await?;
    Ok(set.get_started().map(GetStartedState::from))
}

/// Keep whether the card opens at launch and the steps you finished.
#[tauri::command]
pub(crate) async fn get_started_set(
    state: State<'_, SharedState>,
    at_launch: bool,
    done: Vec<String>,
) -> Result<(), String> {
    let mut set = state.loaded_profile_set().await?;
    set.set_get_started(at_launch, done)
        .map_err(|e| e.to_string())
}

/// Bring the main window forward and open Get started in it.
#[tauri::command]
pub(crate) fn open_get_started<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
    app.emit_to("main", GET_STARTED_OPEN, ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tauri::{App, Manager};

    use super::{get_started_get, get_started_set, GetStartedState};
    use crate::app::state::{AppState, SharedState};
    use crate::disk::paths;
    use crate::profile::set::ProfileSet;

    /// A mock app over a fresh install in `root`.
    async fn fresh(root: &std::path::Path) -> App<MockRuntime> {
        let state: SharedState = Arc::new(AppState::default());
        state
            .set_profiles(ProfileSet::load_or_migrate(root.to_path_buf()).unwrap())
            .await;
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("a mock app");
        app.manage::<SharedState>(state);
        app
    }

    #[tokio::test]
    async fn get_started_opens_at_launch_on_a_fresh_install() {
        let dir = tempfile::tempdir().unwrap();
        let app = fresh(dir.path()).await;
        let open = GetStartedState {
            at_launch: true,
            done: Vec::new(),
        };
        assert_eq!(get_started_get(app.state()).await, Ok(Some(open)));
        let sent = serde_json::to_value(get_started_get(app.state()).await.unwrap()).unwrap();
        assert_eq!(sent, serde_json::json!({ "atLaunch": true, "done": [] }));
    }

    #[tokio::test]
    async fn get_started_set_with_none_done_writes_no_done_key() {
        let dir = tempfile::tempdir().unwrap();
        let app = fresh(dir.path()).await;
        get_started_set(app.state(), false, Vec::new())
            .await
            .unwrap();
        let text = std::fs::read_to_string(paths::profiles_index_path(dir.path())).unwrap();
        assert!(
            text.ends_with("[get_started]\nat_launch = false\n"),
            "{text}"
        );
        assert!(!text.contains("done"), "{text}");
    }

    #[tokio::test]
    async fn get_started_keeps_at_launch_off_after_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let app = fresh(dir.path()).await;
        get_started_set(app.state(), false, vec!["connect".into()])
            .await
            .unwrap();
        let reloaded = fresh(dir.path()).await;
        let shut = GetStartedState {
            at_launch: false,
            done: vec!["connect".into()],
        };
        assert_eq!(get_started_get(reloaded.state()).await, Ok(Some(shut)));
    }
}
