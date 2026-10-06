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
use crate::app::events::LUA_OUTPUT;
use crate::profile::set::DEFAULT_PROFILE_NAME;
use crate::script::output::LuaKind;
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
    two_sessions_with_plugin("helper", HELPER).await
}

/// [`two_sessions_on_two_games`] with the plugin `name`, whose entry
/// script is `body`, on in the profile both play, as
/// [`two_sessions_with_a_plugin`] has [`HELPER`].
async fn two_sessions_with_plugin(name: &str, body: &str) -> (Harness, SessionId, SessionId) {
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    let plugins = crate::disk::paths::plugins_dir(h.dir.path());
    write_plugin(&plugins, name, body);
    h.state.selected_profile().await.plugins.enabled = vec![name.into()];
    let first = h.state.selected_session();
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &first, plugins).await;
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

/// The affect fulls `pairs`, as a store keeps them.
fn fulls(pairs: &[(&str, i64)]) -> crate::affects::full::FullMap {
    pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
}

/// The fulls the file keeps for `character` on the fake game `server`.
fn saved_fulls(h: &Harness, server: &FakeServer, character: &str) -> crate::affects::full::FullMap {
    let file = crate::disk::paths::affect_full_path(h.dir.path());
    let Ok(text) = std::fs::read_to_string(file) else {
        return fulls(&[]);
    };
    let table: toml::Table = text.parse().expect("the file reads");
    let key = crate::affects::full::character_key("127.0.0.1", server.port, character);
    table
        .get("characters")
        .and_then(|characters| characters.get(key.as_str()))
        .and_then(toml::Value::as_table)
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_integer()?)))
                .collect()
        })
        .unwrap_or_default()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn affect_fulls_stay_in_their_session_and_a_disconnect_writes_only_its_own() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .affect_file
        .set_path(crate::disk::paths::affect_full_path(h.dir.path()));
    {
        let mut second = h.servers[1].options.lock().expect("the options");
        second.name = "Builder".into();
        second.affects = vec![
            vosh_prompt::testkit::Affect::spell("sanctuary", 9),
            vosh_prompt::testkit::Affect::spell("fly", 20),
        ];
    }
    let (one, two) = (h.first, h.open_session().await);
    h.connect_to(one, &h.servers[0]).await;
    h.connect_to(two, &h.servers[1]).await;
    let builder = fulls(&[("fly", 20), ("sanctuary", 9)]);
    h.until("each session's login fulls", |h| {
        h.fulls_of(one) == fulls(&[("armor", 44), ("bless", 6)]) && h.fulls_of(two) == builder
    })
    .await;
    let tester = fulls(&[("armor", 48), ("bless", 6)]);
    h.type_in(one, "cast 48 armor").await;
    h.until("the recast in the first session", |h| {
        h.fulls_of(one) == tester
    })
    .await;
    assert_eq!(h.fulls_of(two), builder);

    // The second session's disconnect writes Builder's fulls and clears
    // its own map alone.
    h.disconnect_session(two).await;
    h.until("Builder's fulls in the file", |h| {
        h.fulls_of(two).is_empty() && saved_fulls(h, &h.servers[1], "Builder") == builder
    })
    .await;
    assert_eq!(h.fulls_of(one), tester);
    let shown = crate::ipc::affects::affect_full_get(h.app.state(), Some(one))
        .await
        .expect("the first session's fulls");
    assert_eq!(shown, tester);
    let written = saved_fulls(&h, &h.servers[0], "Tester");
    assert!(written.is_empty() || written == tester, "{written:?}");

    // The first session's disconnect writes Tester's, and Builder's stay.
    h.disconnect_session(one).await;
    h.until("Tester's fulls in the file", |h| {
        saved_fulls(h, &h.servers[0], "Tester") == tester
    })
    .await;
    assert_eq!(saved_fulls(&h, &h.servers[1], "Builder"), builder);
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

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_in_a_session_that_closed_leaves_its_profile_to_the_close() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let two = open_session_on(&h, "Healer").await;
    let second = h.state.session(Some(two)).expect("the second session");
    let healer = second.profile();
    // The close takes the session out of the map, then waits for its
    // connection to end. A login that names a character Default claims
    // lands in that wait.
    h.state.close_session(two).expect("the close");
    assert_eq!(
        crate::profile::switch::switch_profile(&h.state, &second, DEFAULT_PROFILE_NAME)
            .await
            .err(),
        Some(crate::sessions::NO_SUCH_SESSION.to_string())
    );
    // So the rest of the close still finds the session on Healer, open,
    // and saves Healer as it closes it.
    assert!(std::sync::Arc::ptr_eq(&second.profile(), &healer));
    assert_eq!(open_names(&h), [DEFAULT_PROFILE_NAME, "Healer"]);
    assert!(h.state.is_open(&healer));
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
        crate::ipc::session::session_rename(
            h.app.handle().clone(),
            h.app.state(),
            session,
            name.map(str::to_string),
        )
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

