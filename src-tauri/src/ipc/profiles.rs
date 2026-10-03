//! The commands for your profiles. Settings > Characters lists them and
//! creates, renames, copies, deletes and switches them, and Settings >
//! General chooses which settings every character shares. Before it
//! connects, the page asks which profile claims the login and switches
//! to it.
//!
//! `profiles.toml` indexes the profiles, each one saves to
//! `profiles/<name>.toml`, and `AppState.profile_set` holds the live set.

use tauri::{AppHandle, Manager, State};
use tracing::warn;

use crate::app::events::{broadcast, CUSTOM_THEMES_CHANGED, PROFILES_CHANGED};
use crate::app::state::{
    SharedState, AUTO_PERSIST_SUPPRESSED, MIGRATION_RELAUNCH_PENDING, PROFILES_NOT_LOADED,
};
use crate::disk::save::{persist_profile_locked, persist_state, PERSIST_LOCK};
use crate::profile::switch::apply_profile_switch;
use crate::profile_config::{hand_out_shared, share_custom_themes, GlobalConfig, HeldCustomThemes};

#[derive(serde::Serialize)]
pub(crate) struct ProfilesListPayload {
    pub active: String,
    pub profiles: Vec<crate::profile_set::ProfileEntry>,
}

#[tauri::command]
pub(crate) async fn profiles_list(
    state: State<'_, SharedState>,
) -> Result<ProfilesListPayload, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    Ok(ProfilesListPayload {
        active: set.active_name().to_string(),
        profiles: set.list().to_vec(),
    })
}

/// Write the live profile to its file before a copy of `source` reads
/// that file, when `source` is the live profile. Call with
/// [`PERSIST_LOCK`] held across this and the copy, so the copy reads
/// what the flush wrote and no persist rewrites the source mid copy.
async fn flush_before_copy(shared: &SharedState, app_data: Option<&std::path::Path>, source: &str) {
    let copying_live = shared
        .profile_set
        .lock()
        .await
        .as_ref()
        .is_some_and(|set| set.active_name() == source);
    // The live profile can run two seconds ahead of its file. After
    // `#profile reset` or `load` it is deliberately diverged, and the
    // copy takes the file as it stands.
    if copying_live && !AUTO_PERSIST_SUPPRESSED.load(std::sync::atomic::Ordering::Acquire) {
        persist_state(shared, app_data).await;
    }
}

/// Create a profile with `auto_match` as its login claim, starting as a
/// copy of `copy_from` when given. Returns the new entry and does not
/// switch. The claim takes nothing from other profiles, so a caller
/// that wants the character for itself follows with
/// `profile_set_login`.
#[tauri::command]
pub(crate) async fn profile_create(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    copy_from: Option<String>,
    auto_match: Option<crate::profile_set::AutoMatch>,
) -> Result<crate::profile_set::ProfileEntry, String> {
    let app_data = app.path().app_data_dir().ok();
    let entry = create_profile(
        state.inner(),
        app_data.as_deref(),
        &name,
        copy_from.as_deref(),
        auto_match,
        &MIGRATION_RELAUNCH_PENDING,
    )
    .await?;
    broadcast(&app, PROFILES_CHANGED, &entry.name);
    Ok(entry)
}

/// The body of [`profile_create`] over the app data folder `app_data`,
/// with `relaunch_pending` in place of [`MIGRATION_RELAUNCH_PENDING`], so
/// a test can run it after the wizard.
pub(crate) async fn create_profile(
    state: &SharedState,
    app_data: Option<&std::path::Path>,
    name: &str,
    copy_from: Option<&str>,
    auto_match: Option<crate::profile_set::AutoMatch>,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) -> Result<crate::profile_set::ProfileEntry, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    if let Some(source) = copy_from {
        // Read under the lock, which the wizard holds until it sets the
        // flag.
        if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
            return Err(COPY_MIGRATION_PENDING.into());
        }
        flush_before_copy(state, app_data, source).await;
    }
    let mut guard = state.profile_set.lock().await;
    let Some(set) = guard.as_mut() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    set.create_from(name, copy_from, auto_match)
        .map_err(|e| e.to_string())
}

