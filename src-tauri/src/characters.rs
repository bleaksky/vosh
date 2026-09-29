//! Settings > Characters. A character is a name in a profile's
//! `auto_match`, and the Characters group edits any profile in place,
//! active or not, without switching the live session.
//!
//! The active profile lives in memory, so reads and writes for it go
//! through the live `Profile` the way every other command does. An
//! inactive profile lives only in `profiles/<name>.toml`, so reads load
//! that file and writes rewrite it under [`PERSIST_LOCK`], and an edit
//! to one announces itself as `vosh://profile-changed` rather than the
//! events that carry the active profile's panes and tracked affects to
//! the main window.

use serde::Serialize;
use tauri::State;
use tracing::warn;

use crate::commands::{panes_generation, SharedState, PERSIST_LOCK, PROFILES_NOT_LOADED};
use crate::profile_config::{
    GlobalConfig, PaneLayoutPersist, ProfileConfig, TrackedAffect, UiConfig,
};
use crate::profile_set::{
    display_name, world_name, AutoMatch, ProfileEntry, ProfileSet, ProfileSetError, Scope,
};

/// One profile as the Characters group shows it.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProfileDetail {
    pub name: String,
    /// `Default` for the reserved `default` profile, else the name.
    pub display_name: String,
    /// Whether this is the live profile.
    pub active: bool,
    pub auto_match: Option<AutoMatch>,
    /// The display name of the profile's world, when it has one.
    pub world_name: Option<String>,
    pub tracked_affects: Vec<TrackedAffect>,
    pub panes: PaneLayoutPersist,
    /// The live pane generation for the active profile. None for an
    /// inactive one, whose tree no pane layout write can target.
    pub generation: Option<u64>,
    /// Whether the login toggle reads on. See [`ProfileSet::login_on`].
    pub login_on: bool,
}

impl ProfileDetail {
    fn new(
        entry: ProfileEntry,
        active: bool,
        login_on: bool,
        ui: &UiConfig,
        generation: Option<u64>,
    ) -> Self {
        let world_name = entry
            .auto_match
            .as_ref()
            .and_then(|am| am.host.as_deref())
            .map(world_name);
        Self {
            display_name: display_name(&entry.name),
            name: entry.name,
            active,
            auto_match: entry.auto_match,
            world_name,
            tracked_affects: ui.tracked_affects.clone(),
            panes: ui.pane_layout(),
            generation,
            login_on,
        }
    }
}

fn not_found(name: &str) -> String {
    ProfileSetError::NotFound(name.to_string()).to_string()
}

/// Lay the global dock layout over `ui` when that category is global,
/// the way a switch applies global.toml, so an inactive profile that
/// never saved panes migrates the same tree it will show once live.
fn apply_global_dock(set: &ProfileSet, ui: &mut UiConfig) {
    if !matches!(set.scope().dock_layout, Scope::Global) {
        return;
    }
    let path = set.global_path();
    if !path.exists() {
        return;
    }
    match GlobalConfig::load(&path) {
        Ok(global) => {
            if let Some(dock) = global.dock_layout {
                ui.dock_layout = dock;
            }
        }
        Err(e) => warn!(error = %e, path = %path.display(), "global config unreadable"),
    }
}

/// Load `name`'s file, or the defaults for a profile that never saved
/// one, the way a switch to it would.
fn load_profile_file(set: &ProfileSet, name: &str) -> Result<ProfileConfig, String> {
    let path = set.profile_path(name);
    if !path.exists() {
        return Ok(ProfileConfig::default());
    }
    ProfileConfig::load(&path).map_err(|e| {
        warn!(error = %e, path = %path.display(), "profile file unreadable");
        format!(
            "Vosh could not read the {} profile file.",
            display_name(name)
        )
    })
}

/// An inactive profile's UI config as it would load once live.
pub(crate) fn stored_ui(set: &ProfileSet, name: &str) -> Result<UiConfig, String> {
    let mut ui = load_profile_file(set, name)?.ui;
    apply_global_dock(set, &mut ui);
    Ok(ui)
}

/// Read one profile for the Characters group.
#[tauri::command]
pub(crate) async fn profile_detail_get(
    state: State<'_, SharedState>,
    name: String,
) -> Result<ProfileDetail, String> {
    profile_detail(state.inner(), &name).await
}

