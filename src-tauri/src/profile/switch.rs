//! Switching the active profile, when you pick one and when a login
//! names a character another profile claims. A switch saves the profile
//! you leave, reads the next one's file and global.toml, and lays them
//! over the live profile. The same read of global.toml keeps your shared
//! settings across a `#profile reset` or `#profile load`.

use tauri::{AppHandle, Manager};
use tracing::warn;

use crate::app::events::{broadcast, broadcast_profile_ui, PROFILE_SWITCHED};
use crate::app::state::SharedState;
use crate::disk::save::{persist_state, PERSIST_LOCK};
use crate::loadouts::catalog::lay_catalog_over;
use crate::output;
use crate::profile::file::ProfileConfig;
use crate::profile::shared::{GlobalConfig, SharedLayer};

/// The files a switch to a profile loads: its own file, None for a
/// profile that never saved one, and global.toml, None before the first
/// save.
struct SwitchFiles {
    per_profile: Option<ProfileConfig>,
    global: Option<GlobalConfig>,
}

/// Read the files a switch to `name` loads, and only then point the
/// index at it. A file that does not read changes nothing, so the index
/// keeps naming the profile the live state holds and the next persist
/// still writes that profile to its own file.
fn open_profile_for_switch(
    set: &mut crate::profile::set::ProfileSet,
    name: &str,
) -> Result<SwitchFiles, String> {
    use crate::profile::set::{display_name, ProfileSetError};
    if set.get(name).is_none() {
        return Err(ProfileSetError::NotFound(name.to_string()).to_string());
    }
    let refused = |what: &str| {
        format!(
            "Vosh could not open the {} profile because it could not read {what}. You are \
             still using the {} profile.",
            display_name(name),
            display_name(set.active_name()),
        )
    };
    let path = set.profile_path(name);
    let per_profile = if path.exists() {
        match ProfileConfig::load(&path) {
            Ok(config) => Some(config),
            Err(e) => {
                warn!(error = %e, path = %path.display(), "profile file unreadable at switch");
                return Err(refused("the profile file"));
            }
        }
    } else {
        None
    };
    let global_path = set.global_path();
    // Only the categories the scope shares, so a value global.toml still
    // holds from before cannot cover the one the profile file owns.
    let global = match GlobalConfig::load_shared(&global_path, set.scope()) {
        Ok(config) => config,
        Err(e) => {
            warn!(error = %e, path = %global_path.display(), "global config unreadable at switch");
            return Err(refused("global.toml, which holds your shared settings"));
        }
    };
    let leaving = set.active_path();
    set.switch(name).map_err(|e| e.to_string())?;
    // Both files read, and the live profile is about to hold what they
    // say, so the saves may write them again. The file of the profile you
    // left no longer stands behind the live profile, and every other write
    // to it reads it first, so a file that did not read at launch is safe
    // from here on.
    for path in [&leaving, &path, &global_path] {
        crate::disk::atomic::release_unread(path);
    }
    Ok(SwitchFiles {
        per_profile,
        global,
    })
}

