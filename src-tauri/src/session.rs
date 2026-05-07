//! Per-session task. Wires the connection, the telnet parser, the line
//! accumulator, and the trigger engine together. Emits Tauri events.

use std::sync::Arc;
use std::time::Duration;

use mudclient_telnet::{option as telnet_option, Event as TelnetEvent, Negotiator, Parser};
use mudclient_trigger::LineResult;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

use crate::connection::{self, ConnectionError, Stream};
use crate::gmcp_bind;
use crate::input;
use crate::line_accumulator::{ChunkOp, LineAccumulator};
use crate::map_state::{self, SharedMap};
use crate::profile::Profile;
use crate::tick::{TickPayload, TickRuntime};

const TICK_EMIT_INTERVAL: Duration = Duration::from_millis(250);

const READ_BUFFER_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OutputPayload {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StatePayload {
    Connecting { host: String, port: u16, tls: bool },
    Connected { host: String, port: u16, tls: bool },
    Disconnected { reason: Option<String> },
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GmcpPayload {
    pub package: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RoutedPayload {
    pub pane: String,
    pub text: String,
}

/// GMCP packages we ask the server to enable in Core.Supports.Set. Char,
/// Room, and Comm cover the player view; World powers the tick timer reset
/// (Aabahran ticks fire the moment its `World.Time.hour` field advances);
/// Map carries the server-rendered tile grid for the map pane's server
/// mode. More packages land alongside the script engine in Phase 8.
const REQUESTED_GMCP_PACKAGES: &[&str] = &["Char 1", "Room 1", "Comm 1", "World 1", "Map 1"];

pub(crate) struct SessionHandle {
    tx_outgoing: mpsc::UnboundedSender<Vec<u8>>,
    task: JoinHandle<()>,
}

impl SessionHandle {
    /// Send raw bytes to the connection. Returns false when the session has
    /// already been torn down.
    pub(crate) fn send(&self, bytes: Vec<u8>) -> bool {
        self.tx_outgoing.send(bytes).is_ok()
    }

    pub(crate) async fn shutdown(self) {
        drop(self.tx_outgoing);
        let _ = self.task.await;
    }
}

/// Open a connection, install a parser plus negotiator, and spin up the IO
/// loop. The returned handle owns the outgoing channel; drop it to close.
pub(crate) async fn spawn(
    app: AppHandle,
    host: String,
    port: u16,
    tls: bool,
    profile: Arc<Mutex<Profile>>,
    map: SharedMap,
) -> Result<SessionHandle, ConnectionError> {
    emit_state(
        &app,
        StatePayload::Connecting {
            host: host.clone(),
            port,
            tls,
        },
    );

    let stream = connection::connect(&host, port, tls).await?;
    info!(%host, port, tls, "session connected");

    emit_state(
        &app,
        StatePayload::Connected {
            host: host.clone(),
            port,
            tls,
        },
    );

    let (tx_outgoing, rx_outgoing) = mpsc::unbounded_channel::<Vec<u8>>();
    let task = tokio::spawn(io_loop(app, stream, rx_outgoing, profile, map));

    Ok(SessionHandle { tx_outgoing, task })
}

async fn io_loop(
    app: AppHandle,
    mut stream: Stream,
    mut rx_outgoing: mpsc::UnboundedReceiver<Vec<u8>>,
    profile: Arc<Mutex<Profile>>,
    map: SharedMap,
) {
    let mut parser = Parser::new();
    let negotiator = Negotiator::new();
    let mut accumulator = LineAccumulator::new();
    let mut buf = vec![0u8; READ_BUFFER_BYTES];

    // Activate the tick timer for this session. The user can disable it
    // later through the slash command.
    {
        let mut p = profile.lock().await;
        p.tick.enable(Instant::now());
    }

    let mut tick_interval = tokio::time::interval(TICK_EMIT_INTERVAL);
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let disconnect_reason = loop {
        tokio::select! {
            biased;
            outgoing = rx_outgoing.recv() => match outgoing {
                Some(bytes) => {
                    if let Err(e) = stream.write_all(&bytes).await {
                        error!(error = %e, "write failed");
                        break Some(format!("write failed: {e}"));
                    }
                    if let Err(e) = stream.flush().await {
                        error!(error = %e, "flush failed");
                        break Some(format!("flush failed: {e}"));
                    }
                }
                None => {
                    debug!("outgoing channel closed; shutting down session");
                    break None;
                }
            },
            read = stream.read(&mut buf) => match read {
                Ok(0) => {
                    info!("server closed connection");
                    break Some("server closed connection".to_string());
                }
                Ok(n) => {
                    let events = parser.feed(&buf[..n]);
                    for event in events {
                        if let Err(e) = handle_event(
                            &app,
                            &mut stream,
                            &negotiator,
                            &mut accumulator,
                            &profile,
                            &map,
                            event,
                        ).await {
                            warn!(error = %e, "event handling failed");
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "read failed");
                    break Some(format!("read failed: {e}"));
                }
            },
            _ = tick_interval.tick() => {
                if let Err(e) = handle_tick(&app, &mut stream, &profile).await {
                    error!(error = %e, "tick handling failed");
                    break Some(format!("tick handling failed: {e}"));
                }
            }
        }
    };

    {
        let mut p = profile.lock().await;
        p.tick.disable();
    }

    accumulator.reset();
    let _ = stream.shutdown().await;
    emit_state(
        &app,
        StatePayload::Disconnected {
            reason: disconnect_reason,
        },
    );
}

async fn handle_tick(
    app: &AppHandle,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
) -> std::io::Result<()> {
    let now = Instant::now();
    // Take the firing decision under the lock. If the timer fired, capture
    // the auto-fire command (if any) so we can run it after releasing the
    // lock.
    let (payload, auto_fire) = {
        let mut p = profile.lock().await;
        let fired = p.tick.try_consume_fire(now);
        let payload = TickPayload::from_runtime(&p.tick, now, fired);
        let auto_fire = if fired {
            p.tick.config.auto_fire.clone()
        } else {
            None
        };
        (payload, auto_fire)
    };

    if !payload.enabled && !payload.fired {
        return Ok(());
    }

    if let Err(e) = app.emit("session://tick", &payload) {
        warn!(error = %e, "failed to emit tick payload");
    }

    if let Some(command) = auto_fire {
        let result = {
            let mut p = profile.lock().await;
            input::process(&mut p, &command)
        };
        if !result.echo.is_empty() {
            let mut buf = Vec::new();
            for line in &result.echo {
                buf.extend_from_slice(b"\r\n");
                buf.extend_from_slice(line.as_bytes());
            }
            buf.extend_from_slice(b"\r\n");
            emit_output(app, buf);
        }
        if !result.bytes.is_empty() {
            stream.write_all(&result.bytes).await?;
            stream.flush().await?;
        }
    }

    Ok(())
}

async fn handle_event(
    app: &AppHandle,
    stream: &mut Stream,
    negotiator: &Negotiator,
    accumulator: &mut LineAccumulator,
    profile: &Arc<Mutex<Profile>>,
    map: &SharedMap,
    event: TelnetEvent,
) -> std::io::Result<()> {
    match event {
        TelnetEvent::Data(bytes) => {
            for op in accumulator.feed(&bytes) {
                match op {
                    ChunkOp::RawDisplay(b) => {
                        emit_output(app, b);
                    }
                    ChunkOp::LineComplete { bytes, clear_first } => {
                        let plain = mudclient_ansi::plain_text(&bytes);
                        let (result, tick_reset) = {
                            let mut p = profile.lock().await;
                            let result = mudclient_trigger::process(&p.triggers, &bytes);
                            let ticked = if p.tick.check_reset_match(&plain) {
                                p.tick.reset(Instant::now());
                                true
                            } else {
                                false
                            };
                            (result, ticked)
                        };
                        emit_line_result(app, &result, clear_first);
                        send_trigger_outputs(stream, &result.sends).await?;
                        if tick_reset {
                            let payload = {
                                let p = profile.lock().await;
                                TickPayload::from_runtime(&p.tick, Instant::now(), false)
                            };
                            if let Err(e) = app.emit("session://tick", &payload) {
                                warn!(error = %e, "failed to emit tick reset payload");
                            }
                        }
                    }
                }
            }
            Ok(())
        }
        TelnetEvent::Subnegotiation { option, payload } if option == telnet_option::GMCP => {
            handle_gmcp(app, profile, map, &payload).await;
            Ok(())
        }
        TelnetEvent::Will(opt) if opt == telnet_option::GMCP => {
            // Accept GMCP via the negotiator, then immediately announce
            // ourselves and the packages we want.
            let response = negotiator.handle(&TelnetEvent::Will(opt));
            stream.write_all(&response).await?;
            stream.write_all(&hello_subnegotiation()).await?;
            stream.write_all(&supports_subnegotiation()).await?;
            stream.flush().await?;
            Ok(())
        }
        other => {
            let response = negotiator.handle(&other);
            if !response.is_empty() {
                stream.write_all(&response).await?;
                stream.flush().await?;
            }
            Ok(())
        }
    }
}

async fn handle_gmcp(
    app: &AppHandle,
    profile: &Arc<Mutex<Profile>>,
    map: &SharedMap,
    payload: &[u8],
) {
    let msg = match mudclient_gmcp::parse(payload) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "failed to parse GMCP payload");
            return;
        }
    };
    let tick_reset = {
        let mut p = profile.lock().await;
        gmcp_bind::apply(&mut p.vars, &msg);
        observe_world_time_for_tick(&mut p.tick, &msg)
    };
    if tick_reset {
        let payload = {
            let p = profile.lock().await;
            TickPayload::from_runtime(&p.tick, Instant::now(), false)
        };
        if let Err(e) = app.emit("session://tick", &payload) {
            warn!(error = %e, "failed to emit tick payload after world hour change");
        }
    }
    if let Err(e) = map_state::handle_room_info(app, map, &msg).await {
        warn!(error = %e, "failed to update map from Room.Info");
    }
    if let Err(e) = app.emit(
        "session://gmcp",
        GmcpPayload {
            package: msg.package,
            data: msg.data,
        },
    ) {
        warn!(error = %e, "failed to emit GMCP event");
    }
}

/// Detect a tick fire from a GMCP `World.Time` push. Aabahran (and most ROM
/// derivatives that ship World.Time) advance the `hour` field every server
/// tick, so an hour change is the natural reset signal. Returns true when
/// the tick was reset.
fn observe_world_time_for_tick(tick: &mut TickRuntime, msg: &mudclient_gmcp::Message) -> bool {
    if msg.package != "World.Time" {
        return false;
    }
    let Some(obj) = msg.data.as_object() else {
        return false;
    };
    let Some(hour_value) = obj.get("hour") else {
        return false;
    };
    let hour_str = match hour_value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return false,
    };
    if tick.observe_world_hour(&hour_str) {
        tick.reset(Instant::now());
        true
    } else {
        false
    }
}

