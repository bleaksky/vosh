//! What the session does with GMCP. When the game offers it, the session
//! says hello and names the packages it reads. Each packet the game sends
//! then updates the variables, your prompt, the room lists and the tick
//! under the profile lock, runs the Lua that listens for it, and goes on
//! to the windows. A new character name in Char.Status or Char.Name is a
//! login, which loads its affect fulls, may switch the profile and sends
//! the session identity.

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::Instant;
use tracing::{info, warn};
use vosh_protocol::telnet::Negotiator;

use crate::app::state::SharedState;
use crate::input;
use crate::profile::live::Profile;
use crate::profile::switch::auto_switch_for_character;
use crate::script::{self, ApplyResult};
use crate::tick::TickStep;

use super::batch::ReadBatch;
use super::conn::Conn;
use super::effects::{apply_script_result, deliver_tick_step, OutputSink, ScriptIo};
use super::gmcp_vars;
use super::prompt_view::observe_prompt_gmcp;
use super::read::walked;

/// GMCP packages we ask the server to enable in Core.Supports.Set. Char,
/// Room, and Comm cover the player view; World powers the tick timer reset
/// (Aabahran ticks fire the moment its `World.Time.hour` field advances);
/// Map carries the server-rendered tile grid for the map pane's server
/// mode; Imm.Queues carries the staff work-queue counters the imm panel
/// renders (the server only sends it to immortals, so declaring it costs
/// mortals nothing). Group carries the roster the Group pane shows.
/// Aabahran sends every package without this list, so it names them
/// for servers that honor it.
pub(super) const REQUESTED_GMCP_PACKAGES: &[&str] = &[
    "Char 1",
    "Room 1",
    "Comm 1",
    "World 1",
    "Map 1",
    "Imm.Queues 1",
    "Group 1",
];

pub(super) async fn handle_gmcp<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    payload: &[u8],
    batch: &mut ReadBatch,
) -> std::io::Result<()> {
    let msg = match vosh_protocol::gmcp::parse(payload) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "failed to parse GMCP payload");
            return Ok(());
        }
    };
    // Package name at info; the full payload only at debug. Display-
    // formatting every radius-7 Map.Tiles grid into a log line sat on
    // the session hot path per movement. Capture raw payloads with
    // RUST_LOG=vosh_app_lib=debug when needed (e.g. the Group.Info
    // duplicate-member server bug).
    info!(package = %msg.package, "gmcp received");
    tracing::debug!(package = %msg.package, data = %msg.data, "gmcp payload");
    // Take the tick step for a World.Time hour change under this lock, as
    // the line path does, so the tick needs no second lock after it.
    let (tick_step, script_apply) = {
        let lock_t0 = std::time::Instant::now();
        let mut p = conn.profile.lock().await;
        conn.perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
        conn.perf.mutex_acquires += 1;
        gmcp_step(&mut p, &msg, Instant::now())
    };

    // Char.Status / Char.Name carry the logged-in character name on
    // Aabahran (and most ROM derivatives). A name the session has not
    // seen yet is a login. Char.Status fires every vitals update, and
    // `character_named` returns at once for a name it already saw.
    if msg.package == "Char.Status" || msg.package == "Char.Name" {
        if let Some(name) = msg.data.get("name").and_then(|v| v.as_str()) {
            let owned = name.trim().to_string();
            if !owned.is_empty() {
                if msg.package == "Char.Status" {
                    batch.character = Some(owned.clone());
                }
                let state = conn.app.state::<crate::app::state::SharedState>();
                character_named(&conn.app, state.inner(), &owned).await;
            }
        }
    }
    let mut sink = OutputSink::Batch(batch);
    if let Some(step) = tick_step {
        conn.perf.ticks += 1;
        deliver_tick_step(
            &conn.app,
            &mut conn.stream,
            &mut conn.walker,
            &conn.profile,
            &conn.lua_timers,
            step,
            &mut sink,
        )
        .await?;
    }
    apply_script_result(
        &conn.app,
        &mut ScriptIo::Session(&mut conn.stream, &mut sink, &mut conn.walker),
        &conn.profile,
        &conn.lua_timers,
        script_apply,
    )
    .await?;
    walk_gmcp(conn, &msg, batch).await?;
    // Keep the last affects list for a window that opens between ticks.
    conn.app
        .state::<crate::app::state::SharedState>()
        .last_affects
        .observe(&msg.package, &msg.data);
    // A list that changes the affect fulls sends them first, so the
    // windows never draw the list against the old ones (a recast at
    // fewer hours than the old full).
    crate::affects::full::observe(&conn.app, &msg.package, &msg.data);
    // Each package goes out on an event of its own, so a page listener
    // hears only the packages it reads, instead of every listener running
    // on every packet and filtering by `payload.package`. Tauri event
    // names allow only letters, digits, `-`, `/`, `:` and `_`, so the `.`
    // between the parts of a package name becomes `-` (`Char.Vitals`
    // becomes `Char-Vitals`). The page's `onGmcpPackage` helper makes the
    // same swap when it picks the event to listen to.
    let event_name = format!("session://gmcp/{}", msg.package.replace('.', "-"));
    if let Err(e) = conn.app.emit(&event_name, &msg.data) {
        warn!(error = %e, package = %msg.package, "failed to emit GMCP event");
    }
    // `perf.gmcp_packets` already incremented by the caller before
    // we ran. This `emit` count would otherwise duplicate that, so
    // we leave gmcp_packets as the single source.
    Ok(())
}

