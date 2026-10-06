//! Plugins turned on and off and loaded again while you play, from the
//! Scripts page in Settings. A plugin is on per profile (Q5 of the
//! Scripts review), so the switch reaches the Lua engine of every session
//! that plays the profile. Each engine keeps its own stops, so a plugin
//! Vosh stopped in one session stays off there until a save or a reload
//! loads it again (Q7).

use std::sync::Arc;

use tauri::AppHandle;
use vosh_script::{Action, Owner, ScriptOutcome};

use super::folder::{self, plugin_name_ok, NAME_RULE};
use super::{follow_profile_plugins, left_off, load_plugin, plugins_dir_of, read_plugin};
use crate::app::state::SharedState;
use crate::disk::save::persist_profile;
use crate::profile::live::Profile;
use crate::script::ApplyResult;
use crate::session::connection::Connection;
use crate::session::effects::deliver_detached;
use crate::sessions::Session;

/// Turn the plugin `name` on or off in the profile `session` plays, and
/// save the profile. A plugin turned on joins the end of the profile's
/// list, and the others keep their order. Every session on the profile
/// then follows the list, as a switch does, so the plugin loads or
/// unloads in each, and each gets what its own load asks for. A plugin
/// Vosh stopped in a session stays off there, on or not.
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
    } else if !plugin_name_ok(name) {
        return Err(NAME_RULE.to_string());
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

/// [`super::plugin_on`] for the plugin `name`, with a note in the Output
/// ring and the terminal once Vosh read it, as frame b1 of the Scripts
/// design shows it: `Vosh reloaded vitals_alert.`
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
