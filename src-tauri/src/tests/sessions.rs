//! Two sessions on the one profile, each against a fake game of its own,
//! through the harness of the fake MUD tests. Each test holds a rule
//! that keeps one session's connection, variables or Lua engine apart
//! from the other's, or that lets what one session changes on the
//! profile reach the other.
//!
//! Every session output also feeds the shared native grid, so each test
//! holds its guard to keep the others off it. No task of a session takes
//! it.

use serde_json::json;
use tauri::Manager;
use vosh_prompt::testkit::mud::{PROMPT, PROMPT_ALL};
use vosh_prompt::testkit::{Build, Options};

use super::fake_mud::harness::{codes, codes_of, FakeServer, Harness};
use crate::profile::set::DEFAULT_PROFILE_NAME;
use crate::sessions::SessionId;

/// What clients sent `server`, as text.
fn sent(server: &FakeServer) -> String {
    String::from_utf8_lossy(&server.received.lock().expect("the bytes")).into_owned()
}

/// Whether a row on the terminal of `session` shows `text`.
fn shows(h: &Harness, session: SessionId, text: &str) -> bool {
    h.screen_of(session).iter().any(|row| row.contains(text))
}

/// What `$name` reads in `session`: its own value, or else the profile's.
async fn var(h: &Harness, session: SessionId, name: &str) -> Option<String> {
    let session = h.state.session(Some(session)).expect("the session");
    let p = h.state.selected_profile().await;
    let c = session.connection.lock();
    c.var_view(&p).get(name).map(str::to_string)
}

/// The harness with a second session, the first session logged in to the
/// first game and the second to the second.
async fn two_sessions_on_two_games() -> (Harness, SessionId, SessionId) {
    log_in_two_sessions(Harness::new(Options::new(Build::New)).await).await
}

/// A plugin that answers the end of `spam 2` with `afk`, and the end of
/// `spam 1` with an alias of its own, `hh`, for `spam 3`.
const HELPER: &str = "\
    mud.trigger('afk', 'Line 2 of 2 of the spam', function() mud.send('afk') end)\n\
    mud.trigger('hh', 'Line 1 of 1 of the spam', function() mud.alias('hh', 'spam 3') end)";

/// [`two_sessions_on_two_games`] with the plugin [`HELPER`] on in the
/// profile both play. The first session loads it at launch and the second
/// as it opens, each into its own engine.
async fn two_sessions_with_a_plugin() -> (Harness, SessionId, SessionId) {
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    let plugins = crate::disk::paths::plugins_dir(h.dir.path());
    write_plugin(&plugins, "helper", HELPER);
    h.state.selected_profile().await.plugins.enabled = vec!["helper".into()];
    let first = h.state.selected_session();
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &h.state, &first, plugins).await;
    log_in_two_sessions(h).await
}

/// Write the plugin `name`, whose entry script is `body`, into the
/// plugins folder `plugins`.
fn write_plugin(plugins: &std::path::Path, name: &str, body: &str) {
    let plugin = plugins.join(name);
    std::fs::create_dir_all(&plugin).expect("the plugin folder");
    std::fs::write(
        plugin.join("manifest.toml"),
        format!("[plugin]\nname = \"{name}\"\n"),
    )
    .expect("the manifest");
    std::fs::write(plugin.join("main.lua"), body).expect("the entry script");
}