/// What the walker reads of a packet: the tiles, the room, a fight and
/// your position. What it then asks for goes out at once, and its lines
/// at the end of the read.
async fn walk_gmcp<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    msg: &vosh_protocol::gmcp::Message,
    batch: &mut ReadBatch,
) -> std::io::Result<()> {
    let out = match msg.package.as_str() {
        "Map.Tiles" => {
            conn.walker.tiles(&msg.data);
            return Ok(());
        }
        "Room.Info" => conn.walker.room_info(&msg.data, Instant::now()),
        "Char.Combat" => conn.walker.combat(&msg.data),
        "Char.State" => conn.walker.state(&msg.data),
        _ => return Ok(()),
    };
    walked(conn, out, batch).await
}

/// Char.Status or Char.Name named the character, trimmed and not empty.
/// A name the session already saw does nothing. A new one is a login:
/// it becomes the current character, its affect fulls load, the profile
/// that claims it on this connection becomes the active one, with a
/// note on the terminal, and the session identity goes out.
async fn character_named<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    character: &str,
) {
    // Char.Status is sent on every vitals update, so without this gate
    // the login would run every pulse.
    let is_new = {
        let Ok(mut guard) = state.current_character.lock() else {
            return;
        };
        if guard.as_deref() == Some(character) {
            false
        } else {
            *guard = Some(character.to_string());
            true
        }
    };
    if !is_new {
        return;
    }
    // The affect gauges read this character's saved fulls.
    crate::affects::full::character_known(app, state, character);
    auto_switch_for_character(app, state, character).await;
    crate::session::identity::broadcast_session_identity(app, state).await;
}

/// What a GMCP packet does to the profile, under the profile lock the
/// caller holds: the variables and the custom prompt take it, Room.Chars
/// is kept for the target commands, a World.Time hour change is the
/// tick, and Lua GMCP handlers run. Returns the tick step and what the
/// handlers asked for, which the caller delivers once the lock drops.
pub(super) fn gmcp_step(
    p: &mut Profile,
    msg: &vosh_protocol::gmcp::Message,
    now: Instant,
) -> (Option<TickStep>, ApplyResult) {
    gmcp_vars::apply(&mut p.vars, msg);
    // Before Lua, so a value a GMCP handler sets with
    // `mud.set_prompt_var` belongs to the pulse this packet starts.
    observe_prompt_gmcp(p, msg);
    // Cache the latest Room.Chars snapshot in the profile so
    // bare `tar <index>` / `tarn` / `tarp` commands can resolve
    // against the current room without round-tripping to the
    // frontend.
    if msg.package == "Room.Chars" {
        if let Some(arr) = msg.data.as_array() {
            // The look this packet goes with lists one line for each
            // entry after its things.
            p.room_block.room_chars(arr.len());
            let chars = input::target::read_room_chars(arr);
            input::target::set_room_chars(p, chars);
        }
    }
    // The look this packet goes with lists a line for each long text its
    // objects share, five spaces or their count before it.
    if msg.package == "Room.Items" {
        if let Some(arr) = msg.data.as_array() {
            p.room_block.room_items(arr.len());
        }
    }
    let tick_step = crate::tick::observe_world_time_for_tick(&mut p.tick, msg, now);
    script::snapshot_vars(&p.script, &p.vars);
    let outcome = match p.script.dispatch_gmcp(&msg.package, &msg.data) {
        Ok(o) => o,
        Err(err) => {
            warn!(error = %err, "lua dispatch_gmcp failed");
            vosh_script::ScriptOutcome::default()
        }
    };
    let apply = script::apply_actions(p, outcome);
    (tick_step, apply)
}

pub(super) fn hello_subnegotiation() -> Vec<u8> {
    let body = vosh_protocol::gmcp::build(
        "Core.Hello",
        &json!({
            "client": "vosh",
            "version": env!("CARGO_PKG_VERSION"),
        }),
    )
    .unwrap_or_default();
    Negotiator::build_gmcp_subnegotiation(&body)
}

pub(super) fn supports_subnegotiation() -> Vec<u8> {
    let body = vosh_protocol::gmcp::build("Core.Supports.Set", &REQUESTED_GMCP_PACKAGES.to_vec())
        .unwrap_or_default();
    Negotiator::build_gmcp_subnegotiation(&body)
}