/// Why a profile cannot be renamed between `migration_apply` and the
/// relaunch that finishes it, or while launch could not finish a wizard
/// run. The next launch writes each profile file the run names under the
/// name it had, and the renamed file would keep what the move took out.
const RENAME_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then rename the profile.";

/// Why a profile cannot be copied in the same window. The copy would take
/// a file the next launch has yet to finish, or the live profile a copy of
/// it saves first, which still holds the items the move took out.
const COPY_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then copy the profile.";

#[tauri::command]
pub(crate) async fn profile_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    {
        let _persist_guard = PERSIST_LOCK.lock().await;
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        set.delete(&name).map_err(|e| e.to_string())?;
    }
    broadcast(&app, PROFILES_CHANGED, &name);
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_rename(
    app: AppHandle,
    state: State<'_, SharedState>,
    old: String,
    new: String,
) -> Result<(), String> {
    rename_profile(state.inner(), &old, &new, &MIGRATION_RELAUNCH_PENDING).await?;
    broadcast(&app, PROFILES_CHANGED, &new);
    Ok(())
}

/// The body of [`profile_rename`], with `relaunch_pending` in place of
/// [`MIGRATION_RELAUNCH_PENDING`], so a test can run it after the wizard.
pub(crate) async fn rename_profile(
    state: &SharedState,
    old: &str,
    new: &str,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
        return Err(RENAME_MIGRATION_PENDING.into());
    }
    let live = {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        let renames_live = set.active_name() == old;
        set.rename(old, new).map_err(|e| e.to_string())?;
        if renames_live {
            state.note_active_profile(set.active_name());
        }
        renames_live.then(|| crate::profile_set::display_name(set.active_name()))
    };
    // The custom prompt draws the live profile's new name.
    if let Some(name) = live {
        state.profile.lock().await.display_name = Some(name);
    }
    Ok(())
}

/// Copy `source` under a new name without its login claim. Duplicating
/// the live profile writes it first, so the copy holds your latest
/// changes.
#[tauri::command]
pub(crate) async fn profile_duplicate(
    app: AppHandle,
    state: State<'_, SharedState>,
    source: String,
    new: String,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir().ok();
    duplicate_profile(
        state.inner(),
        app_data.as_deref(),
        &source,
        &new,
        &MIGRATION_RELAUNCH_PENDING,
    )
    .await?;
    broadcast(&app, PROFILES_CHANGED, &new);
    Ok(())
}

/// The body of [`profile_duplicate`] over the app data folder `app_data`,
/// with `relaunch_pending` in place of [`MIGRATION_RELAUNCH_PENDING`], so
/// a test can run it after the wizard.
pub(crate) async fn duplicate_profile(
    state: &SharedState,
    app_data: Option<&std::path::Path>,
    source: &str,
    new: &str,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
        return Err(COPY_MIGRATION_PENDING.into());
    }
    flush_before_copy(state, app_data, source).await;
    let mut guard = state.profile_set.lock().await;
    let Some(set) = guard.as_mut() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    set.duplicate(source, new).map_err(|e| e.to_string())
}

/// Read the per-category scope map. Frontend uses this to render
/// the toggle row in the Profiles tab.
#[tauri::command]
pub(crate) async fn profile_get_scope(
    state: State<'_, SharedState>,
) -> Result<crate::profile_set::ScopeConfig, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    Ok(*set.scope())
}

