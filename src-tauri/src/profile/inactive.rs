//! Settings > Characters. A character is a name in a profile's
//! `auto_match`, and the Characters group edits any profile in place,
//! open or not, without switching a session.
//!
//! A profile a session plays lives in memory, so reads and writes for it
//! go through its open copy the way every other command does. Any other
//! profile lives only in `profiles/<name>.toml`, so reads load that file
//! and writes rewrite it under [`PERSIST_LOCK`]. An edit to a profile the
//! selected session does not play announces itself as
//! `vosh://profile-changed` rather than the events that carry the
//! selected session's panes and tracked affects to the main window.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use tauri::AppHandle;
use tracing::warn;

use crate::app::events::{broadcast, pane_layout_envelope, PaneLayoutEnvelope, PROFILE_CHANGED};
use crate::app::state::SharedState;
use crate::disk::save::PERSIST_LOCK;
use crate::profile::export::VoshExport;
use crate::profile::file::ProfileConfig;
use crate::profile::live::Profile;
use crate::profile::login_match::AutoMatch;
use crate::profile::open::OpenProfile;
use crate::profile::panes::PaneLayoutPersist;
use crate::profile::set::{display_name, ProfileEntry, ProfileSet, ProfileSetError};
use crate::profile::shared::{GlobalConfig, Scope};
use crate::profile::ui::{TrackedAffect, UiConfig};
use crate::profile::worlds::world_name;

/// One profile as the Characters group shows it.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProfileDetail {
    pub name: String,
    /// `Default` for the reserved `default` profile, else the name.
    pub display_name: String,
    /// Whether the selected session plays it.
    pub active: bool,
    pub auto_match: Option<AutoMatch>,
    /// The display name of the profile's world, when it has one.
    pub world_name: Option<String>,
    pub tracked_affects: Vec<TrackedAffect>,
    pub panes: PaneLayoutPersist,
    /// The live pane generation for the active profile. None for any
    /// other, whose tree no pane layout write can target.
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

#[derive(Clone, Serialize)]
struct ProfileChanged {
    name: String,
}

pub(crate) fn broadcast_profile_changed<R: tauri::Runtime>(app: &AppHandle<R>, name: &str) {
    broadcast(
        app,
        PROFILE_CHANGED,
        &ProfileChanged {
            name: name.to_string(),
        },
    );
}

/// Why an inactive profile edit is refused between `migration_apply`
/// and the relaunch that finishes it.
const MIGRATION_PENDING: &str =
    "Restart Vosh to finish the move to loadouts, then change this profile.";

/// Where a profile named in a command lives: only in its file, which
/// gave or took what the command asked, or open in memory, where the
/// caller reads or edits it instead.
pub(crate) enum Stored<T> {
    File(T),
    Open(Arc<OpenProfile>),
}

/// Rewrite `name`'s file with `edit`, and hand back what `edit` returned.
/// Call with [`PERSIST_LOCK`] held, for a profile no session plays.
fn rewrite_inactive<R>(
    set: &ProfileSet,
    name: &str,
    migration_pending: bool,
    edit: impl FnOnce(&ProfileSet, &mut ProfileConfig) -> R,
) -> Result<R, String> {
    if set.get(name).is_none() {
        return Err(not_found(name));
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
    Ok(out)
}

/// Edit `name`'s file when no session plays it, or hand back its open
/// copy for the caller to edit. Holds [`PERSIST_LOCK`] and the profile
/// set lock across the read, the edit and the write, so it cannot
/// interleave with a persist, a switch, a rename or a delete.
pub(crate) async fn edit_inactive_profile<R>(
    state: &SharedState,
    name: &str,
    edit: impl FnOnce(&ProfileSet, &mut ProfileConfig) -> R + Send,
) -> Result<Stored<R>, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    edit_inactive_locked(state, name, edit).await
}

/// [`edit_inactive_profile`] for a step that holds [`PERSIST_LOCK`]
/// already, such as an import.
pub(crate) async fn edit_inactive_locked<R>(
    state: &SharedState,
    name: &str,
    edit: impl FnOnce(&ProfileSet, &mut ProfileConfig) -> R + Send,
) -> Result<Stored<R>, String> {
    if let Some(open) = state.open_profile(name) {
        return Ok(Stored::Open(open));
    }
    let set = state.loaded_profile_set().await?;
    let migration_pending = state.relaunch_pending.load(Ordering::Acquire);
    rewrite_inactive(&set, name, migration_pending, edit).map(Stored::File)
}

