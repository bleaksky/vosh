//! Switching profiles, when you pick one and when a login names a
//! character another profile claims. A switch moves one session. When
//! the session is the last on the profile it leaves, it saves that
//! profile, which then closes. It joins the next profile when another
//! session plays it, or else reads the next one's file and global.toml
//! into a profile of its own. The connection carries on as it was,
//! apart from the tick settings and the prompt table the next profile
//! hands it. The same read of global.toml keeps your shared settings
//! across a `#profile reset` or `#profile load`.

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tracing::warn;

use crate::app::events::{broadcast, broadcast_profile_ui, PROFILE_SWITCHED};
use crate::app::state::SharedState;
use crate::disk::save::{persist_state, PERSIST_LOCK};
use crate::loadouts::catalog::lay_catalog_over;
use crate::output;
use crate::profile::file::ProfileConfig;
use crate::profile::live::Profile;
use crate::profile::open::{lock_both, OpenProfile};
use crate::profile::shared::{GlobalConfig, SharedLayer};
use crate::script::ApplyResult;
use crate::session::connection::Connection;
use crate::sessions::Session;
use crate::tick::TickConfig;

/// The files a profile opens from: its own file, None for a profile that
/// never saved one, and global.toml, None before the first save.
struct ProfileFiles {
    per_profile: Option<ProfileConfig>,
    global: Option<GlobalConfig>,
    /// Where the profile's own file lives, or would.
    path: std::path::PathBuf,
}

impl ProfileFiles {
    /// Read the files the profile `name` opens from. A file that does not
    /// read is an error that names it, and that adds that you still use
    /// the profile `using` when a switch asks.
    fn read(
        set: &crate::profile::set::ProfileSet,
        name: &str,
        using: Option<&str>,
    ) -> Result<Self, String> {
        use crate::profile::set::{display_name, ProfileSetError};
        if set.get(name).is_none() {
            return Err(ProfileSetError::NotFound(name.to_string()).to_string());
        }
        let refused = |what: &str| {
            let refusal = format!(
                "Vosh could not open the {} profile because it could not read {what}.",
                display_name(name)
            );
            match using {
                Some(using) => format!(
                    "{refusal} You are still using the {} profile.",
                    display_name(using)
                ),
                None => refusal,
            }
        };
        let path = set.profile_path(name);
        let per_profile = if path.exists() {
            match ProfileConfig::load(&path) {
                Ok(config) => Some(config),
                Err(e) => {
                    warn!(error = %e, path = %path.display(), "profile file unreadable at open");
                    return Err(refused("the profile file"));
                }
            }
        } else {
            None
        };
        let global_path = set.global_path();
        // Only the categories the scope shares, so a value global.toml
        // still holds from before cannot cover the one the profile file
        // owns.
        let global = match GlobalConfig::load_shared(&global_path, set.scope()) {
            Ok(config) => config,
            Err(e) => {
                warn!(error = %e, path = %global_path.display(), "global config unreadable at open");
                return Err(refused("global.toml, which holds your shared settings"));
            }
        };
        Ok(Self {
            per_profile,
            global,
            path,
        })
    }

    /// The profile's file read, and a profile is about to hold what it
    /// says, so the saves may write it again. global.toml stays held
    /// while another open profile may hold the defaults in its place.
    fn release(&self) {
        crate::disk::atomic::release_unread(&self.path);
    }
}

/// The profile `name` as a switch opens it from `files`: its file or a
/// fresh one, global.toml over it, then in loadout mode the catalog with
/// the loadouts as `name` gates on them, so no save finds it without its
/// aliases, triggers and macros.
async fn profile_from_files(state: &SharedState, name: &str, files: ProfileFiles) -> Profile {
    let catalog = state.global_catalog.lock().await.clone();
    let loadouts = state
        .loadout_set
        .lock()
        .await
        .as_ref()
        .map(|set| set.for_profile(Some(name)).into_owned());
    let mut p = Profile::default();
    // A profile that never saved a file is fresh.
    let file = files.per_profile.unwrap_or_else(ProfileConfig::fresh);
    file.apply_to(&mut p);
    // Then global.toml, so theme, font, keep last, auto update and the
    // dock layout survive the switch.
    if let Some(g) = files.global {
        g.apply_to(&mut p);
    }
    if let Some(catalog) = &catalog {
        lay_catalog_over(&mut p, catalog, loadouts.as_ref());
    }
    p
}