/// Update the per-category scope map. After the index is updated,
/// persist the active profile so values move to the correct file
/// (a category flipped Global -> Profile lands in the per-profile
/// file on next save; Profile -> Global lands in global.toml).
///
/// Turning the theme category global also folds the custom themes the
/// other profile files hold into the shared list and clears them from
/// those files, or a switch to one of those profiles would lose them.
/// When the list grows, `vosh://custom-themes-changed` carries it to
/// every window.
///
/// Turning a category per profile first copies the shared values into
/// every other profile file that holds none of its own, since the save
/// drops them from global.toml.
#[tauri::command]
pub(crate) async fn profile_set_scope(
    app: AppHandle,
    state: State<'_, SharedState>,
    scope: crate::profile_set::ScopeConfig,
) -> Result<(), String> {
    // Held from the scope change through the persist, so no other
    // profile file write lands between the moves and the save.
    let persist_guard = PERSIST_LOCK.lock().await;
    let shared: SharedState = state.inner().clone();
    let gained = change_scope_locked(&shared, scope).await?;
    persist_profile_locked(&app, &shared).await;
    drop(persist_guard);
    if let Some(list) = gained {
        broadcast(&app, CUSTOM_THEMES_CHANGED, &list);
    }
    broadcast(&app, PROFILES_CHANGED, &"scope");
    Ok(())
}

/// Why a category cannot stop being shared between `migration_apply`
/// and the relaunch that finishes it. The shared values would have to
/// reach profile files that nothing may write in that window.
const SCOPE_MIGRATION_PENDING: &str =
    "Restart Vosh to finish the move to loadouts, then turn this off.";

/// Why the shared categories cannot change while Vosh holds a file it
/// could not read at launch. The live profile holds the defaults where
/// that file's settings belong, and a change would hand those defaults
/// to the other profiles or share them with every character.
fn scope_refusal_for_unread(set: &crate::profile_set::ProfileSet) -> Option<String> {
    use crate::profile_config::is_unread;
    if is_unread(&set.global_path()) {
        return Some(
            "Vosh could not read global.toml, so it will not change which settings every \
             character shares. Fix the file and restart Vosh."
                .to_string(),
        );
    }
    if is_unread(&set.active_path()) {
        return Some(format!(
            "Vosh could not read the {} profile file, so it will not change which settings \
             every character shares. Fix the file or switch to another profile.",
            crate::profile_set::display_name(set.active_name())
        ));
    }
    None
}

/// The body of [`profile_set_scope`] up to its save. Call with
/// [`PERSIST_LOCK`] held. Returns the live custom themes when turning the
/// theme category global added to them.
pub(crate) async fn change_scope_locked(
    state: &SharedState,
    scope: crate::profile_set::ScopeConfig,
) -> Result<Option<Vec<crate::profile_config::CustomTheme>>, String> {
    use crate::profile_set::Scope;
    let migration_pending = MIGRATION_RELAUNCH_PENDING.load(std::sync::atomic::Ordering::Acquire);
    let before = {
        let guard = state.profile_set.lock().await;
        let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
        if let Some(refusal) = scope_refusal_for_unread(set) {
            return Err(refusal);
        }
        *set.scope()
    };
    // Every other profile file holds the defaults for a shared category,
    // and the save below drops the category from global.toml, so each
    // file takes the shared values first or that profile opens with the
    // defaults. The live profile holds the shared values.
    if let Some(stopped) = before.stopped_sharing(&scope) {
        if migration_pending {
            return Err(SCOPE_MIGRATION_PENDING.into());
        }
        let values = {
            let p = state.profile.lock().await;
            GlobalConfig::from_profile(&p, &stopped)
        };
        let guard = state.profile_set.lock().await;
        let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
        hand_out_shared(set, &values)?;
    }
    let (held, global_path) = {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        let theme_was_global = matches!(set.scope().theme, Scope::Global);
        set.set_scope(scope).map_err(|e| e.to_string())?;
        // Nothing may write profile files while a migration relaunch is
        // pending. The next launch moves the themes instead.
        let theme_turned_global =
            !theme_was_global && matches!(scope.theme, Scope::Global) && !migration_pending;
        let held =
            theme_turned_global.then(|| HeldCustomThemes::find(set, Some(set.active_name())));
        (held, set.global_path())
    };
    let mut gained = None;
    if let Some(held) = held {
        let mut p = state.profile.lock().await;
        match share_custom_themes(held, &scope, &global_path, &mut p) {
            Ok(true) => gained = Some(p.ui.custom_themes.clone()),
            Ok(false) => {}
            Err(e) => warn!(error = %e, "custom themes stayed in their profile files"),
        }
    }
    Ok(gained)
}