/// The row of `session` in the rows every window heard last.
fn heard_row(h: &Harness, session: SessionId) -> serde_json::Value {
    let lists = h.events(crate::app::events::SESSIONS_CHANGED);
    let rows = lists.last().and_then(serde_json::Value::as_array);
    rows.and_then(|rows| rows.iter().find(|row| row["id"] == json!(session)))
        .cloned()
        .unwrap_or_default()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_window_hears_a_row_follow_a_connect_a_login_a_switch_and_a_disconnect() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.connect().await;
    h.until("the login on the row", |h| {
        let row = heard_row(h, h.first);
        row["connected"] == json!(true) && row["character"] == json!("Tester")
    })
    .await;
    let row = heard_row(&h, h.first);
    assert_eq!(
        (&row["host"], &row["port"], &row["profile"]),
        (
            &json!("127.0.0.1"),
            &json!(h.port),
            &json!(DEFAULT_PROFILE_NAME)
        )
    );

    let session = h.state.selected_session();
    crate::profile::switch::apply_profile_switch(h.app.handle(), &h.state, &session, "Healer")
        .await
        .expect("the switch");
    assert_eq!(heard_row(&h, h.first)["profile"], json!("Healer"));

    // The row keeps where the session last connected, and the character
    // it played, which its dim name reads (board 3).
    h.disconnect().await;
    let row = heard_row(&h, h.first);
    assert_eq!(
        (&row["connected"], &row["character"], &row["port"]),
        (&json!(false), &json!("Tester"), &json!(h.port))
    );
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connect_tells_the_row_when_the_session_went_online_and_a_disconnect_clears_it() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let now = || {
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970");
        u64::try_from(since_epoch.as_millis()).expect("a time in range")
    };
    let before = now();
    h.connect().await;
    h.until("the time online on the row", |h| {
        heard_row(h, h.first)["since"].is_u64()
    })
    .await;
    let since = heard_row(&h, h.first)["since"].as_u64().expect("a time");
    assert!((before..=now()).contains(&since));

    h.disconnect().await;
    assert_eq!(heard_row(&h, h.first)["since"], json!(null));
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
            crate::profile::set::ProfileSetError::CannotDeletePlayed(DEFAULT_PROFILE_NAME.into())
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
async fn what_lua_sets_on_a_login_packet_saves_the_profile_the_login_leaves() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    let plugins = crate::disk::paths::plugins_dir(h.dir.path());
    write_plugin(
        &plugins,
        "greeter",
        "mud.on_gmcp('Char.Status', function() mud.set_profile_var('greeted', 'yes') end)",
    );
    h.state.selected_profile().await.plugins.enabled = vec!["greeter".into()];
    let first = h.state.selected_session();
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &first, plugins).await;
    // The second session keeps Default open, so the switch does not save
    // it as it leaves.
    let (one, two) = (h.first, h.open_session().await);
    let default_file = h.profile_file(DEFAULT_PROFILE_NAME).await;

    // Healer logs in. The Char.Status that names Healer runs Default's
    // Lua, then moves the first session to Healer before its result
    // applies.
    h.servers[0].options.lock().expect("the options").name = "Healer".into();
    h.connect_to(one, &h.servers[0]).await;
    h.until("the first session to move", |h| {
        shows(h, one, "Vosh switched to the Healer profile.")
    })
    .await;
    assert_eq!(plays(&h, two).as_deref(), Some(DEFAULT_PROFILE_NAME));
    // The value the Lua set marks Default, the profile it ran under.
    h.until("Default's save", |_| {
        saved_var(&default_file, "greeted").is_some()
    })
    .await;
    h.finish(grid).await;
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
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &first, plugins).await;
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
        None,
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