/// Open a second session in `h`, and log the first session in to the
/// first game and the second to the second.
async fn log_in_two_sessions(h: Harness) -> (Harness, SessionId, SessionId) {
    let (one, two) = (h.first, h.open_session().await);
    h.connect_to(one, &h.servers[0]).await;
    h.connect_to(two, &h.servers[1]).await;
    let welcome = "Welcome to the fake Aabahran, Tester.";
    h.until("both logins", |h| {
        shows(h, one, welcome) && shows(h, two, welcome)
    })
    .await;
    (h, one, two)
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_sessions_on_one_profile_play_two_games_apart() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    h.type_in(one, "spam 3").await;
    h.type_in(two, "spam 4").await;
    h.until("the spam of each game", |h| {
        shows(h, one, "Line 3 of 3 of the spam.") && shows(h, two, "Line 4 of 4 of the spam.")
    })
    .await;

    // Each game heard only the line typed in its own session.
    let (first, second) = (sent(&h.servers[0]), sent(&h.servers[1]));
    assert!(
        first.contains("spam 3") && !first.contains("spam 4"),
        "{first:?}"
    );
    assert!(
        second.contains("spam 4") && !second.contains("spam 3"),
        "{second:?}"
    );
    // Each terminal shows only its own session's echo and output.
    assert!(!shows(&h, one, "spam 4") && !shows(&h, one, "of 4 of the spam"));
    assert!(!shows(&h, two, "spam 3") && !shows(&h, two, "of 3 of the spam"));
    // Neither login moved the profile both sessions play.
    let active = h
        .state
        .profile_set
        .lock()
        .await
        .as_ref()
        .expect("the set")
        .active_name()
        .to_string();
    assert_eq!(active, DEFAULT_PROFILE_NAME);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_target_in_one_session_leaves_the_others_empty() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    h.type_in(one, "tar goblin").await;
    h.until("the target in the first session", |h| {
        shows(h, one, "target: goblin")
    })
    .await;

    let target = crate::ipc::session::target_get(h.app.state(), Some(one))
        .await
        .expect("the first target");
    assert_eq!(target.name.as_deref(), Some("goblin"));
    let target = crate::ipc::session::target_get(h.app.state(), Some(two))
        .await
        .expect("the second target");
    assert_eq!(target.name, None);
    assert_eq!(h.events_of(two, "session://target").len(), 0);
    assert!(!shows(&h, two, "goblin"));
    // So is the variable that mirrors it.
    assert_eq!(var(&h, one, "target").await.as_deref(), Some("goblin"));
    assert_eq!(var(&h, two, "target").await, None);
    h.type_in(one, "kick $target").await;
    h.type_in(two, "kick $target").await;
    h.until("both kicks", |h| {
        sent(&h.servers[0]).contains("kick goblin\r\n")
            && sent(&h.servers[1]).contains("kick $target\r\n")
    })
    .await;

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_disconnect_in_one_session_leaves_the_other_connected_with_its_target() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    for (id, name) in [(one, "goblin"), (two, "orc")] {
        // The fake game sends no Room.Chars, so the list goes in place
        // the way the session takes one.
        let session = h.state.session(Some(id)).expect("the session");
        crate::input::target::set_room_chars(
            &mut session.connection.lock(),
            crate::input::target::read_room_chars(&[
                json!({"name": format!("a {name}"), "npc": true}),
            ]),
        );
        h.type_in(id, &format!("tar {name}")).await;
    }
    let names = |h: &Harness, id| {
        h.events_of(id, "session://target")
            .last()
            .map(|t| t["name"].clone())
    };
    h.until("both targets", |h| {
        names(h, one) == Some(json!("goblin")) && names(h, two) == Some(json!("orc"))
    })
    .await;

    h.disconnect_session(two).await;
    h.until("the second target to clear", |h| {
        names(h, two) == Some(json!(null))
    })
    .await;

    // Only the second session's task ended, and only its connection
    // dropped its target and room list.
    let (first, second) = (
        h.state.session(Some(one)).expect("the first session"),
        h.state.session(Some(two)).expect("the second session"),
    );
    assert!(first.slot.lock().await.is_some());
    assert!(second.slot.lock().await.is_none());
    {
        let c = first.connection.lock();
        assert_eq!(c.target.name.as_deref(), Some("goblin"));
        assert_eq!(c.room_chars.len(), 1);
    }
    {
        let c = second.connection.lock();
        assert_eq!(c.target.name, None);
        assert_eq!(c.room_chars.len(), 0);
    }
    assert_eq!(var(&h, one, "target").await.as_deref(), Some("goblin"));
    assert_eq!(var(&h, two, "target").await, None);
    assert_eq!(names(&h, one), Some(json!("goblin")));
    assert!(h
        .events_of(one, "session://state")
        .iter()
        .all(|state| state["kind"] != "disconnected"));

    // The first session still plays its game.
    h.type_in(one, "spam 2").await;
    h.until("the first game to answer", |h| {
        shows(h, one, "Line 2 of 2 of the spam.")
    })
    .await;
    assert!(sent(&h.servers[0]).contains("spam 2"));
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn char_vitals_from_each_game_binds_the_hp_of_its_own_session() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    h.type_in(one, "fight").await;
    h.until("the fight in the first game", |h| {
        shows(h, one, "A Blackwatch guard attacks you!")
    })
    .await;
    h.type_in(one, "#echo The first has $hp hp.").await;
    h.type_in(two, "#echo The second has $hp hp.").await;
    h.until("both answers", |h| {
        shows(h, one, "The first has 765 hp.") && shows(h, two, "The second has 1020 hp.")
    })
    .await;
    assert_eq!(h.state.selected_profile().await.vars.get("hp"), None);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_var_stays_in_its_session_and_unvar_takes_the_profile_value_from_both() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    h.state.selected_profile().await.vars.set("home", "Hollow");
    h.type_in(one, "#var mood grim").await;
    h.type_in(two, "#var mood").await;
    h.type_in(two, "#var home inn").await;
    h.until("the second session to miss the mood", |h| {
        shows(h, two, "var mood not set")
    })
    .await;
    assert_eq!(var(&h, one, "mood").await.as_deref(), Some("grim"));
    assert_eq!(var(&h, one, "home").await.as_deref(), Some("Hollow"));
    assert_eq!(var(&h, two, "home").await.as_deref(), Some("inn"));

    h.type_in(one, "#unvar home").await;
    assert_eq!(h.state.selected_profile().await.vars.get("home"), None);
    assert_eq!(var(&h, one, "home").await, None);
    assert_eq!(var(&h, two, "home").await.as_deref(), Some("inn"));

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_var_lua_sets_in_one_session_reads_in_the_other_and_saves_once() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    let marks = || h.state.selected_session().profile().marks();
    let before = marks();
    h.type_in(one, "#lua mud.set_profile_var('home', 'Hollow')")
        .await;
    let after = marks();
    assert!(after > before, "the line marks the profile to save");

    h.type_in(two, "recall $home").await;
    h.until("the second game to hear the value", |h| {
        sent(&h.servers[1]).contains("recall Hollow\r\n")
    })
    .await;
    // The second session marks nothing, so the one save the line started
    // writes the value to the file of the profile both play.
    assert_eq!(marks(), after);
    let file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    let saved = |file: &std::path::Path| {
        crate::profile::file::ProfileConfig::load(file)
            .ok()
            .and_then(|config| config.profile_vars.get("home").cloned())
    };
    h.until("the save", |_| saved(&file).is_some()).await;
    assert_eq!(saved(&file).as_deref(), Some("Hollow"));

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plugin_trigger_sends_only_to_the_game_of_the_line_it_matched() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_a_plugin().await;
    for id in [one, two] {
        let session = h.state.session(Some(id)).expect("the session");
        assert_eq!(
            session.connection.lock().script.loaded_plugins(),
            ["helper"]
        );
    }
    h.type_in(one, "spam 2").await;
    h.until("the first game to go afk", |h| {
        shows(h, one, "You are now in AFK mode.")
    })
    .await;
    h.type_in(two, "spam 1").await;
    h.until("the second game to answer", |h| {
        shows(h, two, "Line 1 of 1 of the spam.")
    })
    .await;

    assert!(sent(&h.servers[0]).contains("afk"));
    let second = sent(&h.servers[1]);
    assert!(!second.contains("afk"), "{second:?}");
    assert!(!shows(&h, two, "AFK"));

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lua_timer_fires_only_in_the_session_that_set_it() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_a_plugin().await;
    h.type_in(
        one,
        "#lua mud.timer(0.1, function() mud.echo('The timer rang.') end)",
    )
    .await;
    h.until("the timer in the first session", |h| {
        shows(h, one, "The timer rang.")
    })
    .await;
    // Two more polls of the second session find nothing due.
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    assert!(!shows(&h, two, "The timer rang."));
    let second = h.state.session(Some(two)).expect("the second session");
    let leftover = second.lua_timers.lock().await.len();
    assert_eq!(leftover, 0);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lua_global_stays_in_the_session_that_set_it() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_a_plugin().await;
    h.type_in(one, "#lua x = 1").await;
    h.type_in(one, "#lua mud.echo('The first has ' .. tostring(x) .. '.')")
        .await;
    h.type_in(
        two,
        "#lua mud.echo('The second has ' .. tostring(x) .. '.')",
    )
    .await;
    h.until("both answers", |h| {
        shows(h, one, "The first has 1.") && shows(h, two, "The second has nil.")
    })
    .await;

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_alias_lua_makes_in_one_session_expands_in_the_other_and_saves_once() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_a_plugin().await;
    let marks = || h.state.selected_session().profile().marks();
    let before = marks();
    h.type_in(one, "#lua mud.alias('ww', 'spam 3')").await;
    let after = marks();
    assert!(after > before, "the line marks the profile to save");

    h.type_in(two, "ww").await;
    h.until("the second game to answer the alias", |h| {
        shows(h, two, "Line 3 of 3 of the spam.")
    })
    .await;
    assert!(sent(&h.servers[1]).contains("spam 3"));
    // The second session marks nothing, so the one save the line started
    // writes the alias once to the file of the profile both play.
    assert_eq!(marks(), after);
    let file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    let saved = |file: &std::path::Path| {
        crate::profile::file::ProfileConfig::load(file).map_or(0, |config| {
            config.aliases.iter().filter(|a| a.name == "ww").count()
        })
    };
    h.until("the save", |_| saved(&file) > 0).await;
    assert_eq!(saved(&file), 1);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_alias_a_plugin_makes_in_one_session_passes_through_in_the_other() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_a_plugin().await;
    h.type_in(one, "spam 1").await;
    h.until("the first game to answer", |h| {
        shows(h, one, "Line 1 of 1 of the spam.")
    })
    .await;
    h.type_in(one, "hh").await;
    h.type_in(two, "hh").await;
    h.until("both games to answer hh", |h| {
        shows(h, one, "Line 3 of 3 of the spam.") && shows(h, two, "Huh?")
    })
    .await;

    // The first session expanded it, and the second sent it as typed.
    assert!(!sent(&h.servers[0]).contains("hh"));
    let second = sent(&h.servers[1]);
    assert!(
        second.contains("hh\r\n") && !second.contains("spam 3"),
        "{second:?}"
    );
    let second = h.state.session(Some(two)).expect("the second session");
    let leftover = second.connection.lock().plugin_aliases.list().len();
    assert_eq!(leftover, 0);
    assert!(h.state.selected_profile().await.aliases.get("hh").is_none());

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recording_takes_only_its_own_sessions_lines_and_its_alias_expands_in_both() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_a_plugin().await;
    h.type_in(one, "#record walkabout").await;
    h.type_in(two, "look").await;
    h.type_in(one, "spam 3").await;
    h.type_in(two, "#record").await;
    h.type_in(one, "#endrec").await;
    h.until("the recording to save", |h| {
        shows(h, one, "saved macro `walkabout` (1 command(s))")
    })
    .await;
    assert!(shows(&h, two, "not recording."));
    let expansion = h
        .state
        .selected_profile()
        .await
        .aliases
        .get("walkabout")
        .map(|a| a.expansion.clone());
    assert_eq!(expansion.as_deref(), Some("spam 3"));

    h.type_in(two, "walkabout").await;
    h.until("the second game to answer the alias", |h| {
        shows(h, two, "Line 3 of 3 of the spam.")
    })
    .await;
    assert!(sent(&h.servers[1]).contains("spam 3"));

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trigger_vosh_stops_in_one_session_fires_in_the_other_until_you_save_it() {
    use vosh_automation::trigger::{Trigger, TriggerAction};
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    let body = "if spin then while true do end end mud.send('afk')";
    let ender = Trigger::new(
        "ender",
        "^Line 1 of 1 of the spam",
        TriggerAction::Script { body: body.into() },
    );
    h.state
        .selected_profile()
        .await
        .triggers
        .set(ender)
        .expect("the trigger compiles");
    h.type_in(one, "#lua spin = true").await;
    h.type_in(one, "spam 1").await;
    h.until("the stop in the first session", |h| {
        shows(h, one, "Vosh stopped the Lua in trigger ender")
    })
    .await;
    h.type_in(two, "spam 1").await;
    h.until("the second game to go afk", |h| {
        shows(h, two, "You are now in AFK mode.")
    })
    .await;
    assert!(!shows(&h, two, "Vosh stopped"));
    {
        let p = h.state.selected_profile().await;
        assert!(p.triggers.is_stopped("ender", one.stop_key()));
        assert!(!p.triggers.is_stopped("ender", two.stop_key()));
    }

    // It stays off in the first session, though its Lua would no longer
    // spin there. The game answers spam 2 after any afk it heard first.
    h.type_in(one, "#lua spin = false").await;
    h.type_in(one, "spam 1").await;
    h.until("the first game to answer again", |h| {
        let rows = h.screen_of(one);
        rows.iter()
            .filter(|row| row.contains("Line 1 of 1 of the spam."))
            .count()
            == 2
    })
    .await;
    h.type_in(one, "spam 2").await;
    h.until("the first game to answer spam 2", |h| {
        shows(h, one, "Line 2 of 2 of the spam.")
    })
    .await;
    let first = sent(&h.servers[0]);
    assert!(!first.contains("afk"), "{first:?}");

    // Saving it in either session turns it back on in the first.
    h.type_in(two, "#trigger ender {^Line 1 of 1 of the spam} send afk")
        .await;
    h.until("the save", |h| shows(h, two, "trigger ender set"))
        .await;
    h.type_in(one, "spam 1").await;
    h.until("the first game to go afk", |h| {
        shows(h, one, "You are now in AFK mode.")
    })
    .await;

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

/// How many backups sit beside `file`, one for each save that replaced
/// it.
fn backups(file: &std::path::Path) -> usize {
    let name = file.file_name().expect("a file name").to_string_lossy();
    let prefix = format!("{name}.bak.");
    std::fs::read_dir(file.parent().expect("the profiles folder"))
        .expect("the profiles folder reads")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .count()
}

/// How many aliases named `alias` the profile file at `file` holds.
fn saved_aliases(file: &std::path::Path, alias: &str) -> usize {
    crate::profile::file::ProfileConfig::load(file).map_or(0, |config| {
        config.aliases.iter().filter(|a| a.name == alias).count()
    })
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_alias_typed_in_one_session_expands_in_the_other_and_saves_once() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    // The file as it stands, so the save the alias starts leaves a backup.
    crate::disk::save::tests::persist(&h.state).await;
    let file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    let before = backups(&file);

    h.type_in(one, "#alias ww spam 3").await;
    h.type_in(two, "ww").await;
    h.until("the second game to answer the alias", |h| {
        shows(h, two, "Line 3 of 3 of the spam.")
    })
    .await;
    assert!(sent(&h.servers[1]).contains("spam 3"));
    h.until("the save", |_| saved_aliases(&file, "ww") > 0)
        .await;
    // Past the debounce no second write follows.
    tokio::time::sleep(std::time::Duration::from_millis(2_500)).await;
    assert_eq!(saved_aliases(&file, "ww"), 1);
    assert_eq!(backups(&file), before + 1);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_switch_moves_one_session_and_each_profile_saves_to_its_own_file() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    let second = h.state.session(Some(two)).expect("the second session");
    crate::profile::switch::apply_profile_switch(h.app.handle(), &h.state, &second, "Healer")
        .await
        .expect("the second session switches");
    let plays = |id| {
        let session = h.state.session(Some(id)).expect("the session");
        session.profile().name()
    };
    assert_eq!(plays(one).as_deref(), Some(DEFAULT_PROFILE_NAME));
    assert_eq!(plays(two).as_deref(), Some("Healer"));
    assert_eq!(h.state.open_profiles().len(), 2);
    // The first session stays selected, and profiles.toml names its
    // profile.
    assert_eq!(
        h.state.active_profile().as_deref(),
        Some(DEFAULT_PROFILE_NAME)
    );
    let index = crate::profile::set::ProfileSet::load_or_migrate(h.dir.path().to_path_buf())
        .expect("the index reads");
    assert_eq!(index.active_name(), DEFAULT_PROFILE_NAME);

    let (default_file, healer_file) = (
        h.profile_file(DEFAULT_PROFILE_NAME).await,
        h.profile_file("Healer").await,
    );
    crate::disk::save::tests::persist(&h.state).await;
    let default_backups = backups(&default_file);
    h.type_in(two, "#alias hh spam 2").await;
    h.type_in(one, "hh").await;
    h.until("the first game to answer the plain word", |h| {
        shows(h, one, "Huh?")
    })
    .await;
    h.until("the Healer save", |_| saved_aliases(&healer_file, "hh") > 0)
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(2_500)).await;
    assert_eq!(saved_aliases(&default_file, "hh"), 0);
    assert_eq!(backups(&default_file), default_backups);

    let healer_backups = backups(&healer_file);
    h.type_in(two, "#profile save").await;
    h.until("the save in the second session", |h| {
        shows(h, two, "profile saved to")
    })
    .await;
    assert_eq!(backups(&healer_file), healer_backups + 1);
    assert_eq!(backups(&default_file), default_backups);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

/// Open a session on the profile `name`, as the page does with the
/// session form's Profile row.
async fn open_session_on(h: &Harness, name: &str) -> SessionId {
    crate::ipc::session::session_open(h.app.handle().clone(), h.app.state(), Some(name.into()))
        .await
        .expect("a new session")
}

/// The names of the profiles the sessions play, in the order they
/// opened.
fn open_names(h: &Harness) -> Vec<String> {
    h.state
        .open_profiles()
        .iter()
        .filter_map(|open| open.name())
        .collect()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_on_its_own_profile_saves_only_that_file_and_closing_it_closes_the_profile() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    let two = open_session_on(&h, "Healer").await;
    let second = h.state.session(Some(two)).expect("the second session");
    assert_eq!(second.profile().name().as_deref(), Some("Healer"));
    assert_eq!(open_names(&h), [DEFAULT_PROFILE_NAME, "Healer"]);
    h.connect_to(two, &h.servers[1]).await;
    h.until("the second login", |h| {
        shows(h, two, "Welcome to the fake Aabahran, Tester.")
    })
    .await;

    let (default_file, healer_file) = (
        h.profile_file(DEFAULT_PROFILE_NAME).await,
        h.profile_file("Healer").await,
    );
    crate::disk::save::tests::persist(&h.state).await;
    let default_backups = backups(&default_file);
    h.type_in(two, "#alias hh spam 2").await;
    h.until("the Healer save", |_| saved_aliases(&healer_file, "hh") > 0)
        .await;
    let healer_backups = backups(&healer_file);
    h.type_in(two, "#profile save").await;
    h.until("the save in the second session", |h| {
        shows(h, two, "profile saved to")
    })
    .await;
    assert_eq!(backups(&healer_file), healer_backups + 1);
    assert_eq!(saved_aliases(&default_file, "hh"), 0);
    assert_eq!(backups(&default_file), default_backups);

    // A change the debounce has yet to write saves as the session closes.
    second.lock_profile().await.vars.set("home", "Hollow");
    crate::ipc::session::session_close(h.app.handle().clone(), h.app.state(), two)
        .await
        .expect("the second session closes");
    assert!(second.slot.lock().await.is_none(), "its connection ended");
    assert!(h.state.session(Some(two)).is_err());
    assert_eq!(open_names(&h), [DEFAULT_PROFILE_NAME]);
    let saved = crate::profile::file::ProfileConfig::load(&healer_file).expect("Healer's file");
    assert_eq!(
        saved.profile_vars.get("home").map(String::as_str),
        Some("Hollow")
    );
    // Vosh keeps the only session.
    assert_eq!(
        crate::ipc::session::session_close(h.app.handle().clone(), h.app.state(), h.first).await,
        Err(crate::sessions::ONLY_SESSION.to_string())
    );

    h.finish(grid).await;
}

/// The sessions profiles.toml in `h` keeps for the next launch, with the
/// selected one and the active profile.
fn kept_sessions(
    h: &Harness,
) -> (
    Vec<crate::profile::set::SessionEntry>,
    Option<SessionId>,
    String,
) {
    let text = std::fs::read_to_string(h.dir.path().join("profiles.toml")).expect("the index");
    let index: crate::profile::set::ProfilesIndex = toml::from_str(&text).expect("it reads");
    (index.sessions, index.selected, index.active)
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profiles_toml_keeps_the_sessions_while_they_say_more_than_the_active_profile() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let index = h.dir.path().join("profiles.toml");
    let alone = std::fs::read_to_string(&index).expect("the index");
    let rename = |session, name: Option<&str>| {
        crate::ipc::session::session_rename(h.app.state(), session, name.map(str::to_string))
    };
    // One session with no name says nothing the active profile does not.
    rename(h.first, None).await.expect("the rename");
    assert_eq!(std::fs::read_to_string(&index).expect("the index"), alone);

    let two = h.open_session().await;
    let (kept, selected, _) = kept_sessions(&h);
    let ids: Vec<_> = kept.iter().map(|entry| entry.id).collect();
    assert_eq!(ids, [h.first, two]);
    assert!(kept
        .iter()
        .all(|entry| entry.profile == DEFAULT_PROFILE_NAME));
    assert_eq!(selected, Some(h.first));

    // A connect somewhere new, a rename, a switch and a selection each
    // save the list.
    let second = h.state.session(Some(two)).expect("the second session");
    crate::session::connect(
        h.app.handle(),
        &h.state,
        &second,
        "127.0.0.1".into(),
        h.servers[1].port,
        false,
    )
    .await
    .expect("the second game answers");
    crate::session::disconnect(h.app.handle(), &h.state, &second).await;
    let entry = kept_sessions(&h).0.remove(1);
    assert_eq!(
        (entry.host.as_deref(), entry.port, entry.tls),
        (Some("127.0.0.1"), Some(h.servers[1].port), false)
    );
    rename(two, Some("Alt")).await.expect("the rename");
    assert_eq!(kept_sessions(&h).0[1].name.as_deref(), Some("Alt"));
    crate::profile::switch::switch_profile(&h.state, &second, "Healer")
        .await
        .expect("the switch");
    assert_eq!(kept_sessions(&h).0[1].profile, "Healer");
    crate::ipc::session::session_select(h.app.handle().clone(), h.app.state(), two)
        .await
        .expect("the selection moves");
    let (_, selected, active) = kept_sessions(&h);
    assert_eq!((selected, active.as_str()), (Some(two), "Healer"));

    // Back to one session with no name, the file reads as an older build
    // writes it.
    crate::ipc::session::session_close(h.app.handle().clone(), h.app.state(), two)
        .await
        .expect("the second session closes");
    assert_eq!(std::fs::read_to_string(&index).expect("the index"), alone);

    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn a_profile_a_session_plays_stays_on_delete_and_renames_for_every_session_on_it() {
    // A selection shows the session's grid, which other tests read.
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let two = open_session_on(&h, "Healer").await;
    let three = open_session_on(&h, "Healer").await;
    // With the second session selected, profiles.toml names Healer, and
    // Default stays open for the first.
    crate::ipc::session::session_select(h.app.handle().clone(), h.app.state(), two)
        .await
        .expect("the selection moves");
    assert_eq!(h.state.active_profile().as_deref(), Some("Healer"));
    assert_eq!(
        crate::profile::set::delete_profile(&h.state, DEFAULT_PROFILE_NAME).await,
        Err(
            crate::profile::set::ProfileSetError::CannotDeleteActive(DEFAULT_PROFILE_NAME.into())
                .to_string()
        )
    );
    assert!(h
        .state
        .profile_set
        .lock()
        .await
        .as_ref()
        .expect("the set")
        .get(DEFAULT_PROFILE_NAME)
        .is_some());

    crate::profile::set::rename_profile(&h.state, "Healer", "Cleric")
        .await
        .expect("the rename");
    for id in [two, three] {
        let session = h.state.session(Some(id)).expect("the session");
        assert_eq!(session.profile().name().as_deref(), Some("Cleric"));
        let shown = crate::prompt::client_values(
            &*session.lock_profile().await,
            &session.connection.lock(),
            tokio::time::Instant::now(),
        );
        assert_eq!(shown.profile.as_deref(), Some("Cleric"));
    }
    assert_eq!(open_names(&h), [DEFAULT_PROFILE_NAME, "Cleric"]);
    assert_eq!(h.state.active_profile().as_deref(), Some("Cleric"));
}

/// The name of the profile `session` plays.
fn plays(h: &Harness, session: SessionId) -> Option<String> {
    let session = h.state.session(Some(session)).expect("the session");
    session.profile().name()
}

/// The plugins the Lua engine of `session` runs.
fn plugins_of(h: &Harness, session: SessionId) -> Vec<String> {
    let session = h.state.session(Some(session)).expect("the session");
    let c = session.connection.lock();
    c.script.loaded_plugins()
}

/// The profile variable `name` as the profile file at `file` saves it.
fn saved_var(file: &std::path::Path, name: &str) -> Option<String> {
    let config = crate::profile::file::ProfileConfig::load(file).ok()?;
    config.profile_vars.get(name).cloned()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_moves_its_own_session_and_a_login_in_the_other_joins_that_profile_in_memory() {
    use vosh_automation::trigger::{Trigger, TriggerAction};
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    // Healer claims Healer on every port of the host, so on both games.
    let claim = crate::profile::login_match::AutoMatch {
        host: Some("127.0.0.1".into()),
        port: None,
        characters: vec!["Healer".into()],
        enabled: true,
    };
    h.state
        .profile_set
        .lock()
        .await
        .as_mut()
        .expect("the set")
        .set_metadata("Healer", None, Some(claim))
        .expect("Healer claims Healer");
    let plugins = crate::disk::paths::plugins_dir(h.dir.path());
    write_plugin(
        &plugins,
        "on_default",
        "mud.trigger('afk', 'Line 2 of 2 of the spam', function() mud.send('afk') end)",
    );
    write_plugin(&plugins, "on_healer", "mud.echo('on_healer loaded')");
    let healer_file = h.profile_file("Healer").await;
    let mut healer = crate::profile::file::ProfileConfig::default();
    healer.plugins.enabled = vec!["on_healer".into()];
    healer.save(&healer_file).expect("Healer's file");
    {
        let mut p = h.state.selected_profile().await;
        p.plugins.enabled = vec!["on_default".into()];
        let bow = Trigger::new(
            "bow",
            "^Line 1 of 2 of the spam",
            TriggerAction::Send {
                template: "bow".into(),
            },
        );
        p.triggers.set(bow).expect("the trigger compiles");
        // A change no save has written yet.
        p.vars.set("camp", "Ford");
    }
    let first = h.state.selected_session();
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &h.state, &first, plugins).await;
    let (one, two) = (h.first, h.open_session().await);
    let default_file = h.profile_file(DEFAULT_PROFILE_NAME).await;

    // Healer logs in on the first game, and only the first session moves.
    h.servers[0].options.lock().expect("the options").name = "Healer".into();
    h.connect_to(one, &h.servers[0]).await;
    h.until("the first session to move", |h| {
        shows(h, one, "Vosh switched to the Healer profile.") && shows(h, one, "on_healer loaded")
    })
    .await;
    assert_eq!(plays(&h, one).as_deref(), Some("Healer"));
    assert_eq!(plays(&h, two).as_deref(), Some(DEFAULT_PROFILE_NAME));
    assert_eq!(plugins_of(&h, one), ["on_healer"]);
    assert_eq!(plugins_of(&h, two), ["on_default"]);
    // Default stays open for the second session, and the first did not
    // save it as it left.
    assert_eq!(open_names(&h), [DEFAULT_PROFILE_NAME, "Healer"]);
    assert_eq!(saved_var(&default_file, "camp"), None);

    // Default's trigger and plugin answer the second game alone.
    h.connect_to(two, &h.servers[1]).await;
    h.until("the Tester login", |h| {
        shows(h, two, "Welcome to the fake Aabahran, Tester.")
    })
    .await;
    h.type_in(two, "spam 2").await;
    h.until("the second game to go afk", |h| {
        shows(h, two, "You are now in AFK mode.")
    })
    .await;
    h.type_in(one, "spam 2").await;
    h.type_in(one, "spam 1").await;
    h.until("the first game to answer both", |h| {
        shows(h, one, "Line 1 of 1 of the spam.")
    })
    .await;
    let second = sent(&h.servers[1]);
    assert!(second.contains("bow\r\n"), "{second:?}");
    let sent_first = sent(&h.servers[0]);
    assert!(
        !sent_first.contains("bow") && !sent_first.contains("afk"),
        "{sent_first:?}"
    );

    // Healer logs in on the second game too, and the second session joins
    // the Healer the first plays without reading its file: a change on
    // disk stays out, and a change the first made and nothing saved shows.
    h.disconnect_session(two).await;
    let mut on_disk = crate::profile::file::ProfileConfig::default();
    on_disk
        .profile_vars
        .insert("drawn".into(), "from disk".into());
    on_disk.save(&healer_file).expect("Healer's file");
    first.lock_profile().await.vars.set("home", "Hollow");
    h.servers[1].options.lock().expect("the options").name = "Healer".into();
    h.connect_to(two, &h.servers[1]).await;
    h.until("the second session to move", |h| {
        shows(h, two, "Vosh switched to the Healer profile.")
    })
    .await;
    let second = h.state.session(Some(two)).expect("the second session");
    assert!(std::sync::Arc::ptr_eq(&first.profile(), &second.profile()));
    {
        let p = second.lock_profile().await;
        assert_eq!(p.vars.get("home"), Some("Hollow"));
        assert_eq!(p.vars.get("drawn"), None);
    }
    assert_eq!(
        saved_var(&healer_file, "drawn").as_deref(),
        Some("from disk")
    );
    assert_eq!(plugins_of(&h, two), ["on_healer"]);
    // Default, left by its last session, saved and closed.
    assert_eq!(open_names(&h), ["Healer"]);
    assert_eq!(saved_var(&default_file, "camp").as_deref(), Some("Ford"));

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_switches_the_opposite_way_at_once_both_finish() {
    let h = Harness::new(Options::new(Build::New)).await;
    let two = open_session_on(&h, "Healer").await;
    let switch = |id, name: &'static str| {
        let (app, state) = (h.app.handle().clone(), h.state.clone());
        let session = state.session(Some(id)).expect("the session");
        tokio::spawn(async move {
            crate::profile::switch::apply_profile_switch(&app, &state, &session, name).await
        })
    };
    let (one_way, other_way) = (switch(h.first, "Healer"), switch(two, DEFAULT_PROFILE_NAME));
    let both = async { (one_way.await, other_way.await) };
    let (one_way, other_way) = tokio::time::timeout(std::time::Duration::from_secs(5), both)
        .await
        .expect("both switches finish");
    one_way.expect("the task").expect("the first switch");
    other_way.expect("the task").expect("the second switch");
    assert_eq!(plays(&h, h.first).as_deref(), Some("Healer"));
    assert_eq!(plays(&h, two).as_deref(), Some(DEFAULT_PROFILE_NAME));
    // Each profile is open once, in whichever order the switches ran.
    let names = open_names(&h);
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().any(|name| name == DEFAULT_PROFILE_NAME));
    assert!(names.iter().any(|name| name == "Healer"));
}

/// When the tick count of `session` last restarted, while it runs.
fn last_tick(h: &Harness, session: SessionId) -> Option<tokio::time::Instant> {
    let session = h.state.session(Some(session)).expect("the session");
    let last = session.connection.lock().tick.last_tick;
    last
}

/// The tick settings the profile file at `file` holds.
fn saved_tick(file: &std::path::Path) -> Option<crate::tick::TickConfig> {
    crate::profile::file::ProfileConfig::load(file)
        .ok()
        .map(|config| config.tick)
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tick_disable_in_one_session_stops_the_others_count_and_saves_off_once() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    crate::disk::save::tests::persist(&h.state).await;
    let file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    let before = backups(&file);
    assert!(last_tick(&h, two).is_some(), "the second count runs");

    h.type_in(one, "#tick disable").await;
    assert_eq!(last_tick(&h, one), None);
    assert_eq!(last_tick(&h, two), None);
    h.until("the save", |_| {
        saved_tick(&file).is_some_and(|tick| !tick.enabled)
    })
    .await;
    // Past the debounce no second write follows.
    tokio::time::sleep(std::time::Duration::from_millis(2_500)).await;
    assert_eq!(backups(&file), before + 1);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_interval_from_settings_reaches_both_counts() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    // The second game's spam is a real tick, so the second count syncs.
    h.type_in(one, "#tick on {Line 1 of 1 of the spam}").await;
    h.type_in(two, "spam 1").await;
    let second = h.state.session(Some(two)).expect("the second session");
    h.until("the second count to sync", |_| {
        second.connection.lock().tick.synced
    })
    .await;

    let config = h.state.selected_profile().await.tick.config.clone();
    let saved = crate::ipc::tick::tick_set_config(
        h.app.handle().clone(),
        h.app.state(),
        crate::tick::TickConfig {
            interval_secs: 45,
            ..config
        },
    )
    .await
    .expect("the settings apply");
    assert_eq!(saved.interval_secs, 45);
    let p = h.state.selected_profile().await;
    let now = tokio::time::Instant::now();
    let first = h.state.session(Some(one)).expect("the first session");
    // The first count, unsynced, starts again at the new interval.
    let left = first.connection.lock().tick.remaining(&p.tick, now);
    assert!(left.is_some_and(|left| left.as_secs() >= 44), "{left:?}");
    // The second keeps its count and waits for the game at the new one.
    let c = second.connection.lock();
    assert!(c.tick.synced);
    assert!(c.tick.interval_changed_at.is_some());
    assert_eq!(
        c.tick.next_fire(&p.tick),
        c.tick
            .last_tick
            .map(|last| last + std::time::Duration::from_secs(45))
    );
    drop((c, p));

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tick_reset_in_one_session_leaves_the_others_count_alone() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    let (first, second) = (last_tick(&h, one), last_tick(&h, two));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    h.type_in(one, "#tick reset").await;
    assert!(last_tick(&h, one) > first);
    assert_eq!(last_tick(&h, two), second);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connect_beside_a_connected_session_keeps_the_tick_you_turned_off_off() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let (one, two) = (h.first, h.open_session().await);
    let welcome = "Welcome to the fake Aabahran, Tester.";
    h.connect_to(one, &h.servers[0]).await;
    h.until("the first login", |h| shows(h, one, welcome)).await;
    h.type_in(one, "#tick disable").await;

    h.connect_to(two, &h.servers[1]).await;
    h.until("the second login", |h| shows(h, two, welcome))
        .await;
    assert!(!h.state.selected_profile().await.tick.config.enabled);
    let second = h.state.session(Some(two)).expect("the second session");
    let c = second.connection.lock();
    assert!(c.tick.in_session);
    assert_eq!(c.tick.last_tick, None);
    drop(c);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_switch_beside_a_connected_session_keeps_the_tick_you_turned_off_off() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    let (one, two) = (h.first, open_session_on(&h, "Healer").await);
    let welcome = "Welcome to the fake Aabahran, Tester.";
    h.connect_to(one, &h.servers[0]).await;
    h.connect_to(two, &h.servers[1]).await;
    h.until("both logins", |h| {
        shows(h, one, welcome) && shows(h, two, welcome)
    })
    .await;
    h.type_in(one, "#tick disable").await;
    assert!(last_tick(&h, two).is_some(), "the count on Healer runs");

    let second = h.state.session(Some(two)).expect("the second session");
    crate::profile::switch::apply_profile_switch(
        h.app.handle(),
        &h.state,
        &second,
        DEFAULT_PROFILE_NAME,
    )
    .await
    .expect("the second session joins the first");
    assert!(!h.state.selected_profile().await.tick.config.enabled);
    assert_eq!(last_tick(&h, two), None);
    assert!(second.connection.lock().tick.in_session);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

/// The setting `prompt` types in the first game, and what it stores.
const TYPED_X: &str = "<%h/%Hhp %m/%Mmn>";
const PROMPT_X: &str = "<%h/%Hhp %m/%Mmn> ";

/// The Aabahran codes the prompt engine of `session` reads with.
fn own_codes(h: &Harness, session: SessionId) -> String {
    let session = h.state.session(Some(session)).expect("the session");
    let c = session.connection.lock();
    codes_of(&c.prompt.config().capture).0
}

/// The last row the terminal of `session` shows.
fn last_row(h: &Harness, session: SessionId) -> String {
    h.screen_of(session).pop().unwrap_or_default()
}

/// [`two_sessions_on_two_games`] with the first game on the fake game's
/// PROMPT and the second on `PROMPT_ALL`, under a profile that draws
/// `<%hp>` and follows the game from codes neither game holds. Each
/// session has drawn its first prompt.
async fn two_sessions_with_codes_of_their_own() -> (Harness, SessionId, SessionId) {
    let h = Harness::new(Options::new(Build::New)).await;
    h.servers[1].options.lock().expect("the options").prompt = PROMPT_ALL.into();
    h.set_prompt(codes("<%hhp> ")).await;
    let (h, one, two) = log_in_two_sessions(h).await;
    h.until("a drawn prompt in each session", |h| {
        last_row(h, one) == "<1020>" && last_row(h, two) == "<1020>"
    })
    .await;
    (h, one, two)
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_session_reads_its_prompt_with_the_codes_its_own_game_sent() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_codes_of_their_own().await;
    assert_eq!(own_codes(&h, one), PROMPT);
    assert_eq!(own_codes(&h, two), PROMPT_ALL);

    // The first game shows its PROMPT again, then takes a new one. The
    // profile keeps the codes of the session that saw them last, and the
    // second session reads with its own.
    h.type_in(one, "prompt").await;
    h.until("the PROMPT again", |h| shows(h, one, "Current prompt"))
        .await;
    assert_eq!(own_codes(&h, two), PROMPT_ALL);
    h.type_in(one, &format!("prompt {TYPED_X}")).await;
    h.until("the new codes", |h| own_codes(h, one) == PROMPT_X)
        .await;
    assert_eq!(codes_of(&h.capture().await).0, PROMPT_X);
    assert_eq!(own_codes(&h, two), PROMPT_ALL);
    h.type_in(two, "spam 1").await;
    h.until("a prompt drawn from the second game's codes", |h| {
        shows(h, two, "Line 1 of 1 of the spam.") && last_row(h, two) == "<1020>"
    })
    .await;

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_prompt_edit_in_one_session_reaches_the_other_which_keeps_its_codes() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_codes_of_their_own().await;

    // Settings saves a new design in the first session, and both draw it.
    let config = crate::ipc::prompt::prompt_config_get(h.app.state(), Some(one))
        .await
        .expect("the table");
    crate::ipc::prompt::prompt_config_set(
        h.app.handle().clone(),
        h.app.state(),
        vosh_prompt::PromptConfig {
            template: "<%hp>%mana".into(),
            ..config
        },
        None,
        Some(one),
    )
    .await
    .expect("the table saves");
    h.until("the new design in both sessions", |h| {
        last_row(h, one) == "<1020>800" && last_row(h, two) == "<1020>800"
    })
    .await;
    assert_eq!(own_codes(&h, two), PROMPT_ALL);
    assert_eq!(codes_of(&h.capture().await).0, PROMPT);

    // Drawing off in the first session repaints the second with its
    // game's own prompt. The line you typed closed the first session's
    // prompt, so its next one shows the game's own.
    h.type_in(one, "#prompt draw off").await;
    h.until("the second game's own prompt", |h| {
        last_row(h, two) == "<1020hp 800m 930mv>"
    })
    .await;
    h.type_in(one, "").await;
    h.until("the first game's own prompt", |h| {
        last_row(h, one) == "[1020/1020hp 800/800mn 930/930mv]"
    })
    .await;
    let second = crate::ipc::prompt::prompt_config_get(h.app.state(), Some(two))
        .await
        .expect("the second table");
    assert!(!second.draw);
    assert_eq!(second.template, "<%hp>%mana");
    assert_eq!(codes_of(&second.capture).0, PROMPT_ALL);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

/// Whether the profile `session` plays holds the alias `ww` and the
/// trigger `tt`.
async fn holds_ww_and_tt(h: &Harness, session: SessionId) -> (bool, bool) {
    let session = h.state.session(Some(session)).expect("the session");
    let p = session.lock_profile().await;
    (
        p.aliases.get("ww").is_some(),
        p.triggers.get("tt").is_some(),
    )
}

/// Whether the prompt engine of `session` draws your design.
fn draws(h: &Harness, session: SessionId) -> bool {
    let session = h.state.session(Some(session)).expect("the session");
    let draw = session.connection.lock().prompt.config().draw;
    draw
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_reset_and_load_in_one_session_reach_every_session_on_the_profile() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    // No profile claims Builder, so the first session stays on Default.
    h.servers[0].options.lock().expect("the options").name = "Builder".into();
    let (one, two) = (h.first, h.open_session().await);
    let three = open_session_on(&h, "Healer").await;
    h.connect_to(one, &h.servers[0]).await;
    h.connect_to(two, &h.servers[1]).await;
    h.until("both logins", |h| {
        shows(h, one, "Welcome to the fake Aabahran, Builder.")
            && shows(h, two, "Welcome to the fake Aabahran, Tester.")
    })
    .await;

    // Default and its file hold an alias, a trigger, the tick turned off
    // and the switch for drawing the prompt the other way from a reset.
    let reset_draw = crate::profile::file::ProfileConfig::default()
        .prompt_config()
        .draw;
    let draw_line = if reset_draw {
        "#prompt draw off"
    } else {
        "#prompt draw on"
    };
    for line in [
        "#alias ww spam 3",
        "#trigger tt {^Line 1 of} send look",
        "#tick disable",
        draw_line,
    ] {
        h.type_in(one, line).await;
    }
    crate::disk::save::tests::persist(&h.state).await;
    assert_eq!(last_tick(&h, two), None);
    assert_eq!(draws(&h, two), !reset_draw);

    h.type_in(one, "#profile reset").await;
    h.until("the line in the second session", |h| {
        shows(h, two, "Builder reset this profile to its defaults.")
    })
    .await;
    assert!(shows(&h, one, "profile reset to defaults"));
    assert!(!shows(&h, one, "reset this profile") && !shows(&h, three, "reset this profile"));
    assert_eq!(holds_ww_and_tt(&h, two).await, (false, false));
    let second = h.state.session(Some(two)).expect("the second session");
    assert_eq!(
        second.lock_profile().await.tick.config,
        crate::tick::TickConfig::default()
    );
    assert!(last_tick(&h, two).is_some(), "the second count follows");
    assert_eq!(draws(&h, two), reset_draw);

    // The hold is Default's alone. Its debounce and the quit save leave
    // its file as it was, and Healer still saves an edit.
    let (default_file, healer_file) = (
        h.profile_file(DEFAULT_PROFILE_NAME).await,
        h.profile_file("Healer").await,
    );
    let default = second.profile();
    let healer = h
        .state
        .session(Some(three))
        .expect("the third session")
        .profile();
    crate::disk::save::schedule_profile_persist(h.app.handle(), &default);
    h.type_in(three, "#alias hh spam 2").await;
    h.until("the Healer save", |_| saved_aliases(&healer_file, "hh") > 0)
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(2_500)).await;
    assert_eq!(saved_aliases(&default_file, "ww"), 1);
    assert!(default.held() && !healer.held());

    h.type_in(two, "#profile load").await;
    h.until("the line in the first session", |h| {
        shows(h, one, "Tester loaded this profile from its file.")
    })
    .await;
    assert!(shows(&h, two, "profile loaded from"));
    assert!(!shows(&h, two, "loaded this profile") && !shows(&h, three, "loaded this profile"));
    assert_eq!(holds_ww_and_tt(&h, one).await, (true, true));
    assert_eq!(draws(&h, one), !reset_draw);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

/// The harness in loadout mode. The catalog holds `kk` in the combat
/// group and `hh` in heals, Melee turns combat on and Heals turns heals
/// on, and the first session plays Default.
async fn loadout_mode() -> Harness {
    use crate::loadouts::catalog::{save_global_catalog, GlobalCatalog};
    use crate::loadouts::set::{save_loadout_set, Loadout, LoadoutSet};
    use vosh_automation::alias::Alias;
    let h = Harness::new(Options::new(Build::New)).await;
    let dir = h.dir.path();
    h.state
        .app_data
        .set(dir.to_path_buf())
        .expect("the app data folder");
    let grouped = |name: &str, group: &str| Alias {
        group: Some(group.into()),
        ..Alias::new(name, "spam 1")
    };
    let catalog = GlobalCatalog {
        aliases: vec![grouped("kk", "combat"), grouped("hh", "heals")],
        enabled_presets: Some(Vec::new()),
        ..GlobalCatalog::default()
    };
    save_global_catalog(dir, &catalog).expect("catalog.toml");
    let turns_on = |name: &str, group: &str| Loadout {
        enabled_groups: vec![group.into()],
        ..Loadout::empty(name)
    };
    let loadouts = LoadoutSet {
        loadouts: vec![turns_on("Melee", "combat"), turns_on("Heals", "heals")],
        ..LoadoutSet::default()
    };
    save_loadout_set(dir, &loadouts).expect("loadouts.toml");
    assert!(crate::app::launch::load_loadout_mode(&h.state, dir).await);
    h
}

/// Whether the profile `session` plays has its combat group on, and its
/// heals group.
async fn combat_and_heals(h: &Harness, session: SessionId) -> (bool, bool) {
    let session = h.state.session(Some(session)).expect("the session");
    let p = session.lock_profile().await;
    (
        p.aliases.is_group_enabled("combat"),
        p.aliases.is_group_enabled("heals"),
    )
}

/// What the Loadouts editor shows as on for the selected session.
async fn shown_active(h: &Harness) -> Vec<String> {
    crate::ipc::loadouts::loadouts_get_state(h.app.state())
        .await
        .expect("the loadout state")
        .active
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn a_loadout_turned_on_in_the_second_session_gates_its_own_profile_only() {
    use crate::loadouts::set::{load_loadout_set, set_active_loadouts};
    // A selection shows the session's grid, which other tests read.
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    let h = loadout_mode().await;
    let one = h.first;
    // With one profile open, the change writes the top level as before.
    set_active_loadouts(h.app.handle(), vec!["Melee".into()])
        .await
        .expect("Melee turns on");
    let saved = load_loadout_set(h.dir.path()).expect("loadouts.toml");
    assert_eq!(saved.active, ["Melee"]);
    assert!(saved.profiles.is_empty());

    // Healer opens through the top-level stack.
    let two = open_session_on(&h, "Healer").await;
    assert_eq!(combat_and_heals(&h, two).await, (true, false));
    crate::ipc::session::session_select(h.app.handle().clone(), h.app.state(), two)
        .await
        .expect("the selection moves");
    set_active_loadouts(h.app.handle(), vec!["Heals".into()])
        .await
        .expect("Heals turns on");
    assert_eq!(combat_and_heals(&h, two).await, (false, true));
    assert_eq!(combat_and_heals(&h, one).await, (true, false));
    let saved = load_loadout_set(h.dir.path()).expect("loadouts.toml");
    assert_eq!(saved.active, ["Melee"]);
    assert_eq!(saved.profiles["Healer"].active, ["Heals"]);
    assert_eq!(shown_active(&h).await, ["Heals"]);
    crate::ipc::session::session_select(h.app.handle().clone(), h.app.state(), one)
        .await
        .expect("the selection moves back");
    assert_eq!(shown_active(&h).await, ["Melee"]);
}

/// The names of the aliases in `names`, sorted.
fn sorted(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = names.into_iter().collect();
    names.sort();
    names
}

/// The aliases catalog.toml holds.
fn catalog_aliases(h: &Harness) -> Vec<String> {
    let catalog = crate::loadouts::catalog::load_global_catalog(h.dir.path()).expect("catalog");
    sorted(catalog.aliases.into_iter().map(|a| a.name))
}

/// The aliases the profile `session` plays holds.
async fn aliases_of(h: &Harness, session: SessionId) -> Vec<String> {
    let session = h.state.session(Some(session)).expect("the session");
    let p = session.lock_profile().await;
    sorted(p.aliases.list().into_iter().map(|a| a.name.clone()))
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_alias_added_in_the_first_session_reaches_healer_and_survives_its_save() {
    use crate::loadouts::set::set_active_loadouts;
    use vosh_automation::alias::Alias;
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = loadout_mode().await;
    let (one, two) = (h.first, open_session_on(&h, "Healer").await);
    // Healer keeps a stack of its own, Heals alone, and an alias no save
    // has written yet.
    crate::ipc::session::session_select(h.app.handle().clone(), h.app.state(), two)
        .await
        .expect("the selection moves");
    set_active_loadouts(h.app.handle(), vec!["Heals".into()])
        .await
        .expect("Heals turns on");
    crate::ipc::session::session_select(h.app.handle().clone(), h.app.state(), one)
        .await
        .expect("the selection moves back");
    let second = h.state.session(Some(two)).expect("the second session");
    second
        .lock_profile()
        .await
        .aliases
        .set(Alias::new("mine", "spam 3"));

    // The first session adds an alias and drops one, and Settings adds
    // one in a group of its own.
    h.type_in(one, "#alias ww spam 2").await;
    h.type_in(one, "#unalias kk").await;
    let mut loot = Alias::new("ll", "spam 1");
    loot.group = Some("loot".into());
    h.state.selected_profile().await.aliases.set(loot);
    crate::disk::save::tests::persist(&h.state).await;
    assert_eq!(catalog_aliases(&h), ["hh", "ll", "ww"]);
    assert_eq!(aliases_of(&h, two).await, ["hh", "ll", "mine", "ww"]);
    // Healer gates the new group through its own stack.
    let healer_has_loot = second.lock_profile().await.aliases.is_group_enabled("loot");
    let default_has_loot = h
        .state
        .selected_profile()
        .await
        .aliases
        .is_group_enabled("loot");
    assert!(!healer_has_loot && default_has_loot);

    // A save from Healer keeps them, and the first session takes its own.
    crate::disk::save::persist_profile(&h.state, &second.profile()).await;
    assert_eq!(catalog_aliases(&h), ["hh", "ll", "mine", "ww"]);
    assert_eq!(aliases_of(&h, one).await, ["hh", "ll", "mine", "ww"]);
    h.finish(grid).await;
}

/// Share the theme across the profiles, or keep it per profile, from the
/// selected session, as Settings does.
async fn share_theme(h: &Harness, shared: bool) {
    use crate::profile::shared::{Scope, ScopeConfig};
    let _persist_guard = crate::disk::save::PERSIST_LOCK.lock().await;
    let theme = if shared {
        Scope::Global
    } else {
        Scope::Profile
    };
    let scope = ScopeConfig {
        theme,
        ..ScopeConfig::default()
    };
    crate::profile::shared::change_scope_locked(&h.state, scope)
        .await
        .expect("the scope changes");
    crate::disk::save::persist_state(&h.state, &h.state.selected_session().profile()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn a_shared_theme_set_in_the_first_session_survives_a_save_from_the_second() {
    // A selection shows the session's grid, which other tests read.
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    for loadouts in [false, true] {
        let h = if loadouts {
            loadout_mode().await
        } else {
            let h = Harness::new(Options::new(Build::New)).await;
            h.state
                .app_data
                .set(h.dir.path().to_path_buf())
                .expect("the app data folder");
            h
        };
        let two = open_session_on(&h, "Healer").await;
        let healer = h.state.session(Some(two)).expect("the second session");
        let global_theme = || async {
            let global = h
                .state
                .profile_set
                .lock()
                .await
                .as_ref()
                .expect("the set")
                .global_path();
            crate::profile::shared::GlobalConfig::load(&global)
                .expect("global.toml")
                .theme
        };
        h.state.selected_profile().await.ui.theme = "nord".into();
        crate::disk::save::tests::persist(&h.state).await;
        assert_eq!(healer.lock_profile().await.ui.theme, "nord", "{loadouts}");
        crate::disk::save::persist_profile(&h.state, &healer.profile()).await;
        assert_eq!(global_theme().await.as_deref(), Some("nord"), "{loadouts}");

        // Each profile keeps a theme of its own, until the first session
        // shares its theme again.
        share_theme(&h, false).await;
        healer.lock_profile().await.ui.theme = "dracula".into();
        crate::disk::save::persist_profile(&h.state, &healer.profile()).await;
        h.state.selected_profile().await.ui.theme = "solarized".into();
        share_theme(&h, true).await;
        assert_eq!(
            healer.lock_profile().await.ui.theme,
            "solarized",
            "{loadouts}"
        );
        crate::disk::save::persist_profile(&h.state, &healer.profile()).await;
        assert_eq!(
            global_theme().await.as_deref(),
            Some("solarized"),
            "{loadouts}"
        );
    }
}
