//! The commands for alerts: what the alert presets of a profile do,
//! whether the system lets Vosh post banners, the system's own question,
//! and the system page where you turn banners on. The Alerts category of
//! the Presets page calls the two preset commands, and the Alert row the
//! other three.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::alert::banner::{self, Permission};
use crate::alert::presets::PRESETS;
use crate::alert::AlertParts;
use crate::app::state::SharedState;
use crate::disk::save::{save_by, SavePolicy};

/// What the alert presets of a profile do, and which of them ring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AlertPresets {
    /// What each preset does, by id, the profile's `[alerts]` table.
    pub(crate) alerts: BTreeMap<String, AlertParts>,
    /// The ids of the five presets, in the order the Alerts category
    /// lists them.
    pub(crate) ids: Vec<String>,
    /// The ids that ring, the alert presets `ui.enabled_presets` lists.
    pub(crate) on: Vec<String>,
}

/// What the alert presets of `profile` do, the profile Settings shows,
/// or of the selected session's profile when it names none.
#[tauri::command]
pub(crate) async fn alert_presets_get(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<AlertPresets, String> {
    let p = state.lock_named(profile).await?;
    let on = PRESETS
        .iter()
        .filter(|id| crate::alert::presets::parts(&p, id).is_some())
        .map(|id| (*id).to_string())
        .collect();
    Ok(AlertPresets {
        alerts: p.alerts.clone(),
        ids: PRESETS.iter().map(|id| (*id).to_string()).collect(),
        on,
    })
}

/// Set what the alert preset `id` does in `profile`, or in the selected
/// session's profile when it names none, or with no `alert`, forget it so
/// it posts a banner alone, and save.
/// Whether it rings stays with the presets list, which Save in the
/// Presets card writes as for any preset.
#[tauri::command]
pub(crate) async fn alert_presets_set<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    id: String,
    alert: Option<AlertParts>,
    profile: Option<String>,
) -> Result<(), String> {
    if !PRESETS.contains(&id.as_str()) {
        return Err(format!("Vosh has no alert preset named {id}."));
    }
    let open = {
        let mut p = state.lock_named(profile).await?;
        match alert {
            Some(parts) => p.alerts.insert(id, parts),
            None => p.alerts.remove(&id),
        };
        p.open().clone()
    };
    save_by(&app, &state, &open, SavePolicy::Now).await;
    Ok(())
}

/// Whether the system lets Vosh post banners: `granted`, `denied`,
/// `not_asked`, or `unavailable` in a build that cannot post them, such
/// as a dev build on macOS.
#[tauri::command]
pub(crate) async fn alerts_permission() -> Result<Permission, String> {
    tauri::async_runtime::spawn_blocking(banner::permission)
        .await
        .map_err(|e| e.to_string())
}

/// Ask the system to let Vosh post banners. macOS shows its own question
/// the first time, and the answer comes back once you choose.
#[tauri::command]
pub(crate) async fn alerts_ask_permission() -> Result<Permission, String> {
    Ok(banner::ask().await)
}

/// Open the system's notification settings, at Vosh where the system
/// can, so you can turn banners back on.
#[tauri::command]
pub(crate) async fn alerts_open_settings() -> Result<(), String> {
    banner::open_settings()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::Manager;

    #[tokio::test]
    async fn the_presets_read_back_and_an_unknown_one_is_refused() {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("a mock app");
        let state: SharedState = std::sync::Arc::default();
        app.manage(state.clone());
        state
            .selected_session()
            .lock_profile()
            .await
            .ui
            .enabled_presets = vec!["alert_tells".into(), "healing_basics".into()];
        let tells = AlertParts {
            sound: Some("chime".into()),
            ..AlertParts::default()
        };
        alert_presets_set(
            app.handle().clone(),
            app.state(),
            "alert_tells".into(),
            Some(tells.clone()),
            None,
        )
        .await
        .expect("a preset Vosh knows");
        let got = alert_presets_get(app.state(), None)
            .await
            .expect("the presets");
        assert_eq!(got.alerts, BTreeMap::from([("alert_tells".into(), tells)]));
        assert_eq!(got.on, ["alert_tells"]);
        assert_eq!(got.ids.len(), 5);
        let refused = alert_presets_set(
            app.handle().clone(),
            app.state(),
            "healing_basics".into(),
            None,
            None,
        )
        .await;
        assert_eq!(
            refused,
            Err("Vosh has no alert preset named healing_basics.".into())
        );
    }
}
