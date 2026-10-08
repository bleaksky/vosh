//! A Vosh profile export imported under Characters, as a new profile or
//! over one you have (Scripts Q9, Q10 and Q26). [`super::plan`] works out
//! the file first, and the import writes under [`PERSIST_LOCK`], as every
//! profile write does.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tracing::warn;

use super::{plan, CatalogJoin, Clash, ImportPlan};
use crate::app::events::{
    broadcast, broadcast_list_changes, profile_ui_events, ListChanges, ListRevisions,
    MACROS_CHANGED, PROFILES_CHANGED,
};
use crate::app::plugins::{follow_profile_plugins, plugins_dir_of};
use crate::app::state::SharedState;
use crate::disk::save::{persist_state, PERSIST_LOCK};
use crate::loadouts::catalog::{lay_catalog_over, GlobalCatalog};
use crate::loadouts::gating::apply_effective_state;
use crate::loadouts::presets::hold_profile_keys;
use crate::loadouts::set::LoadoutSet;
use crate::profile::file::ProfileConfig;
use crate::profile::inactive::{broadcast_profile_changed, edit_inactive_locked, Stored};
use crate::profile::live::{Macro, Profile};
use crate::profile::login_match::AutoMatch;
use crate::profile::open::OpenProfile;
use crate::profile::set::ProfileSet;
use crate::profile::shared::SharedLayer;
use crate::profile::switch::hand_to_connection;
use crate::script::ApplyResult;
use crate::session::effects::deliver_detached;
use crate::sessions::Session;

/// How the import adds the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AddAs {
    /// As a new profile with the name you give.
    New,
    /// Over the profile you name, which keeps its world, its characters
    /// and its plugins.
    Replace,
}

/// What an import did, for the line Characters shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportResult {
    /// The profile the file went to.
    pub name: String,
    /// Each character you turned on that left another profile for the
    /// new one.
    pub moved_from: Vec<MovedCharacter>,
    /// Each character the file names that stays with the profile that
    /// has it.
    pub kept_with: Vec<KeptCharacter>,
    /// In loadout mode, the catalog group the file's triggers, aliases
    /// and macros joined. None when none joined.
    pub catalog_group: Option<String>,
    /// The file's items the catalog already had, which stayed yours.
    pub clashes: Vec<Clash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct MovedCharacter {
    pub character: String,
    /// The profile it left.
    pub profile: String,
    /// Whether that profile's login reads off now. It turns off when the
    /// character was the last one it had.
    pub login_off: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct KeptCharacter {
    pub character: String,
    /// The profile that keeps it.
    pub profile: String,
}

/// Why an import waits between `migration_apply` and the relaunch that
/// finishes it. The profile files hold no aliases, triggers or macros by
/// then, and the catalog that holds them loads only at launch.
const IMPORT_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then import the profile.";

/// Import `text`, the export you picked as `file_name`, and say what
/// happened.
///
/// New writes a profile named `name` with the world of the export and
/// each character of it that you left on in `logins`, login on. One no
/// profile has joins the list, and one another profile has moves to the
/// new profile. A character you turned off stays where it is, off the
/// new list. With no character the login starts off.
///
/// Replace lays the file over your profile `name`. A profile no session
/// plays has its file rewritten. One a session plays takes the file in
/// memory the way `#profile load` does, and every session on it takes the
/// tick settings and the `[prompt]` table, then the profile saves.
///
/// In loadout mode the file's triggers, aliases and macros join the
/// catalog through the profile the selected session plays, whose save
/// writes catalog.toml and lays the change over the other open profiles.
/// Every window then hears that the profiles changed.
pub(crate) async fn apply_import<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    file_name: &str,
    text: &str,
    add_as: AddAs,
    name: &str,
    logins: &[String],
) -> Result<ImportResult, String> {
    let (result, after) = {
        let _persist_guard = PERSIST_LOCK.lock().await;
        // Read under the lock, which the wizard holds until it sets the flag.
        if state.relaunch_pending.load(Ordering::Acquire) {
            return Err(IMPORT_MIGRATION_PENDING.into());
        }
        write(state, file_name, text, add_as, name, logins).await?
    };
    // What the plugins print and send, and the events, go once the lock
    // lets go, since a send takes the session's slot.
    for (session, apply) in after.plugins {
        deliver_detached(app, &session, apply).await;
    }
    for (event, payload) in after.replaced {
        broadcast(app, event, &payload);
    }
    // The lists and macros changed on the profile in front when the
    // import began, which tells the windows only while it still is.
    if let Some(shown) = &after.shown {
        broadcast_list_changes(app, shown, after.lists);
        if let Some(macros) = after.macros.filter(|_| state.in_front(shown)) {
            broadcast(app, MACROS_CHANGED, &macros);
        }
    }
    broadcast(app, PROFILES_CHANGED, &result.name);
    broadcast_profile_changed(app, &result.name);
    Ok(result)
}