/// Steps 2 and 3 of a switch, after the flush of the outgoing profile.
/// Call with [`PERSIST_LOCK`] held. Loads the incoming profile's file
/// and global.toml, points the index at it, then lays both over the
/// live profile, and in loadout mode the catalog and the loadouts too.
/// Either every step lands or none does, and a save that waits on the
/// lock finds the live profile whole.
pub(crate) async fn switch_live_profile(state: &SharedState, name: &str) -> Result<(), String> {
    // Step 2: read the incoming files, then flip the active pointer in
    // the index.
    let SwitchFiles {
        per_profile,
        global,
    } = {
        let mut set = state.loaded_profile_set().await?;
        open_profile_for_switch(&mut set, name)?
    };

    // In loadout mode the catalog holds the aliases, triggers, and
    // macros, so it fills the stores in the same step. Were a save to
    // find the stores empty in between, it would write an empty catalog.
    let catalog = state.global_catalog.lock().await.clone();
    let loadouts = state.loadout_set.lock().await.clone();

    // Step 3: apply the per-profile file (or defaults) and then overlay
    // global.toml so theme/font/keep-last/auto-update/dock_layout
    // survive the switch.
    {
        let mut p = state.profile.lock().await;
        // The custom prompt keeps the connection's GMCP packets and drops
        // the values the last profile's prompt read. The file below hands
        // it the new profile's [prompt] table.
        p.prompt.switch_profile();
        p.display_name = Some(crate::profile::set::display_name(name));
        state.note_active_profile(name);
        match per_profile {
            Some(snap) => {
                snap.apply_to(&mut p);
            }
            None => {
                // A profile that never saved a file is fresh.
                let fresh = ProfileConfig::fresh();
                fresh.apply_to(&mut p);
            }
        }
        if let Some(g) = global {
            g.apply_to(&mut p);
        }
        if let Some(catalog) = &catalog {
            lay_catalog_over(&mut p, catalog, loadouts.as_ref());
        }
        // The latest Char.Prompt of the connection applies to the new
        // profile's capture by the rule every packet follows.
        p.prompt.follow_latest(chrono::Local::now().fixed_offset());
        // Under the same lock as the swap, so a pane layout write edited
        // from the old profile's tree, or a whole config save read from
        // the old profile, is refused from here on.
        state.note_ui_config_replaced();
    }
    Ok(())
}

/// Shared body for switching the active profile. The
/// `profile_switch` Tauri command and the Char.Status auto-switch
/// path in `auto_switch_for_character` both call this so the
/// persist + load + flip sequence stays identical. An error is a
/// sentence for you, and leaves the index and the live profile on the
/// profile you were using.
pub(crate) async fn apply_profile_switch<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    name: &str,
) -> Result<(), String> {
    switch_profile(state, name).await?;
    // The new profile's capture took the game's latest prompt settings.
    let seen = state.profile.lock().await.prompt.take_seen();
    crate::prompt::report_game_prompt_seen(app, seen);

    // Hand every window the new profile's panes, tracked affects, tick
    // settings, and chip style from here, then the replace notice, on
    // which the main window reads the config again and sends every
    // window the rest. These go out before profile-switched so the
    // stores already hold the new values when windows react to the
    // switch.
    broadcast_profile_ui(app, state).await;

    broadcast(app, PROFILE_SWITCHED, &name);
    Ok(())
}

/// Why a profile cannot switch between `migration_apply` and the relaunch
/// that finishes it. The next profile's file holds no aliases, triggers,
/// or macros any more, and the shared catalog that holds them loads only
/// at launch, so the switch would leave you with none.
const SWITCH_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then switch profiles.";

/// Steps 1 to 3 of [`apply_profile_switch`], which need no app handle,
/// so a test can run them.
pub(crate) async fn switch_profile(state: &SharedState, name: &str) -> Result<(), String> {
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

    // Step 1: snapshot + write the CURRENT active profile so user
    // changes since the last persist are not lost on switch. Skipped
    // after a #profile reset/load: the live profile is deliberately
    // diverged from disk and a passive switch (the GMCP Char.Status
    // auto-switch reaches here too) must not write it back.
    if !state
        .auto_persist_suppressed
        .load(std::sync::atomic::Ordering::Acquire)
    {
        persist_state(state).await;
    }

    switch_live_profile(state, name).await
}

/// Switch to the profile that claims `character` on the live
/// connection, when that is not the active one already.
pub(crate) async fn auto_switch_for_character<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    character: &str,
) {
    let Some(new_name) = auto_switch_target(state, character).await else {
        return;
    };
    // A switch that fails leaves the live profile and the index as they
    // were, and says so on the terminal, since nothing else would tell
    // you the login kept the old profile.
    let line = match apply_profile_switch(app, state, &new_name).await {
        Ok(()) => auto_switch_line(&new_name),
        Err(e) => {
            warn!(error = %e, "auto profile switch failed");
            auto_switch_failed_line(&e)
        }
    };
    output::emit_output(app, line.into_bytes());
}

