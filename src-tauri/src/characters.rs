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

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tracing::warn;

use crate::commands::{
    broadcast, bump_panes_generation, pane_layout_envelope, panes_generation, persist_profile,
    PaneLayoutEnvelope, SharedState, AUTO_PERSIST_SUPPRESSED, MIGRATION_RELAUNCH_PENDING,
    PERSIST_LOCK, PROFILES_NOT_LOADED,
};
use crate::profile_config::{
    GlobalConfig, PaneLayoutPersist, ProfileConfig, TrackedAffect, UiConfig,
};
use crate::profile_set::{
    display_name, world_name, AutoMatch, LoginClaim, ProfileEntry, ProfileSet, ProfileSetError,
    Scope,
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

/// Load `name`'s file, or for a profile that never saved one what a
/// switch to it loads, see [`ProfileConfig::fresh`].
pub(crate) fn load_profile_file(set: &ProfileSet, name: &str) -> Result<ProfileConfig, String> {
    let path = set.profile_path(name);
    if !path.exists() {
        return Ok(ProfileConfig::fresh());
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

/// Sent after an edit to one profile's detail, active or not, naming
/// it as `{ name }`. Unlike `vosh://tracked-affects-changed` and
/// `vosh://pane-layout-changed` it carries no data, so an edit to an
/// inactive profile can never reach the main window's stores.
pub(crate) const PROFILE_CHANGED_EVENT: &str = "vosh://profile-changed";

#[derive(Clone, Serialize)]
struct ProfileChanged {
    name: String,
}

pub(crate) fn broadcast_profile_changed<R: tauri::Runtime>(app: &AppHandle<R>, name: &str) {
    broadcast(
        app,
        PROFILE_CHANGED_EVENT,
        &ProfileChanged {
            name: name.to_string(),
        },
    );
}

/// Why an inactive profile edit is refused between `migration_apply`
/// and the relaunch that finishes it.
const MIGRATION_PENDING: &str =
    "Restart Vosh to finish the move to loadouts, then change this profile.";

/// Rewrite `name`'s file with `edit` when `name` is inactive, and hand
/// back what `edit` returned. Ok(None) without writing when `name` is
/// the live profile, so the caller edits the live profile instead. Call
/// with [`PERSIST_LOCK`] held.
fn rewrite_inactive<R>(
    set: &ProfileSet,
    name: &str,
    migration_pending: bool,
    edit: impl FnOnce(&ProfileSet, &mut ProfileConfig) -> R,
) -> Result<Option<R>, String> {
    if set.get(name).is_none() {
        return Err(not_found(name));
    }
    if set.active_name() == name {
        return Ok(None);
    }
    // The just archived per profile files must not come back before
    // the relaunch reads the new catalog.
    if migration_pending {
        return Err(MIGRATION_PENDING.to_string());
    }
    let mut config = load_profile_file(set, name)?;
    let out = edit(set, &mut config);
    let path = set.profile_path(name);
    config.save(&path).map_err(|e| {
        warn!(error = %e, path = %path.display(), "inactive profile save failed");
        format!(
            "Vosh could not save the {} profile file.",
            display_name(name)
        )
    })?;
    Ok(Some(out))
}

/// Edit an inactive profile's file. Holds [`PERSIST_LOCK`] and the
/// profile set lock across the read, the edit and the write, so it
/// cannot interleave with a persist, a switch, a rename or a delete.
/// Ok(None) means `name` is live and nothing was written.
pub(crate) async fn edit_inactive_profile<R>(
    state: &SharedState,
    name: &str,
    edit: impl FnOnce(&ProfileSet, &mut ProfileConfig) -> R + Send,
) -> Result<Option<R>, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
    rewrite_inactive(
        set,
        name,
        MIGRATION_RELAUNCH_PENDING.load(Ordering::Acquire),
        edit,
    )
}

/// The active profile's name, for naming a live edit.
pub(crate) async fn active_name(state: &SharedState) -> Option<String> {
    state
        .profile_set
        .lock()
        .await
        .as_ref()
        .map(|set| set.active_name().to_string())
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

/// Put a profile's panes back to the stock map over affects tree,
/// keeping whether its panel shows and how wide it is. The active
/// profile when `profile` is absent.
///
/// The live path bumps the pane generation under the profile lock, so
/// a splitter drag still in flight is refused rather than undoing the
/// reset, persists at once (unless `#profile reset` or `load` left the
/// profile diverged from disk), and broadcasts
/// `vosh://pane-layout-changed`. An inactive profile has its file
/// rewritten and only `vosh://profile-changed` goes out.
#[tauri::command]
pub(crate) async fn pane_layout_reset(
    app: AppHandle,
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<PaneLayoutEnvelope, String> {
    let shared: SharedState = state.inner().clone();
    if let Some(name) = profile.as_deref() {
        if let Some(layout) = reset_inactive_panes(&shared, name).await? {
            broadcast_profile_changed(&app, name);
            return Ok(PaneLayoutEnvelope {
                layout,
                generation: None,
            });
        }
    }
    let envelope = reset_live_panes(&shared).await;
    if !AUTO_PERSIST_SUPPRESSED.load(Ordering::Acquire) {
        persist_profile(&app, &shared).await;
    }
    broadcast(&app, "vosh://pane-layout-changed", &envelope);
    if let Some(active) = active_name(&shared).await {
        broadcast_profile_changed(&app, &active);
    }
    Ok(envelope)
}

/// Reset an inactive profile's saved tree. Ok(None) when `name` is live.
async fn reset_inactive_panes(
    state: &SharedState,
    name: &str,
) -> Result<Option<PaneLayoutPersist>, String> {
    edit_inactive_profile(state, name, |set, config| {
        let mut ui = config.ui.clone();
        apply_global_dock(set, &mut ui);
        let layout = ui.pane_layout().with_default_tree();
        config.ui.panes = Some(layout.clone());
        layout
    })
    .await
}

/// Reset the live profile's tree and hand back its new envelope.
async fn reset_live_panes(state: &SharedState) -> PaneLayoutEnvelope {
    let mut p = state.profile.lock().await;
    let layout = p.ui.pane_layout().with_default_tree();
    p.ui.panes = Some(layout);
    bump_panes_generation();
    pane_layout_envelope(&p)
}

/// Turn the login toggle for `name` on or off for `character`. On takes
/// the character from every other profile on the same world and names
/// them in `released_from`. Never switches the live profile, since the
/// toggle applies at the next login. See [`ProfileSet::set_login`].
#[tauri::command]
pub(crate) async fn profile_set_login(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    character: String,
    on: bool,
) -> Result<LoginClaim, String> {
    let claim = {
        let mut guard = state.profile_set.lock().await;
        let set = guard.as_mut().ok_or(PROFILES_NOT_LOADED)?;
        set.set_login(&name, &character, on)
            .map_err(|e| e.to_string())?
    };
    broadcast(&app, "vosh://profiles-changed", &name);
    Ok(claim)
}

/// Point `name` at a world. Edits only the host and port, so it cannot
/// overwrite a description or characters the other window holds. See
/// [`ProfileSet::set_world`].
#[tauri::command]
pub(crate) async fn profile_set_world(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    host: Option<String>,
    port: Option<u16>,
) -> Result<ProfileEntry, String> {
    let entry = {
        let mut guard = state.profile_set.lock().await;
        let set = guard.as_mut().ok_or(PROFILES_NOT_LOADED)?;
        set.set_world(&name, host, port)
            .map_err(|e| e.to_string())?
    };
    broadcast(&app, "vosh://profiles-changed", &name);
    Ok(entry)
}

/// A profile's settings as TOML, the way `#profile save` writes them:
/// the live profile for the active name, the saved file (or defaults)
/// for any other. Holds [`PERSIST_LOCK`] like [`profile_detail`], so a
/// switch cannot land between deciding which one to read and reading it.
pub(crate) async fn profile_toml(state: &SharedState, name: &str) -> Result<String, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let stored = {
        let guard = state.profile_set.lock().await;
        let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
        if set.get(name).is_none() {
            return Err(not_found(name));
        }
        if set.active_name() == name {
            None
        } else {
            Some(load_profile_file(set, name)?)
        }
    };
    let config = match stored {
        Some(config) => config,
        None => ProfileConfig::from_profile(&*state.profile.lock().await),
    };
    config.to_toml().map_err(|e| {
        warn!(error = %e, profile = name, "profile export failed");
        format!("Vosh could not export the {} profile.", display_name(name))
    })
}

/// Where an export of `name` lands in `dir`: `Ilsabet profile.toml`,
/// or `Ilsabet profile (2).toml` and on when that file is there, so an
/// export never replaces a file you already have.
pub(crate) fn export_path(dir: &Path, name: &str) -> PathBuf {
    let stem = format!("{} profile", display_name(name));
    let first = dir.join(format!("{stem}.toml"));
    if !first.exists() {
        return first;
    }
    let mut n = 2u32;
    loop {
        let path = dir.join(format!("{stem} ({n}).toml"));
        if !path.exists() {
            return path;
        }
        n += 1;
    }
}

/// Where an export went, for the sentence Settings shows.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProfileExport {
    pub path: String,
    pub file_name: String,
}

/// Save a profile's settings as a TOML file in your Downloads folder,
/// active or not, and say where it went. Settings has no save panel,
/// so the file takes a name that never replaces another.
#[tauri::command]
pub(crate) async fn profile_export_file(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<ProfileExport, String> {
    let toml = profile_toml(state.inner(), &name).await?;
    let dir = app
        .path()
        .download_dir()
        .map_err(|_| "Vosh could not find your Downloads folder.".to_string())?;
    let path = export_path(&dir, &name);
    std::fs::write(&path, toml).map_err(|e| {
        warn!(error = %e, path = %path.display(), "profile export write failed");
        format!(
            "Vosh could not save the {} profile in your Downloads folder.",
            display_name(&name)
        )
    })?;
    Ok(ProfileExport {
        file_name: path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.display().to_string(),
    })
}

/// Who is logged in, for the Characters group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SessionIdentity {
    pub host: String,
    pub port: u16,
    /// The character from Char.Status or Char.Name, once the MUD sends
    /// it.
    pub character: Option<String>,
    /// The live profile.
    pub profile: String,
    /// The profile whose login toggle claims `character` on this world.
    /// None when no profile claims it. See [`ProfileSet::claimed_by`].
    pub claimed_by: Option<String>,
}

/// Sent with the new [`SessionIdentity`], or null, after a connect, a
/// disconnect, and the first sight of a character name after login.
/// Settings is its own webview and may open after all of those, so it
/// also reads the current value with `session_identity_get`.
pub(crate) const SESSION_IDENTITY_EVENT: &str = "vosh://session-identity-changed";

/// The session identity, or None while no connection is up.
pub(crate) async fn session_identity(state: &SharedState) -> Option<SessionIdentity> {
    let connection = state.current_connection.lock().ok().and_then(|g| g.clone());
    let (host, port) = connection?;
    let character = state.current_character.lock().ok().and_then(|g| g.clone());
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref()?;
    let claimed_by = character
        .as_deref()
        .and_then(|c| set.claimed_by(&host, port, c));
    Some(SessionIdentity {
        profile: set.active_name().to_string(),
        host,
        port,
        character,
        claimed_by,
    })
}

pub(crate) async fn broadcast_session_identity<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
) {
    let identity = session_identity(state).await;
    broadcast(app, SESSION_IDENTITY_EVENT, &identity);
}

/// Who is logged in: the connection, the character once known, the
/// live profile, and which profile claims that character. Null while
/// no connection is up.
#[tauri::command]
pub(crate) async fn session_identity_get(
    state: State<'_, SharedState>,
) -> Result<Option<SessionIdentity>, String> {
    Ok(session_identity(state.inner()).await)
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
        let leftover = &detail.tracked_affects;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(detail.panes, PaneLayoutPersist::default_layout());
        assert!(!detail.login_on);

        assert!(profile_detail(&state, "Nobody").await.is_err());
    }

    fn set_affects(list: &[&str]) -> impl FnOnce(&ProfileSet, &mut ProfileConfig) + Send {
        let list: Vec<TrackedAffect> = list.iter().map(|n| affect(n)).collect();
        move |_, config| config.ui.tracked_affects = list
    }

    #[tokio::test]
    async fn an_inactive_edit_writes_its_file_and_never_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let mut config = ProfileConfig::default();
        config.ui.theme = "nord".into();
        config.profile_vars.insert("target".into(), "orc".into());
        write_profile(dir.path(), "Healer", &config);

        let written = edit_inactive_profile(&state, "Healer", set_affects(&["Haste", "Fly"]))
            .await
            .unwrap();
        assert!(written.is_some());

        // The file took the list and kept the rest of the profile.
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let saved = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
        assert_eq!(names(&saved.ui.tracked_affects), ["Haste", "Fly"]);
        assert_eq!(saved.ui.theme, "nord");
        assert_eq!(
            saved.profile_vars.get("target").map(String::as_str),
            Some("orc")
        );

        // The live profile and the active file never moved.
        let live = state.profile.lock().await;
        assert_eq!(names(&live.ui.tracked_affects), ["Sanctuary"]);
        assert!(!set.profile_path(DEFAULT_PROFILE_NAME).exists());
    }

    #[tokio::test]
    async fn an_inactive_edit_creates_the_file_a_profile_never_saved() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        edit_inactive_profile(&state, "Test-Prompt", set_affects(&["Fly"]))
            .await
            .unwrap();
        let detail = profile_detail(&state, "Test-Prompt").await.unwrap();
        assert_eq!(names(&detail.tracked_affects), ["Fly"]);
        assert_eq!(detail.panes, PaneLayoutPersist::default_layout());
        // The file starts the way a switch would, with the default design.
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let file = ProfileConfig::load(&set.profile_path("Test-Prompt")).unwrap();
        assert_eq!(file.prompt_config(), vosh_prompt::PromptConfig::fresh());
    }

    #[tokio::test]
    async fn an_edit_to_the_live_profile_is_handed_back_unwritten() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let written = edit_inactive_profile(&state, DEFAULT_PROFILE_NAME, set_affects(&["Fly"]))
            .await
            .unwrap();
        assert!(written.is_none());
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(!set.profile_path(DEFAULT_PROFILE_NAME).exists());
        assert!(
            edit_inactive_profile(&state, "Nobody", set_affects(&["Fly"]))
                .await
                .is_err()
        );
    }

    #[test]
    fn an_inactive_edit_waits_for_the_migration_relaunch() {
        let dir = tempfile::tempdir().unwrap();
        let set = james_like_set(dir.path());
        let err = rewrite_inactive(&set, "Healer", true, set_affects(&["Fly"])).unwrap_err();
        assert_eq!(err, MIGRATION_PENDING);
        assert!(!set.profile_path("Healer").exists());
    }

    /// Group beside chat in a 360 px panel that is hidden.
    fn arranged() -> PaneLayoutPersist {
        let mut layout = PaneLayoutPersist::default_layout();
        layout.panel_open = false;
        layout.panel_width = Some(360);
        layout.root.split = Some("row".into());
        for (child, pane) in layout.root.children.iter_mut().zip(["group", "chat"]) {
            child.id = pane.into();
            child.pane = Some(pane.into());
        }
        layout.sanitize();
        layout
    }

    #[tokio::test]
    async fn resetting_an_inactive_profile_rewrites_only_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        state.profile.lock().await.ui.panes = Some(arranged());
        let mut config = ProfileConfig::default();
        config.ui.panes = Some(arranged());
        config.ui.tracked_affects = vec![affect("Haste")];
        write_profile(dir.path(), "Healer", &config);

        let reset = reset_inactive_panes(&state, "Healer")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reset, arranged().with_default_tree());
        assert!(!reset.panel_open);
        assert_eq!(reset.panel_width, Some(360));

        let detail = profile_detail(&state, "Healer").await.unwrap();
        assert_eq!(detail.panes, reset);
        assert_eq!(names(&detail.tracked_affects), ["Haste"]);
        // The live profile keeps its own arrangement.
        assert_eq!(state.profile.lock().await.ui.panes, Some(arranged()));
    }

    #[tokio::test]
    async fn resetting_the_live_profile_moves_the_generation() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        state.profile.lock().await.ui.panes = Some(arranged());
        let before = panes_generation();

        let envelope = reset_live_panes(&state).await;
        assert_eq!(envelope.layout, arranged().with_default_tree());
        assert!(envelope.generation.unwrap() > before);
        assert_eq!(
            state.profile.lock().await.ui.panes,
            Some(arranged().with_default_tree())
        );
        assert!(reset_inactive_panes(&state, DEFAULT_PROFILE_NAME)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn export_reads_the_live_profile_or_the_named_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let mut config = ProfileConfig::default();
        config.profile_vars.insert("target".into(), "orc".into());
        config.ui.tracked_affects = vec![affect("Haste")];
        write_profile(dir.path(), "Healer", &config);

        let healer = profile_toml(&state, "Healer").await.unwrap();
        let back = ProfileConfig::from_toml(&healer).unwrap();
        assert_eq!(
            back.profile_vars.get("target").map(String::as_str),
            Some("orc")
        );
        assert_eq!(names(&back.ui.tracked_affects), ["Haste"]);

        let live = profile_toml(&state, DEFAULT_PROFILE_NAME).await.unwrap();
        let back = ProfileConfig::from_toml(&live).unwrap();
        assert_eq!(names(&back.ui.tracked_affects), ["Sanctuary"]);

        // Test-Prompt never saved a file, so it exports what a switch to
        // it loads, the defaults with Vosh's default design.
        let blank = profile_toml(&state, "Test-Prompt").await.unwrap();
        let back = ProfileConfig::from_toml(&blank).unwrap();
        let leftover = &back.ui.tracked_affects;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(back.prompt_config(), vosh_prompt::PromptConfig::fresh());

        assert!(profile_toml(&state, "Nobody").await.is_err());
    }

    #[test]
    fn an_export_never_replaces_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let first = export_path(dir.path(), DEFAULT_PROFILE_NAME);
        assert_eq!(first, dir.path().join("Default profile.toml"));
        std::fs::write(&first, "x").unwrap();
        let second = export_path(dir.path(), DEFAULT_PROFILE_NAME);
        assert_eq!(second, dir.path().join("Default profile (2).toml"));
        std::fs::write(&second, "x").unwrap();
        assert_eq!(
            export_path(dir.path(), DEFAULT_PROFILE_NAME),
            dir.path().join("Default profile (3).toml")
        );
        assert_eq!(
            export_path(dir.path(), "Healer"),
            dir.path().join("Healer profile.toml")
        );
    }

    #[tokio::test]
    async fn session_identity_reports_the_login_and_who_claims_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        assert_eq!(session_identity(&state).await, None);

        *state.current_connection.lock().unwrap() =
            Some(("play.theforsakenlands.com".into(), 1848));
        let identity = session_identity(&state).await.unwrap();
        assert_eq!(identity.character, None);
        assert_eq!(identity.claimed_by, None);
        assert_eq!(identity.profile, DEFAULT_PROFILE_NAME);

        *state.current_character.lock().unwrap() = Some("Erelei".into());
        let identity = session_identity(&state).await.unwrap();
        assert_eq!(
            identity,
            SessionIdentity {
                host: "play.theforsakenlands.com".into(),
                port: 1848,
                character: Some("Erelei".into()),
                profile: DEFAULT_PROFILE_NAME.into(),
                claimed_by: Some(DEFAULT_PROFILE_NAME.into()),
            }
        );

        // A character no profile claims keeps the live profile and
        // reports no claim, so Characters can offer a new profile.
        *state.current_character.lock().unwrap() = Some("Ondrevar".into());
        let identity = session_identity(&state).await.unwrap();
        assert_eq!(identity.claimed_by, None);
        assert_eq!(identity.profile, DEFAULT_PROFILE_NAME);
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