/// What the import leaves for after it lets go of the persist lock.
#[derive(Default)]
struct After {
    /// What following its plugins asks of each session on a profile the
    /// import replaced.
    plugins: Vec<(Arc<Session>, ApplyResult)>,
    /// The events a replace sends, when the selected session plays the
    /// profile replaced.
    replaced: Vec<(&'static str, serde_json::Value)>,
    /// The lists of the selected session's profile that changed.
    lists: ListChanges,
    /// Its macros, when they changed.
    macros: Option<Vec<Macro>>,
    /// That profile, which the two above belong to.
    shown: Option<Arc<OpenProfile>>,
}

/// The body of [`apply_import`]. Call with [`PERSIST_LOCK`] held.
async fn write(
    state: &SharedState,
    file_name: &str,
    text: &str,
    add_as: AddAs,
    name: &str,
    logins: &[String],
) -> Result<(ImportResult, After), String> {
    let sessions = state.all_sessions();
    let selected = state.selected_session();
    // The profile the windows show. No session moves under the lock.
    let shown = selected.profile();
    let loadout_mode = state.global_catalog.lock().await.is_some();
    // The catalog the clash list reads, as the selected session's profile
    // holds it, saved or not.
    let catalog = if loadout_mode {
        Some(GlobalCatalog::from_profile(&*shown.lock().await))
    } else {
        None
    };
    let ImportPlan {
        file,
        claim,
        catalog: join,
    } = {
        let set = state.loaded_profile_set().await?;
        plan(file_name, text, set.scope(), catalog.as_ref())?
    };
    let (lists_before, macros_before) = {
        let p = shown.lock().await;
        let c = selected.connection.lock();
        (ListRevisions::of(&p, &c), p.macros.clone())
    };

    let mut result = ImportResult {
        name: name.to_string(),
        moved_from: Vec::new(),
        kept_with: Vec::new(),
        catalog_group: None,
        clashes: Vec::new(),
    };
    let mut after = After::default();
    let mut to_save: Vec<Arc<OpenProfile>> = Vec::new();
    match add_as {
        AddAs::New => {
            let mut set = state.loaded_profile_set().await?;
            add_new(&mut set, name, &file, claim, logins, &mut result)?;
        }
        AddAs::Replace => {
            let replaced = replace(state, &sessions, &shown, name, file, &mut after).await?;
            to_save.extend(replaced);
        }
    }
    if let Some(join) = join {
        let gate = gate_of(state, shown.name().as_deref()).await;
        result.clashes.clone_from(&join.clashes);
        let group = join.group.clone();
        let joined = {
            let mut p = shown.lock().await;
            let joined = join_into(&mut p, join);
            // The loadouts decide the new group here, as they do in each
            // other open profile the save lays the change over.
            if let Some(gate) = gate.as_ref().filter(|_| joined > 0) {
                apply_effective_state(gate, &mut p);
            }
            joined
        };
        if joined > 0 {
            result.catalog_group = Some(group);
            to_save.insert(0, shown.clone());
        }
    }
    {
        let p = shown.lock().await;
        let c = selected.connection.lock();
        after.lists = ListChanges::since(lists_before, &p, &c);
        after.macros = (p.macros != macros_before).then(|| p.macros.clone());
        after.shown = Some(shown.clone());
    }
    // The selected session's profile saves first in loadout mode, so its
    // save writes the catalog and lays it over the profile replaced.
    to_save.dedup_by(|a, b| Arc::ptr_eq(a, b));
    for open in to_save {
        // The import asked for this state, so the passive saves may write
        // it again after a `#profile reset` held them.
        open.hold(false);
        persist_state(state, &open).await;
    }
    Ok((result, after))
}

/// Add the profile `name` from `file` to `set`, with the world of
/// `claim` and each of its characters `logins` names, login on. One no
/// profile has joins the list, and one another profile has moves there
/// through the login toggle, which takes it from that profile. A
/// character `logins` leaves out stays off the list, and stays with the
/// profile that has it. With no character the login starts off, so an
/// empty claim never takes every login on the world.
fn add_new(
    set: &mut ProfileSet,
    name: &str,
    file: &ProfileConfig,
    claim: Option<AutoMatch>,
    logins: &[String],
    result: &mut ImportResult,
) -> Result<(), String> {
    // The sheet names the characters you left on, in any case.
    let on = |character: &str| {
        logins
            .iter()
            .any(|login| login.trim().eq_ignore_ascii_case(character))
    };
    let mut claimed: Vec<(String, String)> = Vec::new();
    let claim = claim.map(|mut am| {
        let host = am.host.clone().unwrap_or_default();
        am.characters
            .retain(|character| match set.claimant(&host, am.port, character) {
                Some(profile) => {
                    claimed.push((character.clone(), profile.to_string()));
                    false
                }
                None => on(character),
            });
        am.enabled = !am.characters.is_empty();
        am
    });
    let entry = set
        .create_from_file(name, file, claim)
        .map_err(|e| e.to_string())?;
    result.name.clone_from(&entry.name);
    for (character, profile) in claimed {
        if !on(&character) {
            result.kept_with.push(KeptCharacter { character, profile });
            continue;
        }
        let moved = set
            .set_login(&entry.name, &character, true)
            .map_err(|e| e.to_string())?;
        for profile in moved.released_from {
            result.moved_from.push(MovedCharacter {
                character: character.clone(),
                profile,
                login_off: false,
            });
        }
    }
    for moved in &mut result.moved_from {
        moved.login_off = !set.login_on(&moved.profile);
    }
    Ok(())
}

/// Lay `file` over your profile `name`, which keeps its entry in
/// profiles.toml, so its world and characters stay, and its own
/// `[plugins]` list, so the plugins of the file stay off. A profile no
/// session plays has its file rewritten. Returns the open profile when a
/// session plays it, once it took the file, see [`replace_open`].
/// `shown` is the profile the selected session plays.
async fn replace(
    state: &SharedState,
    sessions: &[Arc<Session>],
    shown: &Arc<OpenProfile>,
    name: &str,
    file: ProfileConfig,
    after: &mut After,
) -> Result<Option<Arc<OpenProfile>>, String> {
    let mut file = Some(file);
    let stored = edit_inactive_locked(state, name, |_, config| {
        let plugins = std::mem::take(&mut config.plugins);
        *config = file.take().expect("the planned file");
        config.plugins = plugins;
    })
    .await?;
    let Stored::Open(open) = stored else {
        return Ok(None);
    };
    let file = file.expect("a session plays the profile, so its file stayed as it was");
    replace_open(
        state,
        sessions,
        &open,
        Arc::ptr_eq(&open, shown),
        file,
        after,
    )
    .await?;
    Ok(Some(open))
}

/// Lay `file` over `open`, a profile sessions play, the way `#profile
/// load` lays its file. global.toml goes back over it, and in loadout
/// mode the catalog it held, with the loadouts as it gates on them. Each
/// session on it takes the tick settings and the `[prompt]` table, and
/// follows its plugins. Other sessions print no line, since the import
/// came from Settings and no session typed it. When the selected session
/// plays it, `shown`, its tree takes a new generation as it swaps, and
/// every window hears the events a replace sends.
async fn replace_open(
    state: &SharedState,
    sessions: &[Arc<Session>],
    open: &Arc<OpenProfile>,
    shown: bool,
    file: ProfileConfig,
    after: &mut After,
) -> Result<(), String> {
    let name = open.name();
    let loadout_mode = state.global_catalog.lock().await.is_some();
    let gate = gate_of(state, name.as_deref()).await;
    let (shared, path) = {
        let set = state.loaded_profile_set().await?;
        let path = name.as_deref().map(|name| set.profile_path(name));
        (SharedLayer::read(&set.global_path(), *set.scope()), path)
    };
    let plugins_dir = plugins_dir_of(state).ok();
    let mut p = open.lock().await;
    let tick_before = p.tick.config.clone();
    let kept = loadout_mode.then(|| GlobalCatalog::from_profile(&p));
    let plugins = std::mem::take(&mut p.plugins);
    let warnings = shared.keep_across(&mut p, |p| file.apply_to(p));
    p.plugins = plugins;
    if let Some(kept) = &kept {
        lay_catalog_over(&mut p, kept, gate.as_ref());
    }
    for warning in warnings {
        warn!(profile = ?name, %warning, "the imported profile left something out");
    }
    let players: Vec<Arc<Session>> = p.players(sessions).cloned().collect();
    for player in players {
        let mut c = player.connection.lock();
        hand_to_connection(&mut p, &mut c, &tick_before);
        if let Some(dir) = &plugins_dir {
            let apply = follow_profile_plugins(&mut p, &mut c, dir);
            after
                .plugins
                .push((player.clone(), apply.ran_under(p.open())));
        }
    }
    if shown {
        state.bump_panes_generation();
        after.replaced = profile_ui_events(state, &p).events();
    }
    // The profile holds what the import says now, so its saves may write
    // its file again, as after `#profile load`.
    if let Some(path) = path {
        crate::disk::atomic::release_unread(&path);
    }
    Ok(())
}

/// The loadouts as the profile `name` gates on them, in loadout mode.
async fn gate_of(state: &SharedState, name: Option<&str>) -> Option<LoadoutSet> {
    state
        .loadout_set
        .lock()
        .await
        .as_ref()
        .map(|set| set.for_profile(name).into_owned())
}

/// Add the items of `join` to `p`, the profile the selected session
/// plays, whose save writes them to catalog.toml. An item `p` has by
/// then, for a macro one of yours on its key, stays as it is, like each
/// one the clash list names. A preset macro on the key of a macro that
/// joined is held off, see [`hold_profile_keys`]. Returns how many joined.
fn join_into(p: &mut Profile, join: CatalogJoin) -> usize {
    let mut joined = 0;
    for trigger in join.triggers {
        if p.triggers.get(&trigger.name).is_some() {
            continue;
        }
        let name = trigger.name.clone();
        match p.triggers.set(trigger) {
            Ok(()) => joined += 1,
            Err(e) => warn!(error = %e, trigger = %name, "imported trigger rejected"),
        }
    }
    for alias in join.aliases {
        if p.aliases.get(&alias.name).is_none() {
            p.aliases.set(alias);
            joined += 1;
        }
    }
    for m in join.macros {
        if !p
            .macros
            .iter()
            .any(|mine| mine.preset.is_none() && mine.key == m.key)
        {
            p.macros.push(m);
            joined += 1;
        }
    }
    hold_profile_keys(p);
    joined
}

#[cfg(test)]
mod tests;