/// The open profile `name`, or when no session plays it, that profile
/// read from its files and kept open. Call with [`PERSIST_LOCK`] held, so
/// no other step opens or closes it meanwhile.
pub(crate) async fn open_or_join(
    state: &SharedState,
    name: &str,
) -> Result<Arc<OpenProfile>, String> {
    if let Some(open) = state.open_profile(name) {
        return Ok(open);
    }
    let files = ProfileFiles::read(&*state.loaded_profile_set().await?, name, None)?;
    files.release();
    let profile = profile_from_files(state, name, files).await;
    Ok(state.add_open_profile(name, profile))
}

/// Steps 2 and 3 of a switch, after step 1 saved the profile `session`
/// leaves when no other session plays it. Call with [`PERSIST_LOCK`]
/// held. The session joins `name` when another session plays it, and
/// otherwise opens it from its files, see [`profile_from_files`]. When
/// the session is the selected one, the index then names `name` as
/// active. Either every step lands or none does. The session moves while
/// it holds both profiles, the one that opened first first, and then its
/// connection, so its next step finds the next profile whole, and its
/// connection takes that profile's tick settings and `[prompt]` table
/// and keeps the rest as it was. Its prompt drops the values the last
/// profile's prompt read. The plugins the next profile turns on start in
/// the session's engine and the others stop in the same step, and what
/// they ask for comes back for the caller to deliver once the locks drop,
/// naming the profile the session plays from here. The profile it left
/// closes when no other session plays it.
pub(crate) async fn switch_live_profile(
    state: &SharedState,
    session: &Session,
    name: &str,
) -> Result<ApplyResult, String> {
    let from = session.profile();
    let selected = state.selected_session().id == session.id;
    let point_index = || async {
        if selected {
            let mut set = state.loaded_profile_set().await?;
            set.switch(name).map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    };
    let (to, read) = match state.open_profile(name) {
        // The session plays it already.
        Some(to) if Arc::ptr_eq(&to, &from) => return Ok(ApplyResult::default().ran_under(&to)),
        Some(to) => {
            point_index().await?;
            (to, false)
        }
        None => {
            let using = from.name().unwrap_or_default();
            let files =
                ProfileFiles::read(&*state.loaded_profile_set().await?, name, Some(&using))?;
            point_index().await?;
            files.release();
            let profile = profile_from_files(state, name, files).await;
            (state.add_open_profile(name, profile), true)
        }
    };
    let plugins = move_session(state, session, &from, &to).await;
    if state.close_unplayed(&from) {
        leave_file(state, &from).await;
    }
    // The one profile left open read global.toml in this switch, so the
    // saves may write it again.
    if read && state.open_profiles().len() == 1 {
        if let Some(set) = state.profile_set.lock().await.as_ref() {
            crate::disk::atomic::release_unread(&set.global_path());
        }
    }
    Ok(plugins)
}

/// Move `session` from `from` to `to` with both locked, and hand its
/// connection what `to` holds for it. Returns what the plugins `to` turns
/// on and the others ask for.
async fn move_session(
    state: &SharedState,
    session: &Session,
    from: &Arc<OpenProfile>,
    to: &Arc<OpenProfile>,
) -> ApplyResult {
    let others = state.other_sessions(session.id);
    let (left, mut p) = lock_both(from, to).await;
    // A connected session on the next profile already counts by its
    // switch, so this one follows the switch as it stands, as a connect
    // beside it does, rather than keep its running tick on over it.
    let beside_a_count = p.players(&others).any(|other| other.connected());
    session.play(to.clone());
    // Under the same locks as the move, so a pane layout write edited
    // from the old profile's tree is refused from here on.
    state.bump_panes_generation();

    // The connection did not change, so it keeps what it holds and takes
    // only the next profile's tick settings and [prompt] table. The
    // values the last profile's prompt read go first, since they came
    // from its capture and its scripts.
    let mut c = session.connection.lock();
    c.prompt.switch_profile();
    if beside_a_count {
        c.tick
            .follow(&p.tick, &left.tick.config, tokio::time::Instant::now());
        let table = p.prompt.clone();
        crate::prompt::take_config(&mut p, &mut c, table);
    } else {
        hand_to_connection(&mut p, &mut c, &left.tick.config);
    }
    // The latest Char.Prompt of the connection applies to the next
    // profile's capture by the rule every packet follows, and the profile
    // keeps the table as it then stands.
    let before = c.prompt.revision();
    c.prompt.follow_latest(chrono::Local::now().fixed_offset());
    crate::prompt::keep_table(&mut p, &c, before);
    // The native grid leaves out the mark the next profile gives, as the
    // page does once it hears of the switch.
    crate::input::keep_echo_mark(session.id, crate::input::echo_mark(&p.ui));
    // Under both locks, so no plugin of the profile you left answers a
    // line or a packet for the next one.
    let plugins = match state.app_data.get() {
        Some(app_data) => crate::app::plugins::follow_profile_plugins(
            &mut p,
            &mut c,
            &crate::disk::paths::plugins_dir(app_data),
        ),
        None => ApplyResult::default(),
    };
    plugins.ran_under(to)
}

/// The file of `left`, a profile that just closed, no longer stands
/// behind a profile in memory, and every other write to it reads it
/// first, so a file that did not read at launch is safe from here on.
pub(crate) async fn leave_file(state: &SharedState, left: &OpenProfile) {
    let Some(name) = left.name() else {
        return;
    };
    if let Some(set) = state.profile_set.lock().await.as_ref() {
        crate::disk::atomic::release_unread(&set.profile_path(&name));
    }
}

/// Hand the connection `c` the two things it takes from the profile that
/// a switch, `#profile load`, `#profile reset` or launch just laid over
/// `p`, or that a new session plays: the tick settings and the `[prompt]`
/// table. The tick keeps its count under the new settings, which replaced
/// `tick_before`, so the status line counts on from the last tick. The
/// prompt engine takes the table, and the profile keeps it as the engine
/// then holds it.
pub(crate) fn hand_to_connection(p: &mut Profile, c: &mut Connection, tick_before: &TickConfig) {
    c.tick
        .adopt(&mut p.tick, tick_before, tokio::time::Instant::now());
    let table = p.prompt.clone();
    crate::prompt::take_config(p, c, table);
}

/// Shared body for switching `session` to the profile `name`. The
/// `profile_switch` Tauri command and the Char.Status auto-switch
/// path in `auto_switch_for_character` both call this so the
/// persist + load + flip sequence stays identical. An error is a
/// sentence for you, and leaves the index and the session on the
/// profile you were using.
pub(crate) async fn apply_profile_switch<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    name: &str,
) -> Result<(), String> {
    let plugins = switch_profile(state, session, name).await?;
    // The new profile's capture took the game's latest prompt settings.
    // The plugins ran under that profile, so their result names it.
    let seen = session.connection.lock().prompt.take_seen();
    let open = plugins.profile.clone().unwrap_or_else(|| session.profile());
    crate::prompt::report_game_prompt_seen(app, session, &open, seen);

    // A switch in the selected session brings the next profile to the
    // front. Hand every window its panes, tracked affects, tick settings,
    // and chip style from here, then the replace notice, on which the
    // main window reads the config again and sends every window the
    // rest. These go out before profile-switched so the stores already
    // hold the new values when windows react to the switch. A session
    // behind leaves the profile in front as it was, and the windows take
    // its next profile when a selection brings it to the front.
    if state.selected_session().id == session.id {
        broadcast_profile_ui(app, state).await;
        broadcast(app, PROFILE_SWITCHED, &name);
    }
    // The session's row names the profile it now plays.
    crate::sessions::broadcast_sessions(app, state);

    // What the plugins this profile turned on and the others asked for
    // as the switch made it live.
    crate::app::plugins::follow_profile(app, session, plugins).await;
    Ok(())
}