/// Body of [`profile_detail_get`]. Holds [`PERSIST_LOCK`] so a switch
/// cannot land between deciding whether `name` is live and reading it.
/// The profile set lock is let go before the live profile is locked,
/// the order the persist takes them in.
pub(crate) async fn profile_detail(
    state: &SharedState,
    name: &str,
) -> Result<ProfileDetail, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let (entry, active, login_on, stored) = {
        let guard = state.profile_set.lock().await;
        let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
        let entry = set.get(name).cloned().ok_or_else(|| not_found(name))?;
        let active = set.active_name() == name;
        let stored = if active {
            None
        } else {
            Some(stored_ui(set, name)?)
        };
        (entry, active, set.login_on(name), stored)
    };
    Ok(match stored {
        Some(ui) => ProfileDetail::new(entry, false, login_on, &ui, None),
        None => {
            let p = state.profile.lock().await;
            ProfileDetail::new(entry, active, login_on, &p.ui, Some(panes_generation()))
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::commands::AppState;
    use crate::profile_config::DockEntryPersist;
    use crate::profile_set::tests::james_like_set;
    use crate::profile_set::{ScopeConfig, DEFAULT_PROFILE_NAME};

    fn affect(name: &str) -> TrackedAffect {
        TrackedAffect {
            name: name.into(),
            label: None,
        }
    }

    fn names(list: &[TrackedAffect]) -> Vec<&str> {
        list.iter().map(|t| t.name.as_str()).collect()
    }

    /// App state over a James-like profile set in `dir`, with `default`
    /// live and tracking Sanctuary.
    async fn james_like_state(dir: &std::path::Path) -> SharedState {
        let state: SharedState = Arc::new(AppState::default());
        state.profile.lock().await.ui.tracked_affects = vec![affect("Sanctuary")];
        *state.profile_set.lock().await = Some(james_like_set(dir));
        state
    }

    fn write_profile(dir: &std::path::Path, name: &str, config: &ProfileConfig) {
        let set = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
        config.save(&set.profile_path(name)).unwrap();
    }

    #[tokio::test]
    async fn detail_reads_the_live_profile_for_the_active_name() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let detail = profile_detail(&state, DEFAULT_PROFILE_NAME).await.unwrap();
        assert!(detail.active);
        assert_eq!(detail.display_name, "Default");
        assert_eq!(detail.world_name.as_deref(), Some("The Forsaken Lands"));
        assert_eq!(names(&detail.tracked_affects), ["Sanctuary"]);
        assert_eq!(detail.panes, PaneLayoutPersist::default_layout());
        // Other tests move the shared generation, so only its presence
        // is stable here.
        assert!(detail.generation.is_some());
        assert!(detail.login_on);
    }

    #[tokio::test]
    async fn detail_reads_an_inactive_profile_from_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let mut config = ProfileConfig::default();
        config.ui.tracked_affects = vec![affect("Haste"), affect("Fly")];
        write_profile(dir.path(), "Healer", &config);

        let detail = profile_detail(&state, "Healer").await.unwrap();
        assert!(!detail.active);
        assert_eq!(detail.display_name, "Healer");
        assert_eq!(names(&detail.tracked_affects), ["Haste", "Fly"]);
        assert_eq!(detail.generation, None);
        assert!(detail.login_on);

        // Test-Prompt never saved a file and loses Erelei to default.
        let detail = profile_detail(&state, "Test-Prompt").await.unwrap();
        assert!(detail.tracked_affects.is_empty());
        assert_eq!(detail.panes, PaneLayoutPersist::default_layout());
        assert!(!detail.login_on);

        assert!(profile_detail(&state, "Nobody").await.is_err());
    }

    #[tokio::test]
    async fn detail_migrates_an_inactive_tree_from_the_global_dock() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let dock = vec![DockEntryPersist {
            id: "group".into(),
            zone: "right".into(),
            align: None,
        }];
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        GlobalConfig {
            dock_layout: Some(dock.clone()),
            ..GlobalConfig::default()
        }
        .save(&set.global_path())
        .unwrap();

        let migrated = UiConfig {
            dock_layout: dock,
            ..UiConfig::default()
        }
        .pane_layout();
        assert_ne!(migrated, PaneLayoutPersist::default_layout());
        let detail = profile_detail(&state, "Healer").await.unwrap();
        assert_eq!(detail.panes, migrated);

        // With the dock scoped per profile, the global one stays out.
        {
            let mut guard = state.profile_set.lock().await;
            let set = guard.as_mut().unwrap();
            let scope = ScopeConfig {
                dock_layout: Scope::Profile,
                ..*set.scope()
            };
            set.set_scope(scope).unwrap();
        }
        let detail = profile_detail(&state, "Healer").await.unwrap();
        assert_eq!(detail.panes, PaneLayoutPersist::default_layout());
    }
}
