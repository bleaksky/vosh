//! Raw TCP and TLS connection wrappers built on tokio.
//!
//! Returns a [`Stream`] that the session loop reads from and writes to
//! without caring whether the underlying transport is plain or TLS.

use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::Instant;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

/// How many lines `buf` ends.
pub(crate) fn lines_in(buf: &[u8]) -> u64 {
    buf.iter().fold(0, |n, b| n + u64::from(*b == b'\n'))
}

/// Hard cap on a connect attempt. Bad hosts and silent firewalls otherwise
/// hang the UI for the OS-level timeout (often minutes).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub(crate) enum ConnectionError {
    #[error("invalid host name `{0}`")]
    InvalidHost(String),
    #[error("connect timed out after {}s", CONNECT_TIMEOUT.as_secs())]
    Timeout,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tls error: {0}")]
    Tls(String),
}

/// Either a plain TCP stream or a TLS-wrapped one, and when a line of
/// yours last left on it. The session loop owns this and reads or writes
/// through it without branching on the variant.
pub(crate) struct Stream {
    io: Io,
    /// When the last write that ended a line left, the commands you type
    /// and those triggers, timers, the tick and Lua send. A telnet answer
    /// ends no line, so it never counts.
    last_line: Option<Instant>,
    /// When the oldest line of yours the game has not answered yet left.
    /// Any bytes from the game answer it.
    unanswered: Option<Instant>,
    /// Lines sent on the stream, every one that left and every one held.
    lines_out: u64,
    /// The lines held back while the writing card drives the game's
    /// editor, see [`Stream::hold`].
    held: Option<Vec<u8>>,
}

enum Io {
    Tcp(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl Stream {
    fn new(io: Io) -> Self {
        Self {
            io,
            last_line: None,
            unanswered: None,
            lines_out: 0,
            held: None,
        }
    }

    pub(crate) async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = match &mut self.io {
            Io::Tcp(s) => s.read(buf).await,
            Io::Tls(s) => s.read(buf).await,
        };
        if matches!(read, Ok(n) if n > 0) {
            self.unanswered = None;
        }
        read
    }

    /// Write `buf`, or hold it while [`Stream::hold`] holds the lines of
    /// the session. A telnet answer ends no line, so it never waits.
    pub(crate) async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        if let Some(held) = &mut self.held {
            if buf.ends_with(b"\n") {
                self.lines_out += lines_in(buf);
                held.extend_from_slice(buf);
                return Ok(());
            }
        }
        self.write_now(buf).await
    }

    /// Write `buf` past the hold, as the writing card's own lines and the
    /// lines you type go.
    pub(crate) async fn write_now(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match &mut self.io {
            Io::Tcp(s) => AsyncWriteExt::write_all(s, buf).await?,
            Io::Tls(s) => AsyncWriteExt::write_all(s.as_mut(), buf).await?,
        }
        if buf.ends_with(b"\n") {
            let now = Instant::now();
            self.last_line = Some(now);
            self.unanswered.get_or_insert(now);
            self.lines_out += lines_in(buf);
        }
        Ok(())
    }

    /// The lines sent on the stream so far, held ones included.
    pub(crate) fn lines_out(&self) -> u64 {
        self.lines_out
    }

    /// Hold the lines the session sends from here, the commands triggers,
    /// timers, the tick, Lua and `#walk` send, until [`Stream::release`].
    pub(crate) fn hold(&mut self) {
        self.held.get_or_insert_with(Vec::new);
    }

    /// How many lines wait in the hold.
    pub(crate) fn held_lines(&self) -> usize {
        self.held.as_ref().map_or(0, |held| {
            usize::try_from(lines_in(held)).unwrap_or(usize::MAX)
        })
    }

    /// End the hold and send what it kept, in the order it came.
    pub(crate) async fn release(&mut self) -> std::io::Result<()> {
        let Some(held) = self.held.take() else {
            return Ok(());
        };
        if held.is_empty() {
            return Ok(());
        }
        self.lines_out -= lines_in(&held);
        self.write_now(&held).await?;
        self.flush().await
    }