/// Given a connect target, find the first profile whose `auto_match`
/// claims it. Returns the profile name or null. The frontend calls
/// this right before invoking `session_connect` so a matching
/// profile can be switched to ahead of the connection.
#[tauri::command]
pub(crate) async fn profile_resolve_match(
    state: State<'_, SharedState>,
    host: String,
    port: u16,
    character: Option<String>,
) -> Result<Option<String>, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Ok(None);
    };
    Ok(set.resolve_match(&host, port, character.as_deref()))
}

#[tauri::command]
pub(crate) async fn profile_switch(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    let shared: SharedState = state.inner().clone();
    apply_profile_switch(&app, &shared, &name).await
}

#[cfg(test)]
mod tests {
    mod scope {
        use std::sync::Arc;

        use super::super::{change_scope_locked, SharedState, PERSIST_LOCK};
        use crate::app::state::AppState;
        use crate::profile::Profile;
        use crate::profile_config::{
            strip_global_fields, CustomTheme, GlobalConfig, ProfileConfig, TrackedAffect, UiConfig,
        };
        use crate::profile_set::tests::james_like_set;
        use crate::profile_set::{ProfileSet, Scope, ScopeConfig, DEFAULT_PROFILE_NAME};

        fn theme(id: &str, background: &str) -> CustomTheme {
            CustomTheme {
                id: id.into(),
                label: id.into(),
                xterm: [("background".to_string(), background.to_string())]
                    .into_iter()
                    .collect(),
                ..CustomTheme::default()
            }
        }

        fn ids(themes: &[CustomTheme]) -> Vec<&str> {
            themes.iter().map(|t| t.id.as_str()).collect()
        }

        /// The live profile with every shared setting off its default.
        fn shared_profile() -> Profile {
            let mut profile = Profile::default();
            profile.ui.theme = "night-ink".into();
            profile.ui.follow_system_appearance = true;
            profile.ui.light_theme = "classic-vivid".into();
            profile.ui.dark_theme = "night-ink".into();
            profile.ui.custom_themes = vec![theme("night-ink", "#000000")];
            profile.ui.font_family = "Iosevka".into();
            profile.ui.font_size = 16;
            profile.ui.terminal_line_height = "loose".into();
            profile.ui.keep_last_command = true;
            profile.ui.auto_update = true;
            profile
        }

        /// Mirror `persist_profile` for the active profile.
        fn persist(set: &ProfileSet, profile: &Profile) {
            let mut snapshot = ProfileConfig::from_profile(profile);
            strip_global_fields(&mut snapshot, set.scope());
            snapshot.save(&set.active_path()).unwrap();
            GlobalConfig::from_profile(profile, set.scope())
                .save(&set.global_path())
                .unwrap();
        }

        /// Mirror a switch. The active profile file loads first, then the
        /// shared part of global.toml over it.
        fn load(set: &ProfileSet) -> Profile {
            let mut profile = Profile::default();
            let path = set.active_path();
            if path.exists() {
                ProfileConfig::load(&path).unwrap().apply_to(&mut profile);
            }
            if let Some(global) =
                GlobalConfig::load_shared(&set.global_path(), set.scope()).unwrap()
            {
                global.apply_to(&mut profile);
            }
            profile
        }

        fn file(set: &ProfileSet, name: &str) -> ProfileConfig {
            ProfileConfig::load(&set.profile_path(name)).unwrap()
        }

        fn per_profile() -> ScopeConfig {
            ScopeConfig {
                theme: Scope::Profile,
                font: Scope::Profile,
                keep_last_command: Scope::Profile,
                auto_update: Scope::Profile,
                ..ScopeConfig::default()
            }
        }

