//! Per-session task. Wires the connection, the telnet parser, the line
//! accumulator, and the trigger engine together. Emits Tauri events.

use std::sync::Arc;

use mudclient_telnet::{Event as TelnetEvent, Negotiator, Parser};
use mudclient_trigger::LineResult;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::connection::{self, ConnectionError, Stream};
use crate::line_accumulator::{ChunkOp, LineAccumulator};
use crate::profile::Profile;

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
    let task = tokio::spawn(io_loop(app, stream, rx_outgoing, profile));

    Ok(SessionHandle { tx_outgoing, task })
}

async fn io_loop(
    app: AppHandle,
    mut stream: Stream,
    mut rx_outgoing: mpsc::UnboundedReceiver<Vec<u8>>,
    profile: Arc<Mutex<Profile>>,
) {
    let mut parser = Parser::new();
    let negotiator = Negotiator::new();
    let mut accumulator = LineAccumulator::new();
    let mut buf = vec![0u8; READ_BUFFER_BYTES];

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
        }
    };

    accumulator.reset();
    let _ = stream.shutdown().await;
    emit_state(
        &app,
        StatePayload::Disconnected {
            reason: disconnect_reason,
        },
    );
}

async fn handle_event(
    app: &AppHandle,
    stream: &mut Stream,
    negotiator: &Negotiator,
    accumulator: &mut LineAccumulator,
    profile: &Arc<Mutex<Profile>>,
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
                        let result = {
                            let p = profile.lock().await;
                            mudclient_trigger::process(&p.triggers, &bytes)
                        };
                        emit_line_result(app, &result, clear_first);
                        send_trigger_outputs(stream, &result.sends).await?;
                    }
                }
            }
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
    for pane in &result.routes {
        debug!(target = %pane, "route hint queued (Phase 5 will render panes)");
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
