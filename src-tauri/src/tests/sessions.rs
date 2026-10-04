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
use vosh_prompt::testkit::{Build, Options};

use super::fake_mud::harness::{FakeServer, Harness};
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
    let helper = plugins.join("helper");
    std::fs::create_dir_all(&helper).expect("the plugin folder");
    std::fs::write(
        helper.join("manifest.toml"),
        "[plugin]\nname = \"helper\"\n",
    )
    .expect("the manifest");
    std::fs::write(helper.join("main.lua"), HELPER).expect("the entry script");
    h.state.selected_profile().await.plugins.enabled = vec!["helper".into()];
    let first = h.state.selected_session();
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &h.state, &first, plugins).await;
    log_in_two_sessions(h).await
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