/// Why a profile cannot switch between `migration_apply` and the relaunch
/// that finishes it. The next profile's file holds no aliases, triggers,
/// or macros any more, and the shared catalog that holds them loads only
/// at launch, so the switch would leave you with none.
const SWITCH_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then switch profiles.";

/// Steps 1 to 3 of [`apply_profile_switch`], which need no app handle,
/// so a test can run them. Returns what the plugins the switch turned on
/// and off ask for.
pub(crate) async fn switch_profile(
    state: &SharedState,
    session: &Session,
    name: &str,
) -> Result<ApplyResult, String> {
    // Hold the persist lock from the flush through loading the next
    // file, so a Settings write to the incoming profile's file lands
    // either before the load reads it or after the switch made the
    // profile live, never in between.
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if state
        .relaunch_pending
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(SWITCH_MIGRATION_PENDING.into());
    }
    // A session that closed plays nothing more, and its close saves and
    // closes the profile it played. A login in its last moments would
    // otherwise close the profile it leaves unsaved, since the closed
    // session no longer counts as its last player. The close takes the
    // session out of the map under this lock, so the check is exact.
    state.session(Some(session.id))?;

    // Step 1: the last session on a profile saves it as it leaves, so
    // your changes since the last save are not lost when it closes, and
    // a next profile read from its files takes global.toml as you left
    // it. A profile another session still plays stays open, and its own
    // saves write it. Skipped after a #profile reset/load: the profile
    // is deliberately diverged from disk and a passive switch (the GMCP
    // Char.Status auto-switch reaches here too) must not write it back.
    let leaving = session.profile();
    if state.players(&leaving) == 1 && !leaving.held() {
        persist_state(state, &leaving).await;
    }

    let plugins = switch_live_profile(state, session, name).await?;
    // A launch restores the session on the profile it moved to.
    crate::profile::set::save_sessions(state).await;
    Ok(plugins)
}