/// Body of [`profile_detail_get`]. Holds [`PERSIST_LOCK`] so a switch
/// cannot land between deciding whether a session plays `name` and
/// reading it. The profile set lock is let go before an open profile is
/// locked, the order the persist takes them in.
///
/// [`profile_detail_get`]: crate::ipc::characters::profile_detail_get
pub(crate) async fn profile_detail(
    state: &SharedState,
    name: &str,
) -> Result<ProfileDetail, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let open = state.open_profile(name);
    let (entry, active, login_on, stored) = {
        let set = state.loaded_profile_set().await?;
        let entry = set.get(name).cloned().ok_or_else(|| not_found(name))?;
        let stored = match open {
            Some(open) => Stored::Open(open),
            None => Stored::File(stored_ui(&set, name)?),
        };
        (entry, set.active_name() == name, set.login_on(name), stored)
    };
    Ok(match stored {
        Stored::File(ui) => ProfileDetail::new(entry, false, login_on, &ui, None),
        Stored::Open(open) => {
            let p = open.lock().await;
            let generation = active.then(|| state.panes_generation());
            ProfileDetail::new(entry, active, login_on, &p.ui, generation)
        }
    })
}

/// Reset the saved tree of a profile no session plays, or hand back the
/// open copy of one a session plays.
pub(crate) async fn reset_inactive_panes(
    state: &SharedState,
    name: &str,
) -> Result<Stored<PaneLayoutPersist>, String> {
    edit_inactive_profile(state, name, |set, config| {
        let mut ui = config.ui.clone();
        apply_global_dock(set, &mut ui);
        let layout = ui.pane_layout().with_default_tree();
        config.ui.panes = Some(layout.clone());
        layout
    })
    .await
}

/// Reset the tree of `p`, a profile a session plays, and hand back its
/// new envelope. The tree the selected session's profile shows takes a
/// new generation, so a splitter drag still in flight is refused. Any
/// other has no generation, since no pane layout write can target it.
pub(crate) fn reset_open_panes(
    state: &SharedState,
    p: &mut Profile,
    shown: bool,
) -> PaneLayoutEnvelope {
    let layout = p.ui.pane_layout().with_default_tree();
    p.ui.panes = Some(layout.clone());
    if !shown {
        return PaneLayoutEnvelope {
            layout,
            generation: None,
        };
    }
    state.bump_panes_generation();
    pane_layout_envelope(state, p)
}

/// A profile as Export to Downloads writes it. First its settings as
/// TOML, the way `#profile save` writes them: the open copy of a profile
/// a session plays, the saved file (or defaults) for any other, which in
/// loadout mode takes the catalog's presets and your edits to them. Then
/// the `[vosh_export]` table with its world and the characters in `ticked`
/// that it claims. Holds [`PERSIST_LOCK`] like [`profile_detail`], so a
/// switch or a rename cannot land between reading the claim, deciding
/// which copy to read and reading it.
pub(crate) async fn export_text(
    state: &SharedState,
    name: &str,
    ticked: &[String],
) -> Result<String, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let open = state.open_profile(name);
    let (table, stored) = {
        let set = state.loaded_profile_set().await?;
        let entry = set.get(name).ok_or_else(|| not_found(name))?;
        let table = VoshExport::new(entry.auto_match.as_ref(), ticked);
        let stored = match open {
            Some(open) => Stored::Open(open),
            None => Stored::File(load_profile_file(&set, name)?),
        };
        (table, stored)
    };
    let config = match stored {
        Stored::File(mut config) => {
            // In loadout mode the catalog holds the presets you play.
            if let Some(catalog) = &*state.global_catalog.lock().await {
                catalog.lay_presets_over_file(&mut config);
            }
            config
        }
        Stored::Open(open) => ProfileConfig::from_profile(&*open.lock().await),
    };
    let failed = |e: &dyn std::fmt::Display| {
        warn!(error = %e, profile = name, "profile export failed");
        format!("Vosh could not export the {} profile.", display_name(name))
    };
    let text = config.to_toml().map_err(|e| failed(&e))?;
    table.write(&text).map_err(|e| failed(&e))
}