        /// Default is live and shares everything. Healer saved its file
        /// while everything was shared, so it holds the defaults. Test-Prompt
        /// saved its own theme, font, and custom theme before they were
        /// shared, under the id the live custom theme holds.
        async fn three_profiles(dir: &std::path::Path) -> SharedState {
            let set = james_like_set(dir);
            let live = shared_profile();
            persist(&set, &live);

            let mut healer = ProfileConfig::default();
            healer.ui.tracked_affects = vec![TrackedAffect {
                name: "Fly".into(),
                label: None,
            }];
            healer.save(&set.profile_path("Healer")).unwrap();

            let mut prompt = ProfileConfig::default();
            prompt.ui.theme = "night-ink".into();
            prompt.ui.custom_themes = vec![theme("night-ink", "#ffffff")];
            prompt.ui.font_size = 13;
            prompt.save(&set.profile_path("Test-Prompt")).unwrap();

            let state: SharedState = Arc::new(AppState::default());
            *state.profile.lock().await = live;
            *state.profile_set.lock().await = Some(set);
            state
        }

        /// Mirror `profile_set_scope`. The persist that follows the change
        /// runs under the same lock.
        async fn set_scope(state: &SharedState, scope: ScopeConfig) {
            let _persist_guard = PERSIST_LOCK.lock().await;
            change_scope_locked(state, scope).await.unwrap();
            let live = state.profile.lock().await;
            let guard = state.profile_set.lock().await;
            persist(guard.as_ref().unwrap(), &live);
        }

        #[tokio::test]
        async fn turning_sharing_off_hands_the_shared_settings_to_every_profile() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            set_scope(&state, per_profile()).await;

            let mut guard = state.profile_set.lock().await;
            let set = guard.as_mut().unwrap();
            // global.toml no longer holds the categories you turned off.
            let global = GlobalConfig::load(&set.global_path()).unwrap();
            assert!(global.theme.is_none());
            assert!(global.custom_themes.is_none());
            assert!(global.font_size.is_none());
            assert!(global.keep_last_command.is_none());
            assert!(global.auto_update.is_none());

            // Healer held none of its own, so it takes every shared value
            // and keeps what it owns.
            let healer = file(set, "Healer").ui;
            assert_eq!(healer.theme, "night-ink");
            assert!(healer.follow_system_appearance);
            assert_eq!(healer.light_theme, "classic-vivid");
            assert_eq!(healer.dark_theme, "night-ink");
            assert_eq!(ids(&healer.custom_themes), ["night-ink"]);
            assert_eq!(healer.font_family, "Iosevka");
            assert_eq!(healer.font_size, 16);
            assert_eq!(healer.terminal_line_height, "loose");
            assert!(healer.keep_last_command);
            assert!(healer.auto_update);
            assert_eq!(healer.tracked_affects.len(), 1);

            // Test-Prompt keeps its own theme and font, and its own custom
            // theme moves to a fresh id beside the shared one.
            let prompt = file(set, "Test-Prompt").ui;
            assert_eq!(ids(&prompt.custom_themes), ["night-ink", "night-ink-2"]);
            assert_eq!(prompt.custom_themes[1], {
                let mut own = theme("night-ink-2", "#ffffff");
                own.label = "night-ink (Test-Prompt)".into();
                own
            });
            assert_eq!(prompt.theme, "night-ink-2");
            assert_eq!(prompt.font_size, 13);
            assert_eq!(prompt.font_family, UiConfig::default().font_family);
            assert!(prompt.keep_last_command);

            // A switch to Healer shows what it showed while shared.
            set.switch("Healer").unwrap();
            let healer = load(set);
            assert_eq!(healer.ui.theme, "night-ink");
            assert_eq!(healer.ui.font_size, 16);
            assert!(healer.ui.keep_last_command);
            // The live profile kept its values in its own file.
            set.switch(DEFAULT_PROFILE_NAME).unwrap();
            let live = load(set);
            assert_eq!(live.ui.theme, "night-ink");
            assert_eq!(live.ui.font_size, 16);
        }

