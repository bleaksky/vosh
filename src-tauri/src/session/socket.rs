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

use super::round_trip::{Sample, Waits};

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
    /// How long the oldest line of yours the game has not answered has
    /// waited. Any bytes the game sends answer it, GMCP alone too. See
    /// [`Waits`].
    waits: Waits,
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
            waits: Waits::default(),
        }
    }

    pub(crate) async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = match &mut self.io {
            Io::Tcp(s) => s.read(buf).await,
            Io::Tls(s) => s.read(buf).await,
        };
        if matches!(read, Ok(n) if n > 0) {
            self.waits.heard();
        }
        read
    }

    pub(crate) async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        let line = buf.ends_with(b"\n");
        let reached = line && self.kernel().is_some_and(|k| !k.in_flight);
        match &mut self.io {
            Io::Tcp(s) => AsyncWriteExt::write_all(s, buf).await?,
            Io::Tls(s) => AsyncWriteExt::write_all(s.as_mut(), buf).await?,
        }
        if line {
            let now = Instant::now();
            self.last_line = Some(now);
            self.waits.sent(now, reached);
        }
        Ok(())
    }

    /// The round trip to the game at `now`: the kernel's smoothed round
    /// trip time for the socket, or how long the oldest line the game has
    /// not answered has waited when that is longer and the link or the
    /// game is not answering. The network still carrying the line is the
    /// link not answering. The game sending nothing at all past the
    /// longest lag it puts on you is the game not answering. None where
    /// the system does not say. The sample says when the wait began
    /// when the wait is the reading. See [`super::round_trip`].
    pub(crate) fn round_trip(&mut self, now: Instant) -> Option<Sample> {
        let kernel = self.kernel()?;
        Some(self.waits.reading(kernel, now))
    }

    /// What the kernel says of the game socket, None where it does not.
    fn kernel(&self) -> Option<super::round_trip::kernel::Reading> {
        let tcp = match &self.io {
            Io::Tcp(s) => s,
            Io::Tls(s) => s.get_ref().0,
        };
        super::round_trip::kernel::read(tcp)
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

    use super::super::round_trip::{HELD_AT_MOST, SLOW};
    use super::connect;

    /// The kernel reads the round trip on the systems Vosh ships for. A
    /// line the game's machine acknowledged reached the game, so while
    /// the game holds it unanswered, as it holds what you type ahead
    /// while a skill lags you (`ch->wait` in comm.c), the reading stays
    /// the link's and no stall counts.
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[tokio::test]
    async fn a_line_the_game_holds_is_no_stall() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let game = tokio::spawn(async move { listener.accept().await.expect("a client").0 });
        let mut stream = connect("127.0.0.1", port, false).await.expect("the game");
        let mut game = game.await.expect("the game task");

        let fine = stream
            .round_trip(Instant::now())
            .expect("a reading")
            .reading;
        assert!(fine < SLOW, "{fine:?}");

        // A bash lags you, and you type ahead. The game reads both lines
        // and answers neither for 400 ms.
        stream.write_all(b"kick\r\n").await.expect("the line");
        stream.write_all(b"look\r\n").await.expect("the line");
        let mut lines = [0u8; 12];
        game.read_exact(&mut lines)
            .await
            .expect("the game hears them");
        tokio::time::sleep(Duration::from_millis(400)).await;
        let held = stream
            .round_trip(Instant::now())
            .expect("a reading")
            .reading;
        assert!(held < SLOW, "{held:?}");

        game.write_all(b"You see Tolliver here.\r\n")
            .await
            .expect("the answer");
        let mut buf = [0u8; 64];
        assert!(stream.read(&mut buf).await.expect("the answer") > 0);
        let answered = stream
            .round_trip(Instant::now())
            .expect("a reading")
            .reading;
        assert!(answered < SLOW, "{answered:?}");
    }

    /// A game that reads your line and sends nothing back for longer
    /// than any lag it puts on you is not answering, so the wait counts
    /// even though its machine acknowledged the line. Anything it sends
    /// ends the wait, a GMCP packet alone too.
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[tokio::test]
    async fn a_game_that_says_nothing_past_the_longest_lag_is_a_stall() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let game = tokio::spawn(async move { listener.accept().await.expect("a client").0 });
        let mut stream = connect("127.0.0.1", port, false).await.expect("the game");
        let mut game = game.await.expect("the game task");

        stream.write_all(b"look\r\n").await.expect("the line");
        let mut line = [0u8; 6];
        game.read_exact(&mut line).await.expect("the game hears it");
        let silent = Instant::now() + HELD_AT_MOST + Duration::from_secs(1);
        let stalled = stream.round_trip(silent).expect("a reading").reading;
        assert!(stalled >= HELD_AT_MOST, "{stalled:?}");

        let mut gmcp = vec![255, 250, 201];
        gmcp.extend_from_slice(
            br#"Room.Weather {"sky":"rainy","temp":60,"unit":"F","region":"Coastal North"}"#,
        );
        gmcp.extend_from_slice(&[255, 240]);
        game.write_all(&gmcp).await.expect("the packet");
        let mut buf = [0u8; 128];
        assert!(stream.read(&mut buf).await.expect("the packet") > 0);
        let answered = stream.round_trip(silent).expect("a reading").reading;
        assert!(answered < SLOW, "{answered:?}");
    }
}