/// The characters Default lists.
async fn default_characters(h: &Harness) -> Option<Vec<String>> {
    let set = h.state.loaded_profile_set().await.expect("the set");
    let entry = set.get(DEFAULT_PROFILE_NAME).expect("Default");
    entry.auto_match.as_ref().map(|am| am.characters.clone())
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_import_over_the_profile_two_sessions_play_reaches_both_and_prints_no_line() {
    use crate::import::vosh::apply::AddAs;
    use crate::profile::file::ProfileConfig;
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    // A running tick stays on whatever a profile laid over it saved, so
    // the tick goes off first and the import turns it on again.
    h.type_in(one, "#tick disable").await;
    assert_eq!((last_tick(&h, one), last_tick(&h, two)), (None, None));
    let draw = draws(&h, one);

    // An export of Default with an alias, the tick on and the prompt
    // drawn the other way.
    let mut export = ProfileConfig::default();
    export
        .aliases
        .push(vosh_automation::alias::Alias::new("ww", "spam 3"));
    let mut prompt = export.prompt_config();
    prompt.draw = !draw;
    export.set_prompt(prompt);
    let text = export.to_toml().expect("the export");
    let result = crate::ipc::characters::profile_import_apply(
        h.app.handle().clone(),
        h.app.state(),
        "Default profile.toml".into(),
        text,
        AddAs::Replace,
        DEFAULT_PROFILE_NAME.into(),
        Vec::new(),
    )
    .await
    .expect("the import");
    assert_eq!(result.name, DEFAULT_PROFILE_NAME);

    // Each session takes the tick settings and the prompt table, and
    // neither prints the line a `#profile load` prints.
    for session in [one, two] {
        assert!(last_tick(&h, session).is_some(), "{session:?}");
        assert_eq!(draws(&h, session), !draw, "{session:?}");
        assert!(!shows(&h, session, "this profile"), "{session:?}");
        assert!(!shows(&h, session, "profile loaded"), "{session:?}");
    }
    // The alias expands in the second session, the file saved it, and
    // Default keeps its claim on Tester.
    h.type_in(two, "ww").await;
    h.until("the alias", |h| shows(h, two, "Line 3 of 3 of the spam."))
        .await;
    let file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    assert_eq!(saved_aliases(&file, "ww"), 1);
    assert_eq!(
        default_characters(&h).await,
        Some(vec!["Tester".to_string()])
    );

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
    crate::ipc::loadouts::loadouts_get_state(h.app.state(), None)
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
    set_active_loadouts(h.app.handle(), vec!["Melee".into()], None)
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
    set_active_loadouts(h.app.handle(), vec!["Heals".into()], None)
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
    set_active_loadouts(h.app.handle(), vec!["Heals".into()], None)
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

/// The harness with Tolliver on the first game, whom Default claims
/// there, and Orla on the second, whom Build claims there. Nobody has
/// connected yet, and the first session, on Default, is selected.
async fn tolliver_and_orla() -> Harness {
    use crate::profile::login_match::AutoMatch;
    let h = Harness::new(Options::new(Build::New)).await;
    let claim = |who: &str, port| AutoMatch {
        host: Some("127.0.0.1".into()),
        port: Some(port),
        characters: vec![who.into()],
        enabled: true,
    };
    for (server, who) in h.servers.iter().zip(["Tolliver", "Orla"]) {
        server.options.lock().expect("the options").name = who.into();
    }
    {
        let mut set = h.state.profile_set.lock().await;
        let set = set.as_mut().expect("the set");
        set.set_metadata(
            DEFAULT_PROFILE_NAME,
            None,
            Some(claim("Tolliver", h.servers[0].port)),
        )
        .expect("Default claims Tolliver");
        set.create("Build").expect("Build");
        set.set_metadata("Build", None, Some(claim("Orla", h.servers[1].port)))
            .expect("Build claims Orla");
    }
    h
}

/// Connect `session` to the game `server` and wait for `who` to log in.
async fn log_in(h: &Harness, session: SessionId, server: usize, who: &str) {
    h.connect_to(session, &h.servers[server]).await;
    let welcome = format!("Welcome to the fake Aabahran, {who}.");
    h.until(&format!("{who}'s login"), |h| shows(h, session, &welcome))
        .await;
}

/// Every event of `names` the app sends from here on, in order, with its
/// payload.
type Heard = std::sync::Arc<std::sync::Mutex<Vec<(&'static str, serde_json::Value)>>>;

fn hear(h: &Harness, names: &[&'static str]) -> Heard {
    use tauri::Listener;
    let heard = Heard::default();
    for &name in names {
        let keep = heard.clone();
        h.app.listen_any(name, move |event| {
            let payload = serde_json::from_str(event.payload()).expect("a JSON payload");
            keep.lock().expect("the events").push((name, payload));
        });
    }
    heard
}

/// What `heard` holds, which it then forgets.
fn take(heard: &Heard) -> Vec<(&'static str, serde_json::Value)> {
    std::mem::take(&mut *heard.lock().expect("the events"))
}

/// The events every window hears as another profile comes to the front,
/// in order.
fn profile_ui_names(h: &Harness) -> Vec<&'static str> {
    let profile = crate::profile::live::Profile::default();
    let events = crate::app::events::profile_ui_events(&h.state, &profile).events();
    events.into_iter().map(|(event, _)| event).collect()
}

/// Select `session`, as a click on its row does.
async fn select(h: &Harness, session: SessionId) {
    crate::ipc::session::session_select(h.app.handle().clone(), h.app.state(), session)
        .await
        .expect("the selection moves");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selecting_orla_hands_every_window_builds_settings_then_its_name() {
    use crate::app::events::{CHIP_STYLE_CHANGED, PANE_LAYOUT_CHANGED, PROFILE_SWITCHED};
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = tolliver_and_orla().await;
    let one = h.first;
    log_in(&h, one, 0, "Tolliver").await;
    let two = open_session_on(&h, "Build").await;
    // Build keeps a chip style of its own.
    let orla = h.state.session(Some(two)).expect("Orla's session");
    orla.lock_profile().await.ui.chip_style = "icon_value".into();
    log_in(&h, two, 1, "Orla").await;
    let mut names = profile_ui_names(&h);
    names.push(PROFILE_SWITCHED);
    let heard = hear(&h, &names);
    let payload = |heard: &[(&str, serde_json::Value)], name: &str| {
        let found = heard.iter().find(|(event, _)| *event == name);
        found
            .map(|(_, payload)| payload.clone())
            .unwrap_or_default()
    };

    // Every window takes Build's settings, then hears its name, and the
    // panes move on a generation, so a drag from Default's panes cannot
    // land on Build's.
    let generation = h.state.panes_generation();
    select(&h, two).await;
    let got = take(&heard);
    let order: Vec<&str> = got.iter().map(|(event, _)| *event).collect();
    assert_eq!(order, names);
    assert_eq!(payload(&got, CHIP_STYLE_CHANGED), json!("icon_value"));
    assert_eq!(payload(&got, PROFILE_SWITCHED), json!("Build"));
    assert_eq!(
        payload(&got, PANE_LAYOUT_CHANGED)["generation"],
        json!(generation + 1)
    );

    // Tolliver's row brings Default back.
    select(&h, one).await;
    let got = take(&heard);
    assert_eq!(got.len(), names.len(), "{got:?}");
    assert_eq!(payload(&got, CHIP_STYLE_CHANGED), json!("value_only"));
    assert_eq!(payload(&got, PROFILE_SWITCHED), json!(DEFAULT_PROFILE_NAME));

    // A session on the profile in front brings nothing new.
    let three = h.open_session().await;
    select(&h, three).await;
    let got = take(&heard);
    assert!(got.is_empty(), "{got:?}");

    // Closing Orla's session while it is selected hands the selection to
    // the session after it, which brings Default back.
    select(&h, two).await;
    take(&heard);
    h.close_session(two).await;
    assert_eq!(h.state.selected_session().id, three);
    let got = take(&heard);
    assert_eq!(got.len(), names.len(), "{got:?}");
    assert_eq!(payload(&got, PROFILE_SWITCHED), json!(DEFAULT_PROFILE_NAME));

    h.finish(grid).await;
}

/// The events that tell every window a list or the tick settings of the
/// profile in front changed.
const CHANGE_EVENTS: [&str; 8] = [
    crate::app::events::TRIGGERS_CHANGED,
    crate::app::events::ALIASES_CHANGED,
    crate::app::events::PROMPT_CONFIG_CHANGED,
    crate::app::events::MACRO_GROUPS_CHANGED,
    crate::app::events::GROUPS_CHANGED,
    crate::app::events::MACROS_CHANGED,
    crate::app::events::TIMERS_CHANGED,
    crate::app::events::TICK_CONFIG_CHANGED,
];

/// Change the lists of the profile `session` plays from that session:
/// an alias typed, one a `#lua` line makes, a macro group turned off,
/// one more alias that Lua makes as a trigger fires on a line of the
/// game, and the macros Settings saves. Then change its tick interval.
/// The profile holds a macro in the combat group.
async fn change_the_lists(h: &Harness, session: SessionId) {
    use crate::disk::save::{save_then_broadcast, SavePolicy};
    let open = h
        .state
        .session(Some(session))
        .expect("the session")
        .profile();
    open.lock().await.macros.push(crate::profile::live::Macro {
        key: "F1".into(),
        command: "kick".into(),
        group: Some("combat".into()),
        enabled: true,
        preset: None,
    });
    h.type_in(session, "#alias kk kick").await;
    h.type_in(session, "#lua mud.alias('hh', 'spam 3')").await;
    h.type_in(session, "#group combat off").await;
    h.type_in(
        session,
        "#lua mud.trigger('ss', 'Line 1 of 1 of the spam', function() mud.alias('ss', 'spam 2') end)",
    )
    .await;
    h.type_in(session, "spam 1").await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !aliases_of(h, session).await.contains(&"ss".to_string()) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the trigger's alias never came"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(aliases_of(h, session).await, ["hh", "kk", "ss"]);
    assert!(open.lock().await.disabled_macro_groups.contains("combat"));
    // Settings saves the macros of the profile it edits this way.
    let macros = open.lock().await.macros.clone();
    let events = crate::app::events::MACROS_CHANGED;
    save_then_broadcast(
        h.app.handle(),
        &h.state,
        &open,
        SavePolicy::NowUnlessHeld,
        events,
        &macros,
    )
    .await;
    h.type_in(session, "#tick interval 40").await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_list_orla_changes_on_build_behind_tells_no_window() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = tolliver_and_orla().await;
    log_in(&h, h.first, 0, "Tolliver").await;
    let two = open_session_on(&h, "Build").await;
    log_in(&h, two, 1, "Orla").await;
    let heard = hear(&h, &CHANGE_EVENTS);
    change_the_lists(&h, two).await;
    let got = take(&heard);
    assert!(got.is_empty(), "{got:?}");
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_list_maren_changes_behind_on_the_profile_in_front_tells_every_window() {
    use crate::app::events::{
        ALIASES_CHANGED, GROUPS_CHANGED, MACROS_CHANGED, MACRO_GROUPS_CHANGED, TICK_CONFIG_CHANGED,
    };
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = tolliver_and_orla().await;
    log_in(&h, h.first, 0, "Tolliver").await;
    // Maren, whom no profile claims, plays Default beside Tolliver.
    h.servers[1].options.lock().expect("the options").name = "Maren".into();
    let two = h.open_session().await;
    log_in(&h, two, 1, "Maren").await;
    assert_eq!(plays(&h, two).as_deref(), Some(DEFAULT_PROFILE_NAME));
    let heard = hear(&h, &CHANGE_EVENTS);
    change_the_lists(&h, two).await;
    let got: Vec<&str> = take(&heard).into_iter().map(|(event, _)| event).collect();
    let count = |name| got.iter().filter(|event| **event == name).count();
    assert_eq!(count(ALIASES_CHANGED), 3, "{got:?}");
    assert_eq!(count(MACRO_GROUPS_CHANGED), 1, "{got:?}");
    assert_eq!(count(GROUPS_CHANGED), 1, "{got:?}");
    assert_eq!(count(MACROS_CHANGED), 1, "{got:?}");
    assert_eq!(count(TICK_CONFIG_CHANGED), 1, "{got:?}");
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_that_moves_a_session_behind_tells_every_window_its_row_alone() {
    use crate::app::events::{PROFILE_SWITCHED, SESSIONS_CHANGED, SESSION_IDENTITY_CHANGED};
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = tolliver_and_orla().await;
    log_in(&h, h.first, 0, "Tolliver").await;
    let two = h.open_session().await;
    let mut names = profile_ui_names(&h);
    names.extend([PROFILE_SWITCHED, SESSION_IDENTITY_CHANGED, SESSIONS_CHANGED]);
    let heard = hear(&h, &names);

    // Orla logs in on Default, and Build, which claims her, takes her
    // session behind the one in front.
    log_in(&h, two, 1, "Orla").await;
    h.until("the switch to Build", |h| {
        shows(h, two, "Vosh switched to the Build profile.")
    })
    .await;
    h.until("the row on Build", |h| {
        heard_row(h, two)["profile"] == json!("Build")
    })
    .await;
    let got: Vec<&str> = take(&heard).into_iter().map(|(event, _)| event).collect();
    assert!(
        got.iter().all(|event| *event == SESSIONS_CHANGED),
        "{got:?}"
    );
    assert_eq!(plays(&h, h.first).as_deref(), Some(DEFAULT_PROFILE_NAME));
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn who_is_logged_in_goes_out_for_the_selected_session_and_again_on_a_selection() {
    use crate::app::events::SESSION_IDENTITY_CHANGED;
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = tolliver_and_orla().await;
    let heard = hear(&h, &[SESSION_IDENTITY_CHANGED]);
    let character = |heard: &Heard| {
        let heard = heard.lock().expect("the events");
        heard
            .last()
            .map(|(_, identity)| identity["character"].clone())
    };
    let (one, two) = (h.first, open_session_on(&h, "Build").await);
    log_in(&h, one, 0, "Tolliver").await;
    h.until("Tolliver named", |_| {
        character(&heard) == Some(json!("Tolliver"))
    })
    .await;

    // Orla logs in behind, and no window hears of her.
    log_in(&h, two, 1, "Orla").await;
    h.until("the row with Orla", |h| {
        heard_row(h, two)["character"] == json!("Orla")
    })
    .await;
    let got = take(&heard);
    assert!(
        got.iter()
            .all(|(_, identity)| identity["character"] != json!("Orla")),
        "{got:?}"
    );

    // Her row brings her to the front.
    select(&h, two).await;
    let got = take(&heard);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(
        (&got[0].1["character"], &got[0].1["profile"]),
        (&json!("Orla"), &json!("Build"))
    );

    // Tolliver leaves behind, and no window hears of it until his row
    // comes to the front again, with nobody logged in.
    h.disconnect_session(one).await;
    let got = take(&heard);
    assert!(got.is_empty(), "{got:?}");
    select(&h, one).await;
    let got = take(&heard);
    assert_eq!(got, [(SESSION_IDENTITY_CHANGED, serde_json::Value::Null)]);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

/// The names of the triggers the profile file at `file` holds.
fn saved_triggers(file: &std::path::Path) -> Vec<String> {
    crate::profile::file::ProfileConfig::load(file)
        .map(|config| config.triggers.into_iter().map(|t| t.name).collect())
        .unwrap_or_default()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trigger_list_that_names_build_saves_build_and_tells_no_window_while_default_shows() {
    use crate::app::events::TRIGGERS_CHANGED;
    use crate::ipc::automation::triggers_import;
    use vosh_automation::trigger::{Trigger, TriggerAction, TriggerStore};
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = tolliver_and_orla().await;
    let two = open_session_on(&h, "Build").await;
    assert_eq!(h.state.selected_session().id, h.first);
    let mut list = TriggerStore::default();
    list.set(Trigger::new(
        "spam",
        "Line 1 of 1 of the spam",
        TriggerAction::Gag,
    ))
    .expect("the trigger");
    let json = list.export_json().expect("the list");
    let default_before = h.state.selected_profile().await.triggers.list();
    let healer_file = h.profile_file("Healer").await;
    let healer_before = std::fs::read_to_string(&healer_file).ok();
    let heard = hear(&h, &[TRIGGERS_CHANGED]);

    let import = |profile: &str| {
        let profile = Some(profile.to_string());
        triggers_import(h.app.handle().clone(), h.app.state(), json.clone(), profile)
    };
    assert_eq!(import("Build").await, Ok(1));
    let build = h.state.session(Some(two)).expect("the Build session");
    let names: Vec<String> = {
        let p = build.lock_profile().await;
        p.triggers.list().into_iter().map(|t| t.name).collect()
    };
    assert_eq!(names, ["spam"]);
    assert_eq!(saved_triggers(&h.profile_file("Build").await), ["spam"]);
    assert_eq!(
        h.state.selected_profile().await.triggers.list(),
        default_before
    );
    let got = take(&heard);
    assert!(got.is_empty(), "{got:?}");

    // Healer is a profile no session plays, so nothing changes.
    assert_eq!(
        import("Healer").await,
        Err("Healer closed before Vosh could save this change.".to_string())
    );
    assert_eq!(std::fs::read_to_string(&healer_file).ok(), healer_before);
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_settings_holds_stays_open_past_its_last_session_until_save_lets_go() {
    use crate::ipc::automation::triggers_import;
    use crate::ipc::profiles::hold_edits;
    use vosh_automation::trigger::{Trigger, TriggerAction, TriggerStore};
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let two = open_session_on(&h, "Healer").await;
    let mut list = TriggerStore::default();
    list.set(Trigger::new("spam", "Maren tells you", TriggerAction::Gag))
        .expect("the trigger");
    let json = list.export_json().expect("the list");
    let save = || {
        let profile = Some("Healer".to_string());
        triggers_import(h.app.handle().clone(), h.app.state(), json.clone(), profile)
    };

    // A page holds unsaved edits on Healer as its last session closes.
    hold_edits(&h.state, Some("Healer")).await;
    crate::ipc::session::session_close(h.app.handle().clone(), h.app.state(), two)
        .await
        .expect("the second session closes");
    assert_eq!(open_names(&h), [DEFAULT_PROFILE_NAME, "Healer"]);

    // Save still finds it, and writes its file.
    assert_eq!(save().await, Ok(1));
    assert_eq!(saved_triggers(&h.profile_file("Healer").await), ["spam"]);

    // Letting go closes it, since no session plays it.
    hold_edits(&h.state, None).await;
    assert_eq!(open_names(&h), [DEFAULT_PROFILE_NAME]);
    assert_eq!(
        save().await,
        Err("Healer closed before Vosh could save this change.".to_string())
    );
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_settings_lets_go_of_stays_open_while_a_session_plays_it() {
    use crate::ipc::profiles::hold_edits;
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let _two = open_session_on(&h, "Healer").await;
    hold_edits(&h.state, Some("Healer")).await;
    hold_edits(&h.state, None).await;
    assert_eq!(open_names(&h), [DEFAULT_PROFILE_NAME, "Healer"]);
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_alert_preset_that_names_build_saves_build_while_default_shows() {
    use crate::alert::presets::TELLS;
    use crate::ipc::alerts::{alert_presets_get, alert_presets_set};
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = tolliver_and_orla().await;
    let two = open_session_on(&h, "Build").await;
    assert_eq!(h.state.selected_session().id, h.first);
    let parts = crate::alert::AlertParts {
        sound: Some("bell".into()),
        ..crate::alert::AlertParts::default()
    };
    let set = |alert, profile: &str| {
        let profile = Some(profile.to_string());
        alert_presets_set(
            h.app.handle().clone(),
            h.app.state(),
            TELLS.into(),
            alert,
            profile,
        )
    };
    assert_eq!(set(Some(parts.clone()), "Build").await, Ok(()));

    let build = h.state.session(Some(two)).expect("the Build session");
    assert_eq!(build.lock_profile().await.alerts.get(TELLS), Some(&parts));
    assert!(!h.state.selected_profile().await.alerts.contains_key(TELLS));
    let read =
        |profile: Option<&str>| alert_presets_get(h.app.state(), profile.map(str::to_string));
    let named = read(Some("Build")).await.expect("Build's presets");
    assert_eq!(named.alerts.get(TELLS), Some(&parts));
    let shown = read(None).await.expect("Default's presets");
    assert!(!shown.alerts.contains_key(TELLS));

    // Healer is a profile no session plays, so nothing changes.
    assert_eq!(
        set(None, "Healer").await,
        Err("Healer closed before Vosh could save this change.".to_string())
    );
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tick_settings_that_name_build_reach_every_count_on_build_alone() {
    use crate::app::events::TICK_CONFIG_CHANGED;
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = tolliver_and_orla().await;
    let one = h.first;
    log_in(&h, one, 0, "Tolliver").await;
    let two = open_session_on(&h, "Build").await;
    log_in(&h, two, 1, "Orla").await;
    // Maren, whom no profile claims, plays Build beside Orla.
    h.servers[1].options.lock().expect("the options").name = "Maren".into();
    let three = open_session_on(&h, "Build").await;
    log_in(&h, three, 1, "Maren").await;
    assert_eq!(plays(&h, three).as_deref(), Some("Build"));
    assert_eq!(h.state.selected_session().id, one);
    for session in [one, two, three] {
        assert!(last_tick(&h, session).is_some(), "{session} counts");
    }
    let heard = hear(&h, &[TICK_CONFIG_CHANGED]);

    let build = h.state.open_profile("Build").expect("Build is open");
    let config = build.lock().await.tick.config.clone();
    let saved = crate::ipc::tick::tick_set_config(
        h.app.handle().clone(),
        h.app.state(),
        crate::tick::TickConfig {
            enabled: false,
            ..config
        },
        Some("Build".into()),
    )
    .await
    .expect("the settings apply");
    assert!(!saved.enabled);
    // Both counts on Build stop, and Tolliver's on Default runs on.
    assert_eq!(last_tick(&h, two), None);
    assert_eq!(last_tick(&h, three), None);
    assert!(last_tick(&h, one).is_some());
    assert!(h.state.selected_profile().await.tick.config.enabled);
    let file = h.profile_file("Build").await;
    assert_eq!(saved_tick(&file).map(|tick| tick.enabled), Some(false));
    let got = take(&heard);
    assert!(got.is_empty(), "{got:?}");

    h.disconnect_session(two).await;
    h.disconnect_session(three).await;
    h.finish(grid).await;
}

/// A plugin with a global of its own that prints a line at the end of
/// `spam 1`.
const TELLER: &str = "\
    greeting = 'from the plugin'\n\
    mud.trigger('end', 'Line 1 of 1 of the spam', function() print('The spam ended.') end)";

/// The Output ring of `session`, as the Scripts page reads it, each line
/// as its owner, kind and text.
async fn output_of(h: &Harness, session: SessionId) -> Vec<(String, LuaKind, String)> {
    crate::ipc::scripts::lua_output_get(h.app.state(), Some(session))
        .await
        .expect("the ring")
        .into_iter()
        .map(|line| (line.owner, line.kind, line.text))
        .collect()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_session_keeps_the_lua_lines_of_its_own_plugins() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_plugin("teller", TELLER).await;
    h.type_in(one, "spam 1").await;
    h.until("the plugin to print in the first session", |h| {
        shows(h, one, "[lua] The spam ended.")
    })
    .await;

    let printed = (
        "plugin:teller".to_string(),
        LuaKind::Print,
        "The spam ended.".to_string(),
    );
    assert_eq!(output_of(&h, one).await, [printed]);
    let leftover = &output_of(&h, two).await;
    assert!(leftover.is_empty(), "{leftover:?}");
    // The page hears the line from the first session alone.
    let heard = h.events_of(one, LUA_OUTPUT);
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert_eq!(heard[0]["lines"][0]["text"], "The spam ended.");
    let leftover = &h.events_of(two, LUA_OUTPUT);
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(!shows(&h, two, "The spam ended."));

    // Clearing the plugin's lines clears them in that session.
    crate::ipc::scripts::lua_output_clear(h.app.state(), Some("plugin:teller".into()), Some(one))
        .await
        .expect("the clear");
    let leftover = &output_of(&h, one).await;
    assert!(leftover.is_empty(), "{leftover:?}");

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_console_runs_inside_a_plugin_or_in_the_global_environment() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_with_plugin("teller", TELLER).await;
    let run = |code: &str, plugin: Option<&str>, session| {
        crate::ipc::scripts::lua_run(
            h.app.handle().clone(),
            h.app.state(),
            code.to_string(),
            plugin.map(str::to_string),
            Some(session),
        )
    };
    run("greeting = 'typed in the first'", None, one)
        .await
        .expect("the run");
    run("print(greeting)", Some("teller"), one)
        .await
        .expect("the run");
    run("print(greeting)", None, one).await.expect("the run");
    run("print(greeting)", None, two).await.expect("the run");

    let line = |owner: &str, kind, text: &str| (owner.to_string(), kind, text.to_string());
    assert_eq!(
        output_of(&h, one).await,
        [
            line("#lua", LuaKind::Input, "greeting = 'typed in the first'"),
            line("plugin:teller", LuaKind::Input, "print(greeting)"),
            line("plugin:teller", LuaKind::Print, "from the plugin"),
            line("#lua", LuaKind::Input, "print(greeting)"),
            line("#lua", LuaKind::Print, "typed in the first"),
        ]
    );
    // The second session has a global environment of its own.
    assert_eq!(
        output_of(&h, two).await,
        [
            line("#lua", LuaKind::Input, "print(greeting)"),
            line("#lua", LuaKind::Print, "nil"),
        ]
    );
    // What it prints shows in the terminal too, and the line you typed
    // does not.
    h.until("the plugin's answer in the first terminal", |h| {
        shows(h, one, "[lua] from the plugin") && shows(h, one, "[lua] typed in the first")
    })
    .await;
    assert!(!shows(&h, one, "print(greeting)"));
    assert!(!shows(&h, two, "from the plugin"));

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

/// Turn the plugin `name` on or off from the Scripts page with `session`
/// selected there, and return the list as it then shows it.
async fn switch_plugin(
    h: &Harness,
    name: &str,
    on: bool,
    session: SessionId,
) -> Vec<crate::ipc::scripts::PluginRow> {
    crate::ipc::scripts::plugin_set_enabled(
        h.app.handle().clone(),
        h.app.state(),
        name.into(),
        on,
        Some(session),
    )
    .await
    .expect("the switch")
}

/// The row of the plugin `name` in the list `session` sees.
async fn plugin_row(h: &Harness, name: &str, session: SessionId) -> crate::ipc::scripts::PluginRow {
    crate::ipc::scripts::plugins_list(h.app.state(), Some(session))
        .await
        .expect("the list")
        .into_iter()
        .find(|row| row.name == name)
        .expect("the plugin's row")
}

/// The harness with its app data folder in its temporary folder, and the
/// plugins folder it holds.
async fn harness_with_plugins() -> (Harness, std::path::PathBuf) {
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    let plugins = crate::disk::paths::plugins_dir(h.dir.path());
    (h, plugins)
}

/// A plugin that registers one of each thing a plugin can: a trigger, a
/// timer and an alias.
const KEEPER: &str = "\
    mud.trigger('hunger', 'You are hungry.', function() end)\n\
    mud.timer(600, function() end)\n\
    mud.alias('kk', 'look')";

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plugin_turned_on_in_one_session_loads_in_every_session_on_its_profile_only() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, plugins) = harness_with_plugins().await;
    write_plugin(&plugins, "helper", HELPER);
    write_plugin(&plugins, "keeper", KEEPER);
    h.state.selected_profile().await.plugins.enabled = vec!["helper".into()];
    let first = h.state.selected_session();
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &first, plugins).await;
    let (one, two) = (h.first, h.open_session().await);
    let three = open_session_on(&h, "Healer").await;

    let rows = switch_plugin(&h, "keeper", true, two).await;
    let keeper = rows
        .iter()
        .find(|row| row.name == "keeper")
        .expect("its row");
    assert!(keeper.on && keeper.stopped.is_none(), "{keeper:?}");
    // Both sessions on Default load it, each into its own engine, and
    // the session on Healer does not.
    assert_eq!(plugins_of(&h, one), ["helper", "keeper"]);
    assert_eq!(plugins_of(&h, two), ["helper", "keeper"]);
    let leftover = &plugins_of(&h, three);
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(!plugin_row(&h, "keeper", three).await.on);
    // Default keeps its order with the new plugin last, and saves it.
    let default_file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    let saved = crate::profile::file::ProfileConfig::load(&default_file).expect("Default's file");
    assert_eq!(saved.plugins.enabled, ["helper", "keeper"]);
    let third = h.state.session(Some(three)).expect("the third session");
    let leftover = &third.lock_profile().await.plugins.enabled;
    assert!(leftover.is_empty(), "{leftover:?}");

    h.finish(grid).await;
}

/// The Lua triggers, the count of Lua timers and the plugin aliases
/// `session` holds.
async fn registered(h: &Harness, session: SessionId) -> (Vec<String>, usize, Vec<String>) {
    let session = h.state.session(Some(session)).expect("the session");
    let timers = session.lua_timers.lock().await.len();
    let c = session.connection.lock();
    let triggers = c
        .script
        .lua_triggers()
        .into_iter()
        .map(|t| t.name)
        .collect();
    let aliases = c
        .plugin_aliases
        .list()
        .into_iter()
        .map(|(_, alias)| alias.name.clone())
        .collect();
    (triggers, timers, aliases)
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turning_a_plugin_off_takes_back_its_triggers_timers_and_aliases() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, plugins) = harness_with_plugins().await;
    write_plugin(&plugins, "keeper", KEEPER);
    let (one, two) = (h.first, h.open_session().await);

    switch_plugin(&h, "keeper", true, one).await;
    for id in [one, two] {
        assert_eq!(
            registered(&h, id).await,
            (vec!["hunger".into()], 1, vec!["kk".into()])
        );
    }
    switch_plugin(&h, "keeper", false, one).await;
    for id in [one, two] {
        assert_eq!(registered(&h, id).await, (Vec::new(), 0, Vec::new()));
        let leftover = &plugins_of(&h, id);
        assert!(leftover.is_empty(), "{leftover:?}");
    }
    assert!(!plugin_row(&h, "keeper", one).await.on);

    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plugin_folder_named_by_hand_shows_on_the_list_and_turns_off() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, plugins) = harness_with_plugins().await;
    // Made by hand, as Help once told you to, with a hyphen New plugin
    // refuses. Launch loads it all the same.
    write_plugin(&plugins, "weather-pane", KEEPER);
    h.state.selected_profile().await.plugins.enabled = vec!["weather-pane".into()];
    let first = h.state.selected_session();
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &first, plugins).await;
    let one = h.first;
    assert_eq!(plugins_of(&h, one), ["weather-pane"]);

    let row = plugin_row(&h, "weather-pane", one).await;
    assert!(row.on && row.misnamed, "{row:?}");
    let rows = switch_plugin(&h, "weather-pane", false, one).await;
    let row = rows
        .iter()
        .find(|row| row.name == "weather-pane")
        .expect("its row");
    assert!(!row.on, "{row:?}");
    assert_eq!(registered(&h, one).await, (Vec::new(), 0, Vec::new()));
    let leftover = &plugins_of(&h, one);
    assert!(leftover.is_empty(), "{leftover:?}");
    // It turns on again only once its folder keeps the rule.
    let refused = crate::ipc::scripts::plugin_set_enabled(
        h.app.handle().clone(),
        h.app.state(),
        "weather-pane".into(),
        true,
        Some(one),
    )
    .await;
    assert_eq!(
        refused,
        Err(crate::app::plugins::folder::NAME_RULE.to_string())
    );

    h.finish(grid).await;
}

/// `wait_full` as the Scripts design writes it, which never returns
/// while you are hurt.
const WAIT_FULL: &str = "-- wait_full
-- Stand up once your hit points are full.

mud.on_gmcp(\"Char.Vitals\", function(data)
  while data.hp < data.maxhp do
    -- data never changes inside this loop, so it never ends
  end
  mud.send(\"stand\")
end)
";

/// `wait_full` once it waits for the next packet instead.
const WAIT_FULL_FIXED: &str = "-- wait_full
-- Stand up once your hit points are full.

mud.on_gmcp(\"Char.Vitals\", function(data)
  if data.hp >= data.maxhp then
    mud.send(\"stand\")
  end
end)
";

/// Two sessions on Default with `wait_full` made from the Scripts page,
/// on, and saved as the design writes it, then stopped in the first
/// session by a Char.Vitals that finds you hurt.
async fn wait_full_stopped_in_the_first() -> (Harness, SessionId, SessionId) {
    let (h, _) = harness_with_plugins().await;
    let (one, two) = (h.first, h.open_session().await);
    crate::ipc::scripts::plugin_create(
        h.app.handle().clone(),
        h.app.state(),
        "wait_full".into(),
        Some(one),
    )
    .await
    .expect("New plugin");
    save_wait_full(&h, WAIT_FULL, one).await;
    let session = h.state.session(Some(one)).expect("the session");
    let apply = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        let hurt = serde_json::json!({"hp": 186, "maxhp": 1020});
        let stopped = c.script.dispatch_gmcp("Char.Vitals", &hurt);
        crate::script::apply_actions(&mut p, &mut c, stopped).ran_under(p.open())
    };
    crate::session::effects::deliver_detached(h.app.handle(), &session, apply).await;
    (h, one, two)
}

/// Save `code` to `wait_full` from the Scripts page with `session`
/// selected there.
async fn save_wait_full(
    h: &Harness,
    code: &str,
    session: SessionId,
) -> Vec<crate::ipc::scripts::PluginRow> {
    let folder = crate::ipc::scripts::plugin_read(h.app.state(), "wait_full".into(), None)
        .await
        .expect("the plugin");
    crate::ipc::scripts::plugin_save(
        h.app.handle().clone(),
        h.app.state(),
        "wait_full".into(),
        folder.manifest,
        code.into(),
        Some(session),
    )
    .await
    .expect("the save")
}

/// Whether Vosh holds `wait_full` off in `session` after a stop.
fn wait_full_stopped(h: &Harness, session: SessionId) -> bool {
    let session = h.state.session(Some(session)).expect("the session");
    let c = session.connection.lock();
    c.script
        .is_stopped(&vosh_script::Owner::Plugin("wait_full".into()))
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_plugin_stays_stopped_across_its_switch_off_and_on() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = wait_full_stopped_in_the_first().await;
    assert!(wait_full_stopped(&h, one));
    let stopped = Some(crate::ipc::scripts::PluginStop::Time);
    assert_eq!(plugin_row(&h, "wait_full", one).await.stopped, stopped);

    switch_plugin(&h, "wait_full", false, one).await;
    let rows = switch_plugin(&h, "wait_full", true, one).await;
    // On for the profile, and still off in the session that stopped it,
    // with nothing it registered, until a save or a restart.
    assert!(wait_full_stopped(&h, one));
    let leftover = &plugins_of(&h, one);
    assert!(leftover.is_empty(), "{leftover:?}");
    let row = rows
        .iter()
        .find(|row| row.name == "wait_full")
        .expect("its row");
    assert!(row.on, "{row:?}");
    assert_eq!(row.stopped, stopped);
    // The other session runs it.
    assert!(!wait_full_stopped(&h, two));
    assert_eq!(plugins_of(&h, two), ["wait_full"]);
    assert_eq!(plugin_row(&h, "wait_full", two).await.stopped, None);

    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_save_reloads_a_stopped_plugin_and_says_so_in_its_output() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = wait_full_stopped_in_the_first().await;
    assert!(wait_full_stopped(&h, one));

    let rows = save_wait_full(&h, WAIT_FULL_FIXED, one).await;
    let row = rows
        .iter()
        .find(|row| row.name == "wait_full")
        .expect("its row");
    assert_eq!(row.stopped, None);
    assert!(!wait_full_stopped(&h, one));
    assert_eq!(plugins_of(&h, one), ["wait_full"]);
    // The row says when it loaded again, no sooner than the stop, so the
    // page marks the stopped line no more.
    let ring = crate::ipc::scripts::lua_output_get(h.app.state(), Some(one))
        .await
        .expect("the ring");
    let stop = ring
        .iter()
        .find(|line| line.kind == LuaKind::Error)
        .expect("the stop");
    assert!(
        row.loaded_ms.is_some_and(|loaded| loaded >= stop.ts_ms),
        "{row:?}"
    );
    let reloaded = (
        "plugin:wait_full".to_string(),
        LuaKind::Note,
        "Vosh reloaded wait_full.".to_string(),
    );
    // The stop, then the reload, in the session that stopped it.
    let output = output_of(&h, one).await;
    let kinds: Vec<LuaKind> = output.iter().map(|(_, kind, _)| *kind).collect();
    assert_eq!(
        kinds,
        [LuaKind::Note, LuaKind::Error, LuaKind::Note, LuaKind::Note]
    );
    assert_eq!(output.last(), Some(&reloaded));
    h.until("the reload in the first terminal", |h| {
        shows(h, one, "[lua] Vosh reloaded wait_full.")
    })
    .await;
    // The other session runs it too, so it reloads there as well.
    assert_eq!(output_of(&h, two).await.last(), Some(&reloaded));

    h.finish(grid).await;
}

/// Two sessions on Default, where `helper` and `keeper` 0.1.0 are on,
/// with Healer's file turning `keeper` on too and Warrior's turning on
/// only `helper`. Returns the plugins folder and the files of Healer and
/// Warrior.
async fn keeper_on_in_two_profiles() -> (
    Harness,
    SessionId,
    SessionId,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (h, plugins) = harness_with_plugins().await;
    write_plugin(&plugins, "helper", HELPER);
    write_plugin(&plugins, "keeper", KEEPER);
    std::fs::write(
        plugins.join("keeper").join("manifest.toml"),
        "[plugin]\nname = \"keeper\"\nversion = \"0.1.0\"\n",
    )
    .expect("keeper's manifest");
    let (one, two) = (h.first, h.open_session().await);
    switch_plugin(&h, "helper", true, one).await;
    switch_plugin(&h, "keeper", true, one).await;
    h.state
        .profile_set
        .lock()
        .await
        .as_mut()
        .expect("the set")
        .create("Warrior")
        .expect("Warrior");
    let mut files = Vec::new();
    for (profile, enabled) in [
        ("Healer", vec!["keeper", "helper"]),
        ("Warrior", vec!["helper"]),
    ] {
        let file = h.profile_file(profile).await;
        let mut config = crate::profile::file::ProfileConfig::default();
        config.plugins.enabled = enabled.into_iter().map(str::to_string).collect();
        config.save(&file).expect("the profile file");
        files.push(file);
    }
    for id in [one, two] {
        assert_eq!(plugins_of(&h, id), ["helper", "keeper"]);
    }
    let warrior = files.pop().expect("Warrior's file");
    let healer = files.pop().expect("Healer's file");
    (h, one, two, plugins, healer, warrior)
}

/// The plugins a profile file turns on.
fn saved_plugins(file: &std::path::Path) -> Vec<String> {
    crate::profile::file::ProfileConfig::load(file)
        .expect("the profile file")
        .plugins
        .enabled
}

/// `keeper` 0.2.0 from Orla, in a .zip of one folder.
fn keeper_zip() -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let manifest = "[plugin]\nname = \"keeper\"\nversion = \"0.2.0\"\nauthor = \"Orla\"\n";
    for (name, text) in [
        ("keeper/manifest.toml", manifest),
        ("keeper/main.lua", "mud.alias('kk', 'look')"),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .expect("an entry");
        zip.write_all(text.as_bytes()).expect("its bytes");
    }
    zip.finish().expect("the zip").into_inner()
}

/// Whether `session` holds the trigger `keeper` 0.1.0 makes, with the
/// count of its Lua timers and its plugin aliases.
async fn keeper_left(h: &Harness, session: SessionId) -> (bool, usize, Vec<String>) {
    let (triggers, timers, aliases) = registered(h, session).await;
    (triggers.contains(&"hunger".to_string()), timers, aliases)
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn installing_over_a_plugin_turns_it_off_in_every_profile_first() {
    use crate::ipc::scripts::{InstalledPlugin, PluginInstallCheck};
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two, plugins, healer, warrior) = keeper_on_in_two_profiles().await;

    let check = crate::ipc::scripts::plugin_install_check(
        h.app.state(),
        "keeper.zip".into(),
        Some(keeper_zip()),
        None,
    )
    .await
    .expect("the check");
    assert_eq!(
        check,
        PluginInstallCheck {
            name: "keeper".into(),
            version: "0.2.0".into(),
            author: "Orla".into(),
            existing: Some(InstalledPlugin {
                version: "0.1.0".into(),
                on_in: vec!["Default".into(), "Healer".into()],
            }),
        }
    );

    let rows = crate::ipc::scripts::plugin_install(
        h.app.handle().clone(),
        h.app.state(),
        "keeper.zip".into(),
        Some(keeper_zip()),
        None,
        Some(two),
    )
    .await
    .expect("the install");
    let keeper = rows
        .iter()
        .find(|row| row.name == "keeper")
        .expect("its row");
    assert_eq!((keeper.on, keeper.version.as_str()), (false, "0.2.0"));
    // Both sessions on Default let go of all it registered, and keep
    // helper running.
    for id in [one, two] {
        assert_eq!(plugins_of(&h, id), ["helper"]);
        assert_eq!(keeper_left(&h, id).await, (false, 0, Vec::new()));
    }
    // Default drops it in memory and in its file, Healer in its file,
    // and Warrior, which never turned it on, keeps its file as it was.
    assert_eq!(h.state.selected_profile().await.plugins.enabled, ["helper"]);
    let default_file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    assert_eq!(saved_plugins(&default_file), ["helper"]);
    assert_eq!(saved_plugins(&healer), ["helper"]);
    assert_eq!(saved_plugins(&warrior), ["helper"]);
    assert_eq!(backups(&warrior), 0);
    // The new code is in place and runs once you turn it on.
    assert_eq!(
        std::fs::read_to_string(plugins.join("keeper").join("main.lua")).expect("main.lua"),
        "mud.alias('kk', 'look')"
    );
    switch_plugin(&h, "keeper", true, one).await;
    for id in [one, two] {
        assert_eq!(keeper_left(&h, id).await, (false, 0, vec!["kk".into()]));
    }

    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_a_plugin_clears_every_list_and_its_folder() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two, plugins, healer, _) = keeper_on_in_two_profiles().await;
    let remove = |name: &str| {
        crate::ipc::scripts::plugin_remove(
            h.app.handle().clone(),
            h.app.state(),
            name.into(),
            Some(one),
        )
    };

    let rows = remove("keeper").await.expect("the remove");
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["helper"]);
    for id in [one, two] {
        assert_eq!(plugins_of(&h, id), ["helper"]);
        assert_eq!(keeper_left(&h, id).await, (false, 0, Vec::new()));
    }
    assert_eq!(h.state.selected_profile().await.plugins.enabled, ["helper"]);
    let default_file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    assert_eq!(saved_plugins(&default_file), ["helper"]);
    assert_eq!(saved_plugins(&healer), ["helper"]);
    assert!(!plugins.join("keeper").exists());
    assert!(plugins.join("helper").is_dir());
    assert_eq!(
        remove("keeper").await.expect_err("nothing left"),
        "You have no plugin named keeper."
    );

    h.finish(grid).await;
}