/// Switch `session` to the profile that claims `character` on the
/// connection it runs, when it does not play that one already.
pub(crate) async fn auto_switch_for_character<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    character: &str,
) {
    let Some(new_name) = auto_switch_target(state, session, character).await else {
        return;
    };
    // A switch that fails leaves the live profile and the index as they
    // were, and says so on the terminal, since nothing else would tell
    // you the login kept the old profile.
    let line = match apply_profile_switch(app, state, session, &new_name).await {
        Ok(()) => auto_switch_line(&new_name),
        Err(e) => {
            warn!(error = %e, "auto profile switch failed");
            auto_switch_failed_line(&e)
        }
    };
    output::emit_output(app, session, line.into_bytes());
}

/// The profile that `character` logging in on the connection `session`
/// runs should play, when it does not play that one already.
async fn auto_switch_target(
    state: &SharedState,
    session: &Session,
    character: &str,
) -> Option<String> {
    let (host, port) = session
        .current_connection
        .lock()
        .ok()
        .and_then(|g| g.clone())?;
    let playing = session.profile().name();
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref()?;
    set.resolve_match(&host, port, Some(character))
        .filter(|name| Some(name) != playing.as_ref())
}

/// The terminal line that says a login switched the profile, in the
/// yellow that tick warnings use.
fn auto_switch_line(profile: &str) -> String {
    format!(
        "\r\n\x1b[33mVosh switched to the {} profile.\x1b[0m\r\n",
        crate::profile::set::display_name(profile)
    )
}

/// The terminal line that says a login switch did not happen, in the
/// same yellow. `error` is the sentence the switch returned.
pub(crate) fn auto_switch_failed_line(error: &str) -> String {
    format!("\r\n\x1b[33m{error}\x1b[0m\r\n")
}

/// global.toml as a switch reads it, for `#profile reset` and `#profile
/// load` to lay back over the config they swap in. Holds the persist
/// lock for the read, so a save cannot move the file aside midway. None
/// before startup loads the profile set.
pub(crate) async fn read_shared_layer(state: &SharedState) -> Option<SharedLayer> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref()?;
    Some(SharedLayer::read(&set.global_path(), *set.scope()))
}

