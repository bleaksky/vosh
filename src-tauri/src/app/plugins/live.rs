//! Plugins turned on and off and loaded again while you play, from the
//! Scripts page in Settings. A plugin is on per profile, so the switch
//! reaches the Lua engine of every session that plays the profile. Each
//! engine keeps its own stops, so a plugin Vosh stopped in one session
//! stays off there until a save or a reload
//! loads it again. Install and Remove turn a plugin off in every
//! profile, open or not, so new code never runs before you turn it on.

use std::sync::Arc;

use tauri::AppHandle;
use vosh_script::{Action, Owner, ScriptOutcome};

use super::folder;
use super::{
    follow_profile_plugins, left_off, load_plugin, plugin_off, plugins_dir_of, read_plugin,
};
use crate::app::state::SharedState;
use crate::disk::save::{persist_profile, PERSIST_LOCK};
use crate::profile::file::ProfileConfig;
use crate::profile::inactive::{edit_inactive_profile, load_profile_file, Stored};
use crate::profile::live::Profile;
use crate::profile::open::OpenProfile;
use crate::profile::set::{display_name, ProfileSet};
use crate::script::ApplyResult;
use crate::session::connection::Connection;
use crate::session::effects::deliver_detached;
use crate::sessions::Session;

/// Turn the plugin `name` on or off in the profile `session` plays, and
/// save the profile. A plugin turned on joins the end of the profile's
/// list, and the others keep their order. Every session on the profile
/// then follows the list, as a switch does, so the plugin loads or
/// unloads in each, and each gets what its own load asks for. A plugin
/// Vosh stopped in a session stays off there, on or not. Only a name
/// that keeps the rule turns on. Any name turns off, since it only
/// leaves the list, so a plugin whose folder you named by hand, which
/// launch loads all the same, never runs on beyond your reach.
pub(crate) async fn set_enabled<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    name: &str,
    on: bool,
) -> Result<(), String> {
    let plugins_dir = plugins_dir_of(state)?;
    if on {
        folder::existing(&plugins_dir, name)?;
    }
    let sessions = state.all_sessions();
    let (open, results) = {
        let mut p = session.lock_profile().await;
        let enabled = &mut p.plugins.enabled;
        if !on {
            enabled.retain(|kept| kept != name);
        } else if !enabled.iter().any(|kept| kept == name) {
            enabled.push(name.to_string());
        }
        let players: Vec<Arc<Session>> = p.players(&sessions).cloned().collect();
        let mut results = Vec::with_capacity(players.len());
        for player in players {
            let apply = follow_profile_plugins(&mut p, &mut player.connection.lock(), &plugins_dir);
            results.push((player, apply.ran_under(p.open())));
        }
        (p.open().clone(), results)
    };
    persist_profile(state, &open).await;
    for (player, apply) in results {
        deliver_detached(app, &player, apply).await;
    }
    Ok(())
}

/// Load the plugin `name` again from its folder in every session whose
/// profile turns it on, for Save and for Reload. A load clears a stop.
/// Each session it loads in prints that it did, ahead of what the load
/// prints, and one it could not read says so instead.
pub(crate) async fn reload_everywhere<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    name: &str,
) -> Result<(), String> {
    let plugins_dir = plugins_dir_of(state)?;
    folder::existing(&plugins_dir, name)?;
    for session in state.all_sessions() {
        let apply = {
            let mut p = session.lock_profile().await;
            if !p.plugins.enabled.iter().any(|kept| kept == name) {
                continue;
            }
            let apply = reload(&mut p, &mut session.connection.lock(), &plugins_dir, name);
            apply.ran_under(p.open())
        };
        deliver_detached(app, &session, apply).await;
    }
    Ok(())
}

/// The profiles that turn the plugin `name` on, by the names Settings
/// shows, in the order of the profile list, for the question Install
/// asks: a profile a session plays as it stands in memory, any other as
/// its file says. A file Vosh cannot read turns nothing on.
pub(crate) async fn turned_on_in(state: &SharedState, name: &str) -> Result<Vec<String>, String> {
    // No switch lands between finding where a profile lives and reading it.
    let _persist_guard = PERSIST_LOCK.lock().await;
    let open_now = state.open_profiles();
    let stored: Vec<(String, Stored<bool>)> = {
        let set = state.loaded_profile_set().await?;
        set.list()
            .iter()
            .map(|entry| {
                let found = open_now
                    .iter()
                    .find(|open| open.name().as_deref() == Some(entry.name.as_str()));
                let stored = match found {
                    Some(open) => Stored::Open(open.clone()),
                    None => Stored::File(file_turns_on(&set, &entry.name, name)),
                };
                (entry.name.clone(), stored)
            })
            .collect()
    };
    let mut on_in = Vec::new();
    for (profile, stored) in stored {
        let on = match stored {
            Stored::File(on) => on,
            Stored::Open(open) => turns_on(&open.lock().await.plugins.enabled, name),
        };
        if on {
            on_in.push(display_name(&profile));
        }
    }
    Ok(on_in)
}