/// The profile that `character` logging in on the live connection
/// should load, when that is not the active one already.
async fn auto_switch_target(state: &SharedState, character: &str) -> Option<String> {
    let (host, port) = state
        .current_connection
        .lock()
        .ok()
        .and_then(|g| g.clone())?;
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref()?;
    set.resolve_match(&host, port, Some(character))
        .filter(|name| name != set.active_name())
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
        state.profile.lock().await.ui.tracked_affects = vec![affect("Sanctuary")];
        *state.profile_set.lock().await = Some(james_like_set(dir));
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
        let mut config = ProfileConfig::default();
        config.ui.tracked_affects = vec![affect("Haste")];
        config.save(&healer_file(dir.path())).unwrap();

        super::switch_live_profile(&state, "Healer").await.unwrap();
        assert_eq!(active(&state).await, "Healer");
        // The events that name the active profile name the new one.
        assert_eq!(state.active_profile().as_deref(), Some("Healer"));
        assert_eq!(live_affects(&state).await, ["Haste"]);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), "Healer");
    }

    #[tokio::test]
    async fn a_switch_keeps_the_gmcp_packets_and_drops_the_values_triggers_set() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        *state.current_connection.lock().unwrap() =
            Some(("play.theforsakenlands.com".into(), 1848));
        {
            let mut p = state.profile.lock().await;
            p.prompt.connect(true);
            let at = chrono::Local::now().fixed_offset();
            p.prompt.vars.observe(
                "Char.Prompt",
                serde_json::json!({"enabled":true,"prompt":"%n%P%C<%hhp %mm %vmv> ","fprompt":""}),
                at,
            );
            p.prompt
                .vars
                .observe("Char.Vitals", serde_json::json!({"hp":850,"maxhp":900}), at);
            p.prompt.vars.set_script("hp", "840");
            p.prompt.vars.set_script("mood", "grim");
            assert_eq!(p.prompt.vars.prompt_vars().len(), 2);
        }

        super::switch_live_profile(&state, "Healer").await.unwrap();

        let p = state.profile.lock().await;
        assert!(p.prompt.forsaken());
        assert!(
            p.prompt.vars.new_build(),
            "the Char.Prompt of this connection stays"
        );
        assert!(p.prompt.vars.gmcp().get("Char.Vitals").is_some());
        assert!(p.prompt.vars.prompt_vars().is_empty());
    }

    #[tokio::test]
    async fn a_switch_hands_the_prompt_the_next_profile_table_and_its_rules() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        {
            let mut p = state.profile.lock().await;
            // A world Vosh does not know, where no Forsaken Lands rule
            // holds until a capture reads Aabahran's codes.
            p.prompt.connect(false);
            p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
            let at = chrono::Local::now().fixed_offset();
            p.prompt
                .vars
                .observe("Char.Vitals", serde_json::json!({"hp":850,"maxhp":900}), at);
            p.prompt.vars.set_script("mood", "grim");
            assert!(!p.prompt.forsaken());
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

        super::switch_live_profile(&state, "Healer").await.unwrap();
        {
            let p = state.profile.lock().await;
            assert_eq!(*p.prompt.config(), healer);
            assert!(!p.ui.prompt_template_enabled);
            assert_eq!(p.ui.prompt_template, "%mana");
            assert!(p.prompt.forsaken(), "the capture reads Aabahran's codes");
            assert!(p.prompt.vars.gmcp().get("Char.Vitals").is_some());
            assert!(p.prompt.vars.prompt_vars().is_empty());
        }

        // On to a profile that never saved a file, a fresh one, which
        // follows the game and draws nothing until you turn drawing on.
        super::switch_live_profile(&state, "Test-Prompt")
            .await
            .unwrap();
        let p = state.profile.lock().await;
        assert_eq!(*p.prompt.config(), vosh_prompt::PromptConfig::fresh());
        assert!(!p.prompt.draws());
        assert!(!p.prompt.forsaken());
        assert!(p.prompt.vars.gmcp().get("Char.Vitals").is_some());
    }

    #[tokio::test]
    async fn a_profile_you_create_follows_the_game_and_keeps_following_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
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

        super::switch_live_profile(&state, "Fresh").await.unwrap();
        {
            let p = state.profile.lock().await;
            assert_eq!(*p.prompt.config(), vosh_prompt::PromptConfig::fresh());
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

        super::switch_live_profile(&state, "Mortal").await.unwrap();
        let p = state.profile.lock().await;
        assert!(p.prompt.config().is_default());
        assert_eq!(live_names(&p.ui.tracked_affects), ["Haste"]);
    }

    fn live_names(list: &[crate::profile::ui::TrackedAffect]) -> Vec<&str> {
        list.iter().map(|t| t.name.as_str()).collect()
    }

    #[tokio::test]
    async fn a_switch_applies_the_latest_char_prompt_to_the_next_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let game = "%n%P%C<%hhp %mm %vmv> ";
        {
            let mut p = state.profile.lock().await;
            p.prompt.connect(true);
            p.prompt.observe(
                "Char.Prompt",
                serde_json::json!({"enabled": true, "prompt": game, "fprompt": ""}),
                chrono::Local::now().fixed_offset(),
            );
            assert!(!p.prompt.take_seen()[0].applied, "default reads nothing");
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

        super::switch_live_profile(&state, "Healer").await.unwrap();
        {
            let mut p = state.profile.lock().await;
            let vosh_prompt::CaptureConfig::Aabahran(taken) = &p.prompt.config().capture else {
                panic!("an aabahran capture");
            };
            assert_eq!(taken.prompt, game);
            assert_eq!(taken.source, Some(vosh_prompt::config::CaptureSource::Gmcp));
            let seen = p.prompt.take_seen();
            assert_eq!(seen.len(), 1);
            assert!(seen[0].applied, "the toast follows");
        }

        // A capture that does not follow the game keeps its codes.
        let mut file = ProfileConfig::default();
        file.set_prompt(codes(false));
        file.save(&healer_file(dir.path())).unwrap();
        super::switch_live_profile(&state, "Test-Prompt")
            .await
            .unwrap();
        super::switch_live_profile(&state, "Healer").await.unwrap();
        let mut p = state.profile.lock().await;
        assert_eq!(*p.prompt.config(), codes(false));
        let leftover = &p.prompt.take_seen();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[tokio::test]
    async fn a_profile_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        std::fs::write(healer_file(dir.path()), UNREADABLE).unwrap();

        let err = super::switch_live_profile(&state, "Healer")
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
        let mut config = ProfileConfig::default();
        config.ui.tracked_affects = vec![affect("Haste")];
        config.save(&healer_file(dir.path())).unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        std::fs::write(set.global_path(), UNREADABLE).unwrap();

        let err = super::switch_live_profile(&state, "Healer")
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
        std::fs::write(healer_file(dir.path()), UNREADABLE).unwrap();
        *state.current_connection.lock().unwrap() =
            Some(("play.theforsakenlands.com".into(), 1848));

        // Corvanne logging in picks Healer, whose file does not read.
        let target = super::auto_switch_target(&state, "Corvanne").await;
        assert_eq!(target.as_deref(), Some("Healer"));
        let err = super::switch_live_profile(&state, "Healer")
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
        assert_eq!(super::auto_switch_target(&state, "Ilsabet").await, None);
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

        // A switch saves the profile you leave first, and that save
        // leaves the file that did not read alone.
        persist(&state).await;
        assert_eq!(read(&set.active_path()), UNREADABLE);
        super::switch_live_profile(&state, "Healer").await.unwrap();
        assert_eq!(live_affects(&state).await, ["Haste"]);
        state.profile.lock().await.ui.tracked_affects = vec![affect("Fly")];
        persist(&state).await;

        let saved = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
        assert_eq!(saved.ui.tracked_affects[0].name, "Fly");
        // The file that did not read was never written.
        assert_eq!(read(&set.profile_path(DEFAULT_PROFILE_NAME)), UNREADABLE);
    }
}