/// [`read_shared_layer`] for a path other than typed input, when one of
/// `lines` is a `#profile reset` or `#profile load` that acts. A timer,
/// the tick auto-fire command, and `mud.input` then keep the shared
/// settings across it the way typed input does. Call before taking the
/// profile lock.
pub(crate) async fn shared_layer_for_lines<'a, R: tauri::Runtime>(
    app: &AppHandle<R>,
    lines: impl IntoIterator<Item = &'a str>,
) -> Option<SharedLayer> {
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let replaces = lines
        .into_iter()
        .any(|line| crate::input::may_replace_profile(&state, line));
    if !replaces {
        return None;
    }
    read_shared_layer(&state).await
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::app::state::AppState;
    use crate::disk::save::tests::{affect, launch_state, live_affects, persist, read, UNREADABLE};
    use crate::profile::file::ProfileConfig;
    use crate::profile::set::{ProfileSet, DEFAULT_PROFILE_NAME};
    use crate::profile::tests::james_like_set;

    #[test]
    fn auto_switch_line_names_the_profile_in_a_sentence() {
        assert_eq!(
            super::auto_switch_line("Ilsabet"),
            "\r\n\x1b[33mVosh switched to the Ilsabet profile.\x1b[0m\r\n"
        );
        assert_eq!(
            super::auto_switch_line("default"),
            "\r\n\x1b[33mVosh switched to the Default profile.\x1b[0m\r\n"
        );
    }

    /// App state over James's profile set in `dir`, with `default` live
    /// and tracking Sanctuary.
    async fn switch_state(dir: &std::path::Path) -> super::SharedState {
        let state: super::SharedState = std::sync::Arc::new(AppState::default());
        state.app_data.set(dir.to_path_buf()).unwrap();
        state.selected_profile().await.ui.tracked_affects = vec![affect("Sanctuary")];
        state.set_profiles(james_like_set(dir)).await;
        state
    }

    pub(crate) async fn active(state: &super::SharedState) -> String {
        let guard = state.profile_set.lock().await;
        guard.as_ref().unwrap().active_name().to_string()
    }

    fn healer_file(dir: &std::path::Path) -> std::path::PathBuf {
        ProfileSet::load_or_migrate(dir.to_path_buf())
            .unwrap()
            .profile_path("Healer")
    }

    #[tokio::test]
    async fn a_switch_loads_the_named_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        let mut config = ProfileConfig::default();
        config.ui.tracked_affects = vec![affect("Haste")];
        config.save(&healer_file(dir.path())).unwrap();

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        assert_eq!(active(&state).await, "Healer");
        // The events that name the active profile name the new one.
        assert_eq!(state.active_profile().as_deref(), Some("Healer"));
        assert_eq!(live_affects(&state).await, ["Haste"]);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), "Healer");
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn a_switch_hands_the_native_grid_the_mark_of_the_next_profile() {
        use crate::native::grid;

        // The grid map is shared with the other tests.
        let _grid = grid::lock_shared_grid_for_test();
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        let mut config = ProfileConfig::default();
        config.ui.input_echo_mark = "gt".into();
        config.save(&healer_file(dir.path())).unwrap();
        let echo = |mark: &str, command: &str| {
            let mut out = vosh_prompt::stage::Output::new(false);
            out.text(b"Your choice> ");
            grid::feed_session_output(session.id, &out, None);
            grid::feed_local(session.id, format!("{mark}{command}\r\n").as_bytes());
        };
        let gt = "\x1b[90m> \x1b[0m";
        // The chevron of the profile you leave keeps the mark you typed.
        echo(gt, "1");
        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        echo(gt, "2");
        let rows = grid::screen_rows(session.id).unwrap().rows;
        assert_eq!(rows[..2], ["Your choice> > 1", "Your choice> 2"]);
    }

    #[tokio::test]
    async fn a_switch_turns_the_plugins_over_with_the_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        let plugins = crate::disk::paths::plugins_dir(dir.path());
        for name in ["everywhere", "healer_only", "default_only"] {
            let plugin = plugins.join(name);
            std::fs::create_dir_all(&plugin).unwrap();
            std::fs::write(
                plugin.join("manifest.toml"),
                format!("[plugin]\nname = \"{name}\"\n"),
            )
            .unwrap();
            std::fs::write(plugin.join("main.lua"), format!("mud.echo('{name} on')")).unwrap();
        }
        {
            let mut p = state.selected_profile().await;
            let mut c = session.connection.lock();
            p.plugins.enabled = vec!["default_only".into(), "everywhere".into()];
            crate::app::plugins::follow_profile_plugins(&mut p, &mut c, &plugins);
        }
        let mut config = ProfileConfig::default();
        config.plugins.enabled = vec!["everywhere".into(), "healer_only".into()];
        config.save(&healer_file(dir.path())).unwrap();

        let apply = super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        // Once the swap lets go of the profile, the plugins of the one you
        // left are off, and what the new one printed waits to show.
        assert_eq!(
            session.connection.lock().script.loaded_plugins(),
            ["everywhere", "healer_only"]
        );
        assert_eq!(apply.echoes, ["healer_only on"]);
    }

    #[tokio::test]
    async fn a_switch_keeps_the_gmcp_packets_and_drops_the_values_triggers_set() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        *session.current_connection.lock().unwrap() =
            Some(("play.theforsakenlands.com".into(), 1848));
        {
            let mut c = session.connection.lock();
            c.prompt.connect(true);
            let at = chrono::Local::now().fixed_offset();
            c.prompt.vars.observe(
                "Char.Prompt",
                serde_json::json!({"enabled":true,"prompt":"%n%P%C<%hhp %mm %vmv> ","fprompt":""}),
                at,
            );
            c.prompt
                .vars
                .observe("Char.Vitals", serde_json::json!({"hp":850,"maxhp":900}), at);
            c.prompt.vars.set_script("hp", "840");
            c.prompt.vars.set_script("mood", "grim");
            assert_eq!(c.prompt.vars.prompt_vars().len(), 2);
        }

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();

        let c = session.connection.lock();
        assert!(c.prompt.forsaken());
        assert!(
            c.prompt.vars.new_build(),
            "the Char.Prompt of this connection stays"
        );
        assert!(c.prompt.vars.gmcp().get("Char.Vitals").is_some());
        assert!(c.prompt.vars.prompt_vars().is_empty());
    }

    #[tokio::test]
    async fn a_switch_leaves_the_connection_as_it_was_but_drops_the_prompt_values() {
        use crate::session::connection::{QuickKey, RoomChar};
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        // Healer saved the tick off.
        let mut config = ProfileConfig::default();
        config.tick.enabled = false;
        config.save(&healer_file(dir.path())).unwrap();
        let goblin = vec![RoomChar {
            name: "a goblin".into(),
            npc: true,
        }];
        let gg = QuickKey {
            name: "gg".into(),
            verb: "kill".into(),
        };
        let status = serde_json::json!({"level": 60});
        let char_state = serde_json::json!({"language": ""});
        let vitals = serde_json::json!({"hp": 850, "maxhp": 900});
        let (look, count, who, pulse) = {
            let mut p = state.selected_profile().await;
            let mut c = session.connection.lock();
            c.target.name = Some("goblin".into());
            c.target.room_idx = Some(1);
            c.target.quick_keys = vec![gg.clone()];
            c.room_chars = goblin.clone();
            c.room_block.room_chars(1);
            c.fight_tail = true;
            let t0 = tokio::time::Instant::now();
            c.tick.start_session(&mut p.tick, t0);
            assert!(
                c.tick.on_game_tick(&p.tick, t0).is_some(),
                "the game ticked"
            );
            c.prompt.connect(true);
            let at = chrono::Local::now().fixed_offset();
            c.prompt.observe("Char.Status", status.clone(), at);
            c.prompt.observe("Char.State", char_state.clone(), at);
            c.prompt.observe("Char.Vitals", vitals.clone(), at);
            c.prompt.vars.set_script("mood", "grim");
            assert!(!c.prompt.vars.prompt_vars().is_empty());
            (
                c.room_block.clone(),
                (c.tick.last_tick, c.tick.last_signal),
                c.prompt.who(),
                c.prompt.vars.gmcp().pulse(),
            )
        };
        assert_ne!(look, crate::session::room_block::RoomBlock::default());
        assert_ne!(
            who,
            vosh_prompt::aabahran::Who::default(),
            "an immortal in a mobile"
        );

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();

        let p = state.selected_profile().await;
        let c = session.connection.lock();
        assert_eq!(c.target.name.as_deref(), Some("goblin"));
        assert_eq!(c.target.room_idx, Some(1));
        assert_eq!(c.target.quick_keys, [gg]);
        assert_eq!(c.room_chars, goblin);
        assert_eq!(c.room_block, look);
        assert!(c.fight_tail);
        assert!(p.tick.config.enabled, "a running tick stays on");
        assert!(c.tick.synced);
        assert_eq!((c.tick.last_tick, c.tick.last_signal), count);
        let gmcp = c.prompt.vars.gmcp();
        assert_eq!(gmcp.get("Char.Status"), Some(&status));
        assert_eq!(gmcp.get("Char.State"), Some(&char_state));
        assert_eq!(gmcp.get("Char.Vitals"), Some(&vitals));
        assert_eq!(gmcp.pulse(), pulse);
        assert_eq!(c.prompt.who(), who);
        let left = c.prompt.vars.prompt_vars();
        assert!(left.is_empty(), "{left:?}");
    }

    #[tokio::test]
    async fn a_switch_hands_the_prompt_the_next_profile_table_and_its_rules() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        {
            let mut p = state.selected_profile().await;
            let mut c = session.connection.lock();
            // A world Vosh does not know, where no Forsaken Lands rule
            // holds until a capture reads Aabahran's codes.
            c.prompt.connect(false);
            crate::prompt::take_config(
                &mut p,
                &mut c,
                vosh_prompt::PromptConfig::from_legacy(true, "%hp"),
            );
            let at = chrono::Local::now().fixed_offset();
            c.prompt
                .vars
                .observe("Char.Vitals", serde_json::json!({"hp":850,"maxhp":900}), at);
            c.prompt.vars.set_script("mood", "grim");
            assert!(!c.prompt.forsaken());
        }
        let healer = vosh_prompt::PromptConfig {
            draw: false,
            template: "%mana".into(),
            previous_templates: vec!["%move".into()],
            capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
                prompt: "<%h%m %vmv> ".into(),
                ..vosh_prompt::config::AabahranCapture::default()
            }),
            // Each profile keeps its own place, through its file.
            show: vosh_prompt::PromptShow::Pinned,
            mirror: false,
        };
        let mut file = ProfileConfig::default();
        file.set_prompt(healer.clone());
        file.save(&healer_file(dir.path())).unwrap();

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        {
            let p = state.selected_profile().await;
            let c = session.connection.lock();
            assert_eq!(*c.prompt.config(), healer);
            assert!(!p.ui.prompt_template_enabled);
            assert_eq!(p.ui.prompt_template, "%mana");
            assert!(c.prompt.forsaken(), "the capture reads Aabahran's codes");
            assert!(c.prompt.vars.gmcp().get("Char.Vitals").is_some());
            assert!(c.prompt.vars.prompt_vars().is_empty());
        }

        // On to a profile that never saved a file, a fresh one, which
        // follows the game and draws nothing until you turn drawing on.
        super::switch_live_profile(&state, &session, "Test-Prompt")
            .await
            .unwrap();
        let c = session.connection.lock();
        assert_eq!(*c.prompt.config(), vosh_prompt::PromptConfig::fresh());
        assert!(!c.prompt.draws());
        assert!(!c.prompt.forsaken());
        assert!(c.prompt.vars.gmcp().get("Char.Vitals").is_some());
    }

    #[tokio::test]
    async fn a_profile_you_create_follows_the_game_and_keeps_following_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        {
            let mut guard = state.profile_set.lock().await;
            let set = guard.as_mut().unwrap();
            set.create_from("Fresh", None, None).unwrap();
            // A profile whose file holds no design keeps none.
            set.create_from("Mortal", None, None).unwrap();
            let mut file = ProfileConfig::default();
            file.ui.tracked_affects = vec![affect("Haste")];
            file.save(&set.profile_path("Mortal")).unwrap();
        }

        super::switch_live_profile(&state, &session, "Fresh")
            .await
            .unwrap();
        {
            let p = state.selected_profile().await;
            let c = session.connection.lock();
            assert_eq!(*c.prompt.config(), vosh_prompt::PromptConfig::fresh());
            assert_eq!(p.ui.prompt_template, "");
        }
        // The first save keeps it following the game.
        persist(&state).await;
        let path = state
            .profile_set
            .lock()
            .await
            .as_ref()
            .unwrap()
            .profile_path("Fresh");
        assert_eq!(
            ProfileConfig::load(&path).unwrap().prompt_config(),
            vosh_prompt::PromptConfig::fresh()
        );

        super::switch_live_profile(&state, &session, "Mortal")
            .await
            .unwrap();
        let p = state.selected_profile().await;
        assert!(session.connection.lock().prompt.config().is_default());
        assert_eq!(live_names(&p.ui.tracked_affects), ["Haste"]);
    }

    fn live_names(list: &[crate::profile::ui::TrackedAffect]) -> Vec<&str> {
        list.iter().map(|t| t.name.as_str()).collect()
    }

    #[tokio::test]
    async fn a_switch_applies_the_latest_char_prompt_to_the_next_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        let game = "%n%P%C<%hhp %mm %vmv> ";
        {
            let mut c = session.connection.lock();
            c.prompt.connect(true);
            c.prompt.observe(
                "Char.Prompt",
                serde_json::json!({"enabled": true, "prompt": game, "fprompt": ""}),
                chrono::Local::now().fixed_offset(),
            );
            assert!(!c.prompt.take_seen()[0].applied, "default reads nothing");
        }
        let codes = |follow_game| vosh_prompt::PromptConfig {
            draw: true,
            template: "%hp".into(),
            capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
                prompt: "<%h%m %vmv> ".into(),
                follow_game,
                ..vosh_prompt::config::AabahranCapture::default()
            }),
            ..vosh_prompt::PromptConfig::default()
        };
        let mut file = ProfileConfig::default();
        file.set_prompt(codes(true));
        file.save(&healer_file(dir.path())).unwrap();

        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        {
            let mut c = session.connection.lock();
            let vosh_prompt::CaptureConfig::Aabahran(taken) = &c.prompt.config().capture else {
                panic!("an aabahran capture");
            };
            assert_eq!(taken.prompt, game);
            assert_eq!(taken.source, Some(vosh_prompt::config::CaptureSource::Gmcp));
            let seen = c.prompt.take_seen();
            assert_eq!(seen.len(), 1);
            assert!(seen[0].applied, "the toast follows");
        }

        // A capture that does not follow the game keeps its codes.
        let mut file = ProfileConfig::default();
        file.set_prompt(codes(false));
        file.save(&healer_file(dir.path())).unwrap();
        super::switch_live_profile(&state, &session, "Test-Prompt")
            .await
            .unwrap();
        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        let mut c = session.connection.lock();
        assert_eq!(*c.prompt.config(), codes(false));
        let leftover = &c.prompt.take_seen();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[tokio::test]
    async fn a_profile_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        std::fs::write(healer_file(dir.path()), UNREADABLE).unwrap();

        let err = super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "Vosh could not open the Healer profile because it could not read the profile \
             file. You are still using the Default profile."
        );
        // The index still names the live profile, in memory and on
        // disk, so the next persist writes it to its own file.
        assert_eq!(active(&state).await, DEFAULT_PROFILE_NAME);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), DEFAULT_PROFILE_NAME);
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
        // The file that did not read stays as it was.
        assert_eq!(
            std::fs::read_to_string(healer_file(dir.path())).unwrap(),
            UNREADABLE
        );
    }

    #[tokio::test]
    async fn a_global_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        let mut config = ProfileConfig::default();
        config.ui.tracked_affects = vec![affect("Haste")];
        config.save(&healer_file(dir.path())).unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        std::fs::write(set.global_path(), UNREADABLE).unwrap();

        let err = super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "Vosh could not open the Healer profile because it could not read global.toml, \
             which holds your shared settings. You are still using the Default profile."
        );
        assert_eq!(active(&state).await, DEFAULT_PROFILE_NAME);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), DEFAULT_PROFILE_NAME);
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
    }

    #[tokio::test]
    async fn a_login_switch_to_a_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let session = state.selected_session();
        std::fs::write(healer_file(dir.path()), UNREADABLE).unwrap();
        *session.current_connection.lock().unwrap() =
            Some(("play.theforsakenlands.com".into(), 1848));

        // Corvanne logging in picks Healer, whose file does not read.
        let target = super::auto_switch_target(&state, &session, "Corvanne").await;
        assert_eq!(target.as_deref(), Some("Healer"));
        let err = super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap_err();
        assert_eq!(
            super::auto_switch_failed_line(&err),
            "\r\n\x1b[33mVosh could not open the Healer profile because it could not read \
             the profile file. You are still using the Default profile.\x1b[0m\r\n"
        );
        assert_eq!(active(&state).await, DEFAULT_PROFILE_NAME);
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
        // Ilsabet belongs to the live profile, so nothing switches.
        assert_eq!(
            super::auto_switch_target(&state, &session, "Ilsabet").await,
            None
        );
    }

    #[tokio::test]
    async fn a_switch_that_reads_its_files_lets_the_saves_resume() {
        let dir = tempfile::tempdir().unwrap();
        let set = james_like_set(dir.path());
        std::fs::write(set.active_path(), UNREADABLE).unwrap();
        let mut healer = ProfileConfig::default();
        healer.ui.tracked_affects = vec![affect("Haste")];
        healer.save(&set.profile_path("Healer")).unwrap();
        let state = launch_state(dir.path()).await;
        let session = state.selected_session();

        // A switch saves the profile you leave first, and that save
        // leaves the file that did not read alone.
        persist(&state).await;
        assert_eq!(read(&set.active_path()), UNREADABLE);
        super::switch_live_profile(&state, &session, "Healer")
            .await
            .unwrap();
        assert_eq!(live_affects(&state).await, ["Haste"]);
        state.selected_profile().await.ui.tracked_affects = vec![affect("Fly")];
        persist(&state).await;

        let saved = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
        assert_eq!(saved.ui.tracked_affects[0].name, "Fly");
        // The file that did not read was never written.
        assert_eq!(read(&set.profile_path(DEFAULT_PROFILE_NAME)), UNREADABLE);
    }
}