fn turns_on(enabled: &[String], name: &str) -> bool {
    enabled.iter().any(|kept| kept == name)
}

/// Whether the file of the profile `profile` in `set` turns the plugin
/// `name` on.
fn file_turns_on(set: &ProfileSet, profile: &str, name: &str) -> bool {
    load_profile_file(set, profile).is_ok_and(|config| turns_on(&config.plugins.enabled, name))
}

/// Turn the plugin `name` off in every profile, for Install and Remove,
/// so its new code never runs until you turn it on. A profile a
/// session plays drops it from its list in memory and saves, and the
/// plugin unloads in each session on it. Any other profile has its file
/// rewritten when the file turns the plugin on, and stays as it is when
/// it does not. A file Vosh cannot rewrite stops the turn, with the
/// sentence that says why.
pub(crate) async fn off_everywhere<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    name: &str,
) -> Result<(), String> {
    let sessions = state.all_sessions();
    let mut results = Vec::new();
    for profile in files_turning_on(state, name).await? {
        let edited = edit_inactive_profile(state, &profile, |_, config: &mut ProfileConfig| {
            config.plugins.enabled.retain(|kept| kept != name);
        })
        .await?;
        // A session opened the profile since the look.
        if let Stored::Open(open) = edited {
            results.extend(off_in(state, &sessions, &open, name).await);
        }
    }
    for open in state.open_profiles() {
        results.extend(off_in(state, &sessions, &open, name).await);
    }
    for (player, apply) in results {
        deliver_detached(app, &player, apply).await;
    }
    Ok(())
}

/// The profiles no session plays whose file turns the plugin `name` on.
async fn files_turning_on(state: &SharedState, name: &str) -> Result<Vec<String>, String> {
    // The session map comes before the profile set in the lock order.
    let open_now: Vec<String> = state
        .open_profiles()
        .iter()
        .filter_map(|open| open.name())
        .collect();
    let set = state.loaded_profile_set().await?;
    Ok(set
        .list()
        .iter()
        .filter(|entry| !open_now.contains(&entry.name) && file_turns_on(&set, &entry.name, name))
        .map(|entry| entry.name.clone())
        .collect())
}

/// Drop the plugin `name` from the list of `open`, a profile sessions
/// play, and save it, and unload the plugin in each session among
/// `sessions` that plays it and runs it. Returns what each unload asks
/// of its session. A profile that does not turn the plugin on stays as
/// it is.
async fn off_in(
    state: &SharedState,
    sessions: &[Arc<Session>],
    open: &Arc<OpenProfile>,
    name: &str,
) -> Vec<(Arc<Session>, ApplyResult)> {
    let mut results = Vec::new();
    {
        let mut p = open.lock().await;
        if !turns_on(&p.plugins.enabled, name) {
            return results;
        }
        p.plugins.enabled.retain(|kept| kept != name);
        let players: Vec<Arc<Session>> = p.players(sessions).cloned().collect();
        for player in players {
            let mut c = player.connection.lock();
            if c.script
                .loaded_plugins()
                .iter()
                .any(|loaded| loaded == name)
            {
                let apply = plugin_off(&mut p, &mut c, name).ran_under(p.open());
                results.push((player.clone(), apply));
            }
        }
    }
    persist_profile(state, open).await;
    results
}

/// [`super::plugin_on`] for the plugin `name`, with a note in the Output
/// ring and the terminal once Vosh read it, so you know the new code
/// runs: `Vosh reloaded vitals_alert.`
fn reload(
    p: &mut Profile,
    c: &mut Connection,
    plugins_dir: &std::path::Path,
    name: &str,
) -> ApplyResult {
    let plugin = match read_plugin(plugins_dir, name) {
        Ok(plugin) => plugin,
        Err(e) => return left_off(p, c, name, &e),
    };
    let reloaded = ScriptOutcome {
        actions: vec![Action::Note {
            owner: Owner::Plugin(name.to_string()),
            text: format!("Vosh reloaded {name}."),
        }],
        ..ScriptOutcome::default()
    };
    let mut apply = crate::script::apply_actions(p, c, reloaded);
    apply.append(load_plugin(p, c, name, &plugin));
    apply
}
