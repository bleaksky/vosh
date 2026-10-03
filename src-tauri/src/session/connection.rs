//! Raw TCP and TLS connection wrappers built on tokio.
//!
//! Returns a [`Stream`] enum that the session loop reads from and writes to
//! without caring whether the underlying transport is plain or TLS.

use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

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

/// Either a plain TCP stream or a TLS-wrapped one. The session loop owns
/// this and reads or writes through it without branching on the variant.
pub(crate) enum Stream {
    Tcp(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl Stream {
    pub(crate) async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Tcp(s) => s.read(buf).await,
            Stream::Tls(s) => s.read(buf).await,
        }
    }

    pub(crate) async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => AsyncWriteExt::write_all(s, buf).await,
            Stream::Tls(s) => AsyncWriteExt::write_all(s.as_mut(), buf).await,
        }
    }

    pub(crate) async fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => AsyncWriteExt::flush(s).await,
            Stream::Tls(s) => AsyncWriteExt::flush(s.as_mut()).await,
        }
    }

    pub(crate) async fn shutdown(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => AsyncWriteExt::shutdown(s).await,
            Stream::Tls(s) => AsyncWriteExt::shutdown(s.as_mut()).await,
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
        match self {
            Stream::Tcp(s) => s.try_read(buf),
            Stream::Tls(_) => Err(std::io::Error::new(
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
            return Ok::<Stream, ConnectionError>(Stream::Tcp(tcp));
        }

        let server_name = rustls_pki_types::ServerName::try_from(host.to_string())
            .map_err(|_| ConnectionError::InvalidHost(host.to_string()))?;

        let config = build_tls_config();
        let connector = TlsConnector::from(Arc::new(config));
        let tls_stream = connector
            .connect(server_name, tcp)
            .await
            .map_err(|e| ConnectionError::Tls(e.to_string()))?;
        Ok(Stream::Tls(Box::new(tls_stream)))
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
