//! Per-session task. Wires the connection, the telnet parser, and the
//! Tauri event bus together.

use mudclient_telnet::{Event as TelnetEvent, Negotiator, Parser};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::connection::{self, ConnectionError, Stream};

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
    let task = tokio::spawn(io_loop(app, stream, rx_outgoing));

    Ok(SessionHandle { tx_outgoing, task })
}

async fn io_loop(
    app: AppHandle,
    mut stream: Stream,
    mut rx_outgoing: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let mut parser = Parser::new();
    let negotiator = Negotiator::new();
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
                        if let Err(e) = handle_event(&app, &mut stream, &negotiator, event).await {
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
    event: TelnetEvent,
) -> std::io::Result<()> {
    match event {
        TelnetEvent::Data(bytes) => {
            emit_output(app, bytes);
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
