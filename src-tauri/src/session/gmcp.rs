//! What the session does with GMCP. When the game offers it, the session
//! says hello and names the packages it reads. Each packet the game sends
//! then updates the variables, your prompt, the room lists and the tick
//! under the profile lock and the connection's, runs the Lua that listens
//! for it, and goes on to the windows. A new character name in Char.Status
//! or Char.Name is a login, which loads its affect fulls, may switch the
//! profile and sends the session identity.

use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Manager};
use tokio::time::Instant;
use tracing::{info, warn};
use vosh_protocol::telnet::Negotiator;

use crate::app::state::SharedState;
use crate::input;
use crate::profile::live::Profile;
use crate::profile::switch::auto_switch_for_character;
use crate::script::{self, ApplyResult};
use crate::sessions::Session;
use crate::tick::TickStep;

use super::batch::ReadBatch;
use super::conn::Conn;
use super::connection::Connection;
use super::effects::{apply_script_result, deliver_tick_step, OutputSink, ScriptIo};
use super::gmcp_vars;
use super::now_ms;
use super::prompt_view::observe_prompt_gmcp;
use super::read::walked;
use super::vitals_text;

/// GMCP packages we ask the server to enable in Core.Supports.Set. Char,
/// Room, and Comm cover the player view; World powers the tick timer reset
/// (Aabahran ticks fire the moment its `World.Time.hour` field advances);
/// Map carries the server-rendered tile grid for the map pane's server
/// mode; Imm.Queues carries the staff work-queue counters the imm panel
/// renders (the server only sends it to immortals, so declaring it costs
/// mortals nothing). Group carries the roster the Group pane shows.
/// Aabahran sends every package without this list, so it names them
/// for servers that honor it.
///
/// Snoop is the one Aabahran waits for. A Core.Supports body that holds
/// `"Snoop ` turns on Snoop.Start, Snoop.Stop and Snoop.Output for the
/// players you snoop (gmcp.c). The game sends them only to someone who
/// snoops, so a mortal sees no change. serde writes the list with no
/// spaces, so the one entry is enough.
pub(super) const REQUESTED_GMCP_PACKAGES: &[&str] = &[
    "Char 1",
    "Room 1",
    "Comm 1",
    "World 1",
    "Map 1",
    "Imm.Queues 1",
    "Group 1",
    "Snoop 1",
];