fn hello_subnegotiation() -> Vec<u8> {
    let body = mudclient_gmcp::build(
        "Core.Hello",
        &json!({
            "client": "mudclient",
            "version": env!("CARGO_PKG_VERSION"),
        }),
    )
    .unwrap_or_default();
    Negotiator::build_gmcp_subnegotiation(&body)
}

fn supports_subnegotiation() -> Vec<u8> {
    let body = mudclient_gmcp::build("Core.Supports.Set", &REQUESTED_GMCP_PACKAGES.to_vec())
        .unwrap_or_default();
    Negotiator::build_gmcp_subnegotiation(&body)
}

fn emit_line_result(app: &AppHandle, result: &LineResult, clear_first: bool) {
    let mut bytes: Vec<u8> = Vec::new();
    if clear_first {
        // Wipe the partial that was already shown raw so the trigger-
        // processed line replaces it cleanly. ESC [ 2 K clears the entire
        // line, then \r returns the cursor to column zero.
        bytes.extend_from_slice(b"\x1b[2K\r");
    }
    if let Some(text) = &result.display {
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(b"\r\n");
    }
    if !bytes.is_empty() {
        emit_output(app, bytes);
    }
    if let Some(text) = &result.display {
        for pane in &result.routes {
            if let Err(e) = app.emit(
                "session://routed",
                RoutedPayload {
                    pane: pane.clone(),
                    text: text.clone(),
                },
            ) {
                warn!(error = %e, "failed to emit routed line");
            }
        }
    }
}

async fn send_trigger_outputs(stream: &mut Stream, sends: &[String]) -> std::io::Result<()> {
    if sends.is_empty() {
        return Ok(());
    }
    let mut payload = Vec::new();
    for cmd in sends {
        payload.extend_from_slice(cmd.as_bytes());
        payload.extend_from_slice(b"\r\n");
    }
    stream.write_all(&payload).await?;
    stream.flush().await
}

fn emit_output(app: &AppHandle, bytes: Vec<u8>) {
    if let Err(e) = app.emit("session://output", OutputPayload { bytes }) {
        warn!(error = %e, "failed to emit session output");
    }
}

fn emit_state(app: &AppHandle, payload: StatePayload) {
    if let Err(e) = app.emit("session://state", payload) {
        warn!(error = %e, "failed to emit session state");
    }
}