/// Where an export of `name` lands in `dir`: `Ilsabet profile.toml`,
/// or `Ilsabet profile (2).toml` and on when that file is there, so an
/// export never replaces a file you already have.
pub(crate) fn export_path(dir: &Path, name: &str) -> PathBuf {
    crate::disk::paths::export_path(dir, &format!("{} profile", display_name(name)), "toml")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::app::state::AppState;
    use crate::profile::panes::DockEntryPersist;
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    use crate::profile::shared::ScopeConfig;
    use crate::profile::tests::{claim, james_like_set, put_claim};

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
        state.selected_profile().await.ui.tracked_affects = vec![affect("Sanctuary")];
        state.set_profiles(james_like_set(dir)).await;
        state
    }

    /// The text of the active profile file in `dir`, which an edit to
    /// another profile never moves.
    fn active_text(dir: &std::path::Path) -> String {
        let set = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
        std::fs::read_to_string(set.profile_path(DEFAULT_PROFILE_NAME)).unwrap()
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

        // Test-Prompt never saved a file and loses Ilsabet to default.
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
        let active = active_text(dir.path());
        let mut config = ProfileConfig::default();
        config.ui.theme = "nord".into();
        config.profile_vars.insert("target".into(), "orc".into());
        write_profile(dir.path(), "Healer", &config);

        let written = edit_inactive_profile(&state, "Healer", set_affects(&["Haste", "Fly"]))
            .await
            .unwrap();
        assert!(matches!(written, Stored::File(())));

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
        let live = state.selected_profile().await;
        assert_eq!(names(&live.ui.tracked_affects), ["Sanctuary"]);
        assert_eq!(active_text(dir.path()), active);
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
        // The file starts the way a switch would, following the game with
        // drawing off.
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let file = ProfileConfig::load(&set.profile_path("Test-Prompt")).unwrap();
        assert_eq!(file.prompt_config(), vosh_prompt::PromptConfig::fresh());
    }

    #[tokio::test]
    async fn an_edit_to_the_live_profile_is_handed_back_unwritten() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let active = active_text(dir.path());
        let written = edit_inactive_profile(&state, DEFAULT_PROFILE_NAME, set_affects(&["Fly"]))
            .await
            .unwrap();
        assert!(matches!(written, Stored::Open(_)));
        assert_eq!(active_text(dir.path()), active);
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
        state.selected_profile().await.ui.panes = Some(arranged());
        let mut config = ProfileConfig::default();
        config.ui.panes = Some(arranged());
        config.ui.tracked_affects = vec![affect("Haste")];
        write_profile(dir.path(), "Healer", &config);

        let Stored::File(reset) = reset_inactive_panes(&state, "Healer").await.unwrap() else {
            panic!("Healer's file takes the reset");
        };
        assert_eq!(reset, arranged().with_default_tree());
        assert!(!reset.panel_open);
        assert_eq!(reset.panel_width, Some(360));

        let detail = profile_detail(&state, "Healer").await.unwrap();
        assert_eq!(detail.panes, reset);
        assert_eq!(names(&detail.tracked_affects), ["Haste"]);
        // The live profile keeps its own arrangement.
        assert_eq!(state.selected_profile().await.ui.panes, Some(arranged()));
    }

    #[tokio::test]
    async fn resetting_the_live_profile_moves_the_generation() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        state.selected_profile().await.ui.panes = Some(arranged());
        let before = state.panes_generation();

        let envelope = reset_open_panes(&state, &mut *state.selected_profile().await, true);
        assert_eq!(envelope.layout, arranged().with_default_tree());
        assert!(envelope.generation.unwrap() > before);
        assert_eq!(
            state.selected_profile().await.ui.panes,
            Some(arranged().with_default_tree())
        );
        assert!(matches!(
            reset_inactive_panes(&state, DEFAULT_PROFILE_NAME)
                .await
                .unwrap(),
            Stored::Open(_)
        ));
    }

    #[tokio::test]
    async fn export_reads_the_live_profile_or_the_named_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let mut config = ProfileConfig::default();
        config.profile_vars.insert("target".into(), "orc".into());
        config.ui.tracked_affects = vec![affect("Haste")];
        write_profile(dir.path(), "Healer", &config);

        let healer = export_text(&state, "Healer", &[]).await.unwrap();
        let back = ProfileConfig::from_toml(&healer).unwrap();
        assert_eq!(
            back.profile_vars.get("target").map(String::as_str),
            Some("orc")
        );
        assert_eq!(names(&back.ui.tracked_affects), ["Haste"]);

        let live = export_text(&state, DEFAULT_PROFILE_NAME, &[])
            .await
            .unwrap();
        let back = ProfileConfig::from_toml(&live).unwrap();
        assert_eq!(names(&back.ui.tracked_affects), ["Sanctuary"]);

        // Test-Prompt never saved a file, so it exports what a switch to
        // it loads, the defaults, following the game with drawing off.
        let blank = export_text(&state, "Test-Prompt", &[]).await.unwrap();
        let back = ProfileConfig::from_toml(&blank).unwrap();
        let leftover = &back.ui.tracked_affects;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(back.prompt_config(), vosh_prompt::PromptConfig::fresh());

        assert!(export_text(&state, "Nobody", &[]).await.is_err());
    }

    /// An export carries the list of presets that are on, your edits to
    /// them and the preset triggers as installed (Presets Q11).
    #[tokio::test]
    async fn an_export_carries_the_preset_edits_in_per_profile_mode() {
        use crate::loadouts::preset_edits::lilac_line;
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let mut config = ProfileConfig::default();
        config.ui.enabled_presets = vec!["disarm_buff_fade".into()];
        config.preset_edits = lilac_line();
        write_profile(dir.path(), "Healer", &config);
        {
            let mut p = state.selected_profile().await;
            p.ui.enabled_presets = vec!["disarm_buff_fade".into()];
            p.preset_edits = lilac_line();
        }
        for name in ["Healer", DEFAULT_PROFILE_NAME] {
            let text = export_text(&state, name, &[]).await.unwrap();
            let back = ProfileConfig::from_toml(&text).unwrap();
            assert_eq!(back.ui.enabled_presets, ["disarm_buff_fade"], "{name}");
            assert_eq!(back.preset_edits, lilac_line(), "{name}");
        }
    }

    /// In loadout mode an export of a profile no session plays carries
    /// the catalog's presets, as one of the profile you play does, and
    /// never the old list its file keeps.
    #[tokio::test]
    async fn a_loadout_export_carries_the_catalog_presets_for_a_closed_profile() {
        use crate::loadouts::catalog::GlobalCatalog;
        use crate::loadouts::preset_edits::lilac_line;
        use vosh_automation::trigger::{Trigger, TriggerAction};
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let mut config = ProfileConfig::default();
        config.ui.enabled_presets = vec!["sent_tells".into()];
        write_profile(dir.path(), "Healer", &config);
        let mut preset = Trigger::new(
            "disarm.secondary",
            "disarms you and sends your weapon flying",
            TriggerAction::Gag,
        );
        preset.preset = Some("disarm_buff_fade".into());
        let yours = Trigger::new("spam", "^spam$", TriggerAction::Gag);
        *state.global_catalog.lock().await = Some(GlobalCatalog {
            triggers: vec![preset, yours],
            enabled_presets: Some(vec!["disarm_buff_fade".into()]),
            preset_edits: lilac_line(),
            ..GlobalCatalog::default()
        });

        let text = export_text(&state, "Healer", &[]).await.unwrap();
        let back = ProfileConfig::from_toml(&text).unwrap();
        assert_eq!(back.ui.enabled_presets, ["disarm_buff_fade"]);
        assert_eq!(back.preset_edits, lilac_line());
        let names: Vec<_> = back.triggers.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["disarm.secondary"]);
    }

    #[tokio::test]
    async fn an_export_ends_with_the_world_and_the_characters_you_ticked() {
        let dir = tempfile::tempdir().unwrap();
        let state = james_like_state(dir.path()).await;
        let world = claim("play.theforsakenlands.com", Some(1848), &["Maren", "Orla"]);
        {
            let mut guard = state.profile_set.lock().await;
            let set = guard.as_mut().unwrap();
            put_claim(set, "Healer", None, world.clone());
            set.set_world("Test-Prompt", None, None).unwrap();
        }
        let ticked = vec!["Orla".to_string(), "Tolliver".to_string()];

        let text = export_text(&state, "Healer", &ticked).await.unwrap();
        let profile = load_profile_file(&state.loaded_profile_set().await.unwrap(), "Healer")
            .unwrap()
            .to_toml()
            .unwrap();
        let table = VoshExport {
            host: world.host,
            port: world.port,
            characters: vec!["Orla".into()],
        };
        assert_eq!(text, table.write(&profile).unwrap());
        assert_eq!(crate::profile::export::read(&text).unwrap(), Some(table));

        // With no world, the table names nothing, not even what you ticked.
        let text = export_text(&state, "Test-Prompt", &ticked).await.unwrap();
        assert!(text.ends_with("\n\n[vosh_export]\n"), "{text}");
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
