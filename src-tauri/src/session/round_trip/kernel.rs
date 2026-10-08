//! The kernel's smoothed round trip time for a TCP socket (Round Trip
//! Readout P3). Every TCP connection keeps one, so reading it sends
//! nothing over the wire. macOS reports `tcpi_srtt` in milliseconds
//! through `TCP_CONNECTION_INFO`, Linux `tcpi_rtt` in microseconds
//! through `TCP_INFO`, and Windows `RttUs` through `SIO_TCP_INFO`. Any
//! other system, or a call that fails, reports nothing, and the status
//! line then shows no reading.

use std::time::Duration;

use tokio::net::TcpStream;

/// The smoothed round trip time of `tcp`, or None where the system
/// does not say.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub(crate) fn read(tcp: &TcpStream) -> Option<Duration> {
    use std::os::fd::AsRawFd;
    // SAFETY: tcp_connection_info is plain old data, so all zeros is a
    // valid value, and getsockopt writes at most `len` bytes into it
    // on the socket `tcp` keeps open for the call.
    let mut info: libc::tcp_connection_info = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::tcp_connection_info>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            tcp.as_raw_fd(),
            libc::IPPROTO_TCP,
            libc::TCP_CONNECTION_INFO,
            (&raw mut info).cast(),
            &raw mut len,
        )
    };
    (rc == 0).then(|| Duration::from_millis(u64::from(info.tcpi_srtt)))
}

/// The smoothed round trip time of `tcp`, or None where the system
/// does not say.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub(crate) fn read(tcp: &TcpStream) -> Option<Duration> {
    use std::os::fd::AsRawFd;
    // SAFETY: tcp_info is plain old data, so all zeros is a valid
    // value, and getsockopt writes at most `len` bytes into it on the
    // socket `tcp` keeps open for the call.
    let mut info: libc::tcp_info = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::tcp_info>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            tcp.as_raw_fd(),
            libc::IPPROTO_TCP,
            libc::TCP_INFO,
            (&raw mut info).cast(),
            &raw mut len,
        )
    };
    (rc == 0).then(|| Duration::from_micros(u64::from(info.tcpi_rtt)))
}

/// The smoothed round trip time of `tcp`, or None where the system
/// does not say, as on Windows before 10 1703.
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn read(tcp: &TcpStream) -> Option<Duration> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{TCP_INFO_v0, WSAIoctl, SIO_TCP_INFO};
    // Version 0 of the TCP_INFO structure.
    let version: u32 = 0;
    let mut info = TCP_INFO_v0::default();
    let mut returned: u32 = 0;
    // SAFETY: the in buffer is the u32 version, the out buffer is a
    // TCP_INFO_v0 of the size passed, the socket stays open for the
    // call, and with no overlapped struct the call completes before it
    // returns.
    let rc = unsafe {
        WSAIoctl(
            tcp.as_raw_socket() as usize,
            SIO_TCP_INFO,
            (&raw const version).cast(),
            std::mem::size_of::<u32>() as u32,
            (&raw mut info).cast(),
            std::mem::size_of::<TCP_INFO_v0>() as u32,
            &raw mut returned,
            std::ptr::null_mut(),
            None,
        )
    };
    (rc == 0).then(|| Duration::from_micros(u64::from(info.RttUs)))
}

/// Other systems report nothing.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub(crate) fn read(_tcp: &TcpStream) -> Option<Duration> {
    None
}