        #[tokio::test]
        async fn sharing_again_after_turning_it_off_keeps_every_theme() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            set_scope(&state, per_profile()).await;
            set_scope(&state, ScopeConfig::default()).await;

            let live = state.profile.lock().await;
            assert_eq!(ids(&live.ui.custom_themes), ["night-ink", "night-ink-2"]);
            let guard = state.profile_set.lock().await;
            let set = guard.as_ref().unwrap();
            let global = GlobalConfig::load(&set.global_path()).unwrap();
            assert_eq!(
                ids(&global.custom_themes.unwrap()),
                ["night-ink", "night-ink-2"]
            );
            // Test-Prompt still points at its own theme for the next time
            // you turn sharing off.
            assert_eq!(file(set, "Test-Prompt").ui.theme, "night-ink-2");
        }

        #[tokio::test]
        async fn a_profile_that_never_saved_takes_the_shared_settings() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            state
                .profile_set
                .lock()
                .await
                .as_mut()
                .unwrap()
                .create("Bard")
                .unwrap();
            set_scope(
                &state,
                ScopeConfig {
                    font: Scope::Profile,
                    ..ScopeConfig::default()
                },
            )
            .await;

            let guard = state.profile_set.lock().await;
            let set = guard.as_ref().unwrap();
            let bard = file(set, "Bard").ui;
            assert_eq!(bard.font_size, 16);
            assert_eq!(bard.terminal_line_height, "loose");
            // The theme is still shared, so the file keeps the defaults.
            let leftover = &bard.custom_themes;
            assert!(leftover.is_empty(), "{leftover:?}");
        }

        #[tokio::test]
        async fn a_file_vosh_cannot_read_keeps_the_settings_shared() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            let (healer_path, global_path) = {
                let guard = state.profile_set.lock().await;
                let set = guard.as_ref().unwrap();
                std::fs::write(set.profile_path("Test-Prompt"), "theme = [").unwrap();
                (set.profile_path("Healer"), set.global_path())
            };
            let healer_before = std::fs::read_to_string(&healer_path).unwrap();
            let global_before = std::fs::read_to_string(&global_path).unwrap();

            let refused = {
                let _persist_guard = PERSIST_LOCK.lock().await;
                change_scope_locked(&state, per_profile()).await
            };

            let message = refused.unwrap_err();
            assert_eq!(
                message,
                "Vosh could not read the Test-Prompt profile file, so these settings stay the same for every character."
            );
            let guard = state.profile_set.lock().await;
            assert_eq!(guard.as_ref().unwrap().scope().theme, Scope::Global);
            assert_eq!(
                std::fs::read_to_string(&healer_path).unwrap(),
                healer_before
            );
            assert_eq!(
                std::fs::read_to_string(&global_path).unwrap(),
                global_before
            );
        }

        #[test]
        fn stopped_sharing_names_only_the_categories_turned_off() {
            let shared = ScopeConfig::default();
            assert!(shared.stopped_sharing(&shared).is_none());
            let stopped = shared.stopped_sharing(&per_profile()).unwrap();
            assert_eq!(stopped.theme, Scope::Global);
            assert_eq!(stopped.font, Scope::Global);
            assert_eq!(stopped.dock_layout, Scope::Profile);
            assert!(per_profile().stopped_sharing(&shared).is_none());
        }

        #[tokio::test]
        async fn turning_sharing_on_writes_no_other_profile_file() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            let healer_before = {
                let guard = state.profile_set.lock().await;
                std::fs::read_to_string(guard.as_ref().unwrap().profile_path("Healer")).unwrap()
            };
            set_scope(&state, ScopeConfig::default()).await;
            let guard = state.profile_set.lock().await;
            let healer_after =
                std::fs::read_to_string(guard.as_ref().unwrap().profile_path("Healer")).unwrap();
            assert_eq!(healer_after, healer_before);
        }
    }
}