    /// The round trip to the game at `now`: the kernel's smoothed round
    /// trip time for the socket, or how long the oldest line the game has
    /// not answered has waited when that is longer. None where the system
    /// does not say. See [`super::round_trip`].
    pub(crate) fn round_trip(&self, now: Instant) -> Option<Duration> {
        let tcp = match &self.io {
            Io::Tcp(s) => s,
            Io::Tls(s) => s.get_ref().0,
        };
        let waited = self
            .unanswered
            .map_or(Duration::ZERO, |sent| now.duration_since(sent));
        super::round_trip::kernel::read(tcp).map(|kernel| kernel.max(waited))
    }

    /// When a line of yours last left for the game, None before the first.
    pub(crate) fn last_line(&self) -> Option<Instant> {
        self.last_line
    }

    pub(crate) async fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.io {
            Io::Tcp(s) => AsyncWriteExt::flush(s).await,
            Io::Tls(s) => AsyncWriteExt::flush(s.as_mut()).await,
        }
    }

    pub(crate) async fn shutdown(&mut self) -> std::io::Result<()> {
        match &mut self.io {
            Io::Tcp(s) => AsyncWriteExt::shutdown(s).await,
            Io::Tls(s) => AsyncWriteExt::shutdown(s.as_mut()).await,
        }
    }

    /// Non-blocking read. Used by the session loop after a read error to
    /// scoop any bytes still buffered in the kernel recv queue before
    /// tearing the connection down. Returns `WouldBlock` when nothing is
    /// available right now (the caller should stop draining).
    ///
    /// Only meaningful for plain TCP. TLS framing makes a single
    /// non-blocking read unable to surface application bytes in
    /// isolation, so the TLS variant always reports `WouldBlock` and the
    /// caller falls back to its normal error path.
    pub(crate) fn try_read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match &mut self.io {
            Io::Tcp(s) => s.try_read(buf),
            Io::Tls(_) => Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "try_read not supported on TLS streams",
            )),
        }
    }
}

/// Open a connection. Returns a [`Stream`] ready for the session loop.
/// Bounded by `CONNECT_TIMEOUT` so the UI cannot hang forever on a bad host.
pub(crate) async fn connect(host: &str, port: u16, tls: bool) -> Result<Stream, ConnectionError> {
    let attempt = async {
        let tcp = TcpStream::connect((host, port)).await?;
        tcp.set_nodelay(true)?;

        if !tls {
            return Ok::<Stream, ConnectionError>(Stream::new(Io::Tcp(tcp)));
        }

        let server_name = rustls_pki_types::ServerName::try_from(host.to_string())
            .map_err(|_| ConnectionError::InvalidHost(host.to_string()))?;

        let config = build_tls_config();
        let connector = TlsConnector::from(Arc::new(config));
        let tls_stream = connector
            .connect(server_name, tcp)
            .await
            .map_err(|e| ConnectionError::Tls(e.to_string()))?;
        Ok(Stream::new(Io::Tls(Box::new(tls_stream))))
    };

    match tokio::time::timeout(CONNECT_TIMEOUT, attempt).await {
        Ok(result) => result,
        Err(_) => Err(ConnectionError::Timeout),
    }
}

fn build_tls_config() -> ClientConfig {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::Instant;

    use super::connect;

    /// The kernel reads the round trip on the systems Vosh ships for, and
    /// a line the game has not answered counts up past it until the game
    /// writes back.
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[tokio::test]
    async fn the_round_trip_counts_up_while_the_game_has_not_answered() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let game = tokio::spawn(async move { listener.accept().await.expect("a client").0 });
        let mut stream = connect("127.0.0.1", port, false).await.expect("the game");
        let mut game = game.await.expect("the game task");

        let fine = stream.round_trip(Instant::now()).expect("a reading");
        assert!(fine < Duration::from_millis(300), "{fine:?}");

        stream.write_all(b"look\r\n").await.expect("the line");
        let mut line = [0u8; 6];
        game.read_exact(&mut line).await.expect("the game hears it");
        tokio::time::sleep(Duration::from_millis(400)).await;
        let waiting = stream.round_trip(Instant::now()).expect("a reading");
        assert!(waiting >= Duration::from_millis(400), "{waiting:?}");

        game.write_all(b"You see Tolliver here.\r\n")
            .await
            .expect("the answer");
        let mut buf = [0u8; 64];
        assert!(stream.read(&mut buf).await.expect("the answer") > 0);
        let answered = stream.round_trip(Instant::now()).expect("a reading");
        assert!(answered < Duration::from_millis(300), "{answered:?}");
    }
}
