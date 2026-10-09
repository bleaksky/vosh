//! A pane a plugin draws, through the real session against the fake
//! game: the packets of one read send it once, and turning the plugin
//! off empties it at once.

use serde_json::{json, Value as Json};
use tauri::Manager;
use vosh_prompt::testkit::{gmcp, Build, Options};

use super::fake_mud::harness::Harness;
use crate::app::events::LUA_PANES;

/// The weather pane of the Scripts and Panels board, filled from the
/// Room.Weather and Char.State that come with every prompt.
const WEATHER_PANE: &str = "local pane = mud.pane('weather', 'Weather')
local sky, temp, position, language = '', '', '', ''
local function draw()
  pane:set({
    { row = { 'Sky', sky } },
    { row = { 'Temperature', temp } },
    { row = { 'Position', position } },
    { row = { 'Language', language } },
  })
end
mud.on_gmcp('Room.Weather', function(d)
  sky, temp = d.sky, d.temp .. ' ' .. d.unit
  pane:meta(d.region)
  draw()
end)
mud.on_gmcp('Char.State', function(d)
  position, language = d.position, d.language
  draw()
end)";

/// The packet a fixture holds, as the game writes it.
fn packet(text: &str) -> Vec<u8> {
    let (package, json) = text.trim().split_once(' ').expect("a packet");
    gmcp(package, json)
}

/// Turn the plugin `name` on or off in the first session, and deliver
/// what it asks for.
async fn plugin(h: &Harness, name: &str, on: bool) {
    let session = h.state.selected_session();
    let plugins = crate::disk::paths::plugins_dir(h.dir.path());
    let apply = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        let apply = if on {
            crate::app::plugins::plugin_on(&mut p, &mut c, &plugins, name)
        } else {
            crate::app::plugins::plugin_off(&mut p, &mut c, name)
        };
        apply.ran_under(p.open())
    };
    crate::app::plugins::follow_profile(h.app.handle(), &session, apply).await;
}

fn sent(h: &Harness) -> Vec<Json> {
    h.events_of(h.first, LUA_PANES)
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_packets_of_one_read_send_the_pane_once_and_turning_it_off_empties_it() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options {
        name: "Orla".into(),
        ..Options::new(Build::New)
    })
    .await;
    let folder = crate::disk::paths::plugins_dir(h.dir.path()).join("weather_pane");
    std::fs::create_dir_all(&folder).expect("the plugin folder");
    std::fs::write(
        folder.join("manifest.toml"),
        "[plugin]\nname = \"weather_pane\"\n",
    )
    .expect("the manifest");
    std::fs::write(folder.join("main.lua"), WEATHER_PANE).expect("the entry script");
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Orla.").await;
    plugin(&h, "weather_pane", true).await;
    // The load draws the pane from the packets the game sent at login,
    // which a plugin gets as it loads, in one event.
    h.until("the pane", |h| !sent(h).is_empty()).await;
    let loaded = sent(&h);
    assert_eq!(loaded.len(), 1, "{loaded:#?}");
    assert_eq!(loaded[0]["panes"][0]["title"], "Weather");
    assert_eq!(loaded[0]["panes"][0]["meta"], "Temperate");
    assert_eq!(loaded[0]["removed"], json!([]));

    // Both packets come in one read, and four sets and a meta send once.
    let mut both = packet(include_str!(
        "../../../fixtures/gmcp/aabahran/room-weather.gmcp"
    ));
    both.extend(packet(include_str!(
        "../../../fixtures/gmcp/aabahran/char-state.gmcp"
    )));
    h.servers[0].push(&both);
    h.until("the filled pane", |h| sent(h).len() > 1).await;
    // A line after shows the session read on past the packets.
    h.servers[0].push(b"\n\rMaren walks in.\n\r");
    h.until_shown("Maren walks in.").await;
    let filled = sent(&h);
    assert_eq!(filled.len(), 2, "{filled:#?}");
    let row = |label: &str, value: &str| json!({"kind": "row", "label": label, "value": value});
    let pane = json!({
        "plugin": "weather_pane",
        "id": "weather",
        "title": "Weather",
        "meta": "Coastal North",
        "blocks": [
            row("Sky", "rainy"),
            row("Temperature", "60 F"),
            row("Position", "sitting"),
            row("Language", "common"),
        ],
    });
    assert_eq!(filled[1], json!({"panes": [pane.clone()], "removed": []}));
    // A window that opens now reads the same pane.
    let got = crate::ipc::panes::lua_panes_get(h.app.state(), Some(h.first))
        .await
        .expect("the panes");
    assert_eq!(serde_json::to_value(got).expect("json"), json!([pane]));

    // Turning the plugin off removes its pane at once.
    plugin(&h, "weather_pane", false).await;
    h.until("the removal", |h| sent(h).len() == 3).await;
    assert_eq!(
        sent(&h)[2],
        json!({"panes": [], "removed": [{"plugin": "weather_pane", "id": "weather"}]})
    );
    h.finish(grid).await;
}