/// Take one GMCP packet. `log_id` is the session log's row, which the
/// rows of a snooped player's lines attach to.
pub(super) async fn handle_gmcp<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    log_id: Option<i64>,
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
    // Take the tick step for a World.Time hour change under these locks,
    // as the line path does, so the tick needs no lock of its own after.
    let (tick_step, script_apply, daylight, vitals_text, snooped, picked) = {
        let lock_t0 = std::time::Instant::now();
        let mut p = conn.session.lock_profile().await;
        conn.perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
        conn.perf.mutex_acquires += 1;
        let mut c = conn.session.connection.lock();
        let now = Instant::now();
        let picked = c.link.gmcp(&msg.package);
        // A snoop's text goes to its tab and never to the line pipeline.
        // Lua still hears the packet below. Its whole lines go in the log
        // with this read's rows.
        let at = now_ms();
        let snooped = c.snoops.gmcp(&msg.package, &msg.data, at);
        if snooped {
            batch.log.extend(c.snoops.take_log(log_id, at));
        }
        let (tick_step, mut apply) = gmcp_step(&mut p, &mut c, &msg, now);
        // A tell you got or a fight that starts on you rings its preset.
        apply
            .alerts
            .extend(c.preset_watch.gmcp(&p, &msg, conn.stream.last_line(), now));
        // The game's day or night, beside the tick.
        let daylight = (msg.package == "World.Time")
            .then(|| c.tick.observe_daylight(&msg.data))
            .flatten();
        // Your vitals or the fight moved, so a vitals text draws again.
        let vitals_text = vitals_text::after_package(&conn.session, &p, &c, &msg.package, now);
        (
            tick_step,
            apply.ran_under(p.open()),
            daylight,
            vitals_text,
            snooped,
            picked,
        )
    };
    batch.snoop |= snooped;
    vitals_text::emit(&conn.app, &conn.session, vitals_text);
    if let Some(phase) = daylight {
        conn.session.emit(
            &conn.app,
            crate::app::events::DAYLIGHT_CHANGED,
            &crate::tick::DaylightPayload { phase },
        );
    }

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
                let state = conn.app.state::<SharedState>();
                character_named(&conn.app, state.inner(), &conn.session, &owned).await;
            }
        }
    }
    // A reconnect to a character left link dead sends no Char.Status
    // (`check_reconnect`, comm.c), so the character you picked at the
    // account menu names it at the first vitals of play. The page hears
    // it as Char.Name, the package that names the character alone.
    if let Some(name) = picked {
        batch.character = Some(name.clone());
        let state = conn.app.state::<SharedState>();
        character_named(&conn.app, state.inner(), &conn.session, &name).await;
        conn.session.emit_data(
            &conn.app,
            "session://gmcp/Char-Name",
            &serde_json::json!({ "name": name }),
        );
    }
    let mut sink = OutputSink::Batch(batch);
    let mut io = ScriptIo::Session(&mut conn.stream, &mut sink, &mut conn.walker);
    if let Some(step) = tick_step {
        conn.perf.ticks += 1;
        deliver_tick_step(&conn.app, &mut io, &conn.session, step).await?;
    }
    apply_script_result(&conn.app, &mut io, &conn.session, script_apply).await?;
    walk_gmcp(conn, &msg, batch).await?;
    // Keep the last affects, vitals and combat for a window that opens
    // between packets.
    conn.session.last_packages.observe(&msg.package, &msg.data);
    // A list that changes the affect fulls sends them first, so the
    // windows never draw the list against the old ones (a recast at
    // fewer hours than the old full).
    crate::affects::full::observe(&conn.app, &conn.session, &msg.package, &msg.data);
    // Each package goes out on an event of its own, so a page listener
    // hears only the packages it reads, instead of every listener running
    // on every packet and filtering by `payload.package`. Tauri event
    // names allow only letters, digits, `-`, `/`, `:` and `_`, so the `.`
    // between the parts of a package name becomes `-` (`Char.Vitals`
    // becomes `Char-Vitals`). The page's `onGmcpPackage` helper makes the
    // same swap when it picks the event to listen to.
    let event_name = format!("session://gmcp/{}", msg.package.replace('.', "-"));
    conn.session.emit_data(&conn.app, &event_name, &msg.data);
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
        // Char.Vitals comes only with the game's own prompt
        // (`gmcp.c:935`), so the editor and the pager closed. Aabahran
        // sends it before the text of its pulse, so the writer arms the
        // tick and fires it once that text is in.
        "Char.Vitals" => {
            conn.writer.tick(Instant::now());
            return Ok(());
        }
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
/// note on the terminal, and the session identity and the rows go out.
async fn character_named<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    character: &str,
) {
    // Char.Status is sent on every vitals update, so without this gate
    // the login would run every pulse.
    let is_new = {
        let Ok(mut guard) = session.current_character.lock() else {
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
    session.note_played(character);
    // A login on this link ends what the redial followed of the last play.
    session.connection.lock().link.logged_in();
    // The affect gauges read this character's saved fulls.
    crate::affects::full::character_known(app, state, session, character);
    auto_switch_for_character(app, state, session, character).await;
    crate::session::identity::broadcast_session_identity(app, state, session).await;
    crate::sessions::broadcast_sessions(app, state);
    // Another session that played the character here loses it to this
    // one, so its link closes as expected.
    super::reconnect::took_character(app, state, session, character).await;
}

/// What a GMCP packet does to the profile and the connection, under the
/// locks the caller holds: the variables and the custom prompt take it,
/// and the profile keeps the prompt table the engine holds after it, the
/// connection keeps Room.Chars for the target commands and follows the
/// room look and the end of a fight, a World.Time hour change is the
/// tick, and Lua GMCP handlers run. Returns the tick step and what the
/// handlers asked for, which the caller delivers once the locks drop.
pub(super) fn gmcp_step(
    p: &mut Profile,
    c: &mut Connection,
    msg: &vosh_protocol::gmcp::Message,
    now: Instant,
) -> (Option<TickStep>, ApplyResult) {
    let fought = c.prompt.vars.gmcp().fighting();
    gmcp_vars::apply(&mut c.vars, msg);
    // Before Lua, so a value a GMCP handler sets with
    // `mud.set_prompt_var` belongs to the pulse this packet starts.
    observe_prompt_gmcp(p, c, msg);
    // The fight is over, and the text of the round that ended it is
    // still to come.
    if fought && !c.prompt.vars.gmcp().fighting() {
        c.fight_tail = true;
    }
    // The connection keeps the latest Room.Chars list, so a bare
    // `tar <index>`, `tarn` or `tarp` resolves against the current room
    // without asking the page.
    if msg.package == "Room.Chars" {
        if let Some(arr) = msg.data.as_array() {
            // The look this packet goes with lists one line for each
            // entry after its things.
            c.room_block.room_chars(arr.len());
            let chars = input::target::read_room_chars(arr);
            input::target::set_room_chars(c, chars);
        }
    }
    // The look this packet goes with lists a line for each long text its
    // objects share, five spaces or their count before it.
    if msg.package == "Room.Items" {
        if let Some(arr) = msg.data.as_array() {
            c.room_block.room_items(arr.len());
        }
    }
    let tick_step = crate::tick::observe_world_time_for_tick(&p.tick, &mut c.tick, msg, now);
    script::snapshot_vars(p, c);
    let outcome = c.script.dispatch_gmcp(&msg.package, &msg.data);
    let apply = script::apply_actions(p, c, outcome);
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
