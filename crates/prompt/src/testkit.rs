//! The test kit (section 9 of the build spec), behind the `testkit`
//! feature, so the app never carries it.
//!
//! - [`game`] prints a PROMPT the way the server does, from a game state.
//! - [`mud`] is a fake Aabahran that plays one connection in any of the
//!   three server builds, in the game's wire order. The session tests in
//!   `src-tauri` drive the real session against it, and
//!   `examples/fake_mud.rs` serves it over TCP for scripted runs.
//! - [`wire`] names the synthetic pulses in
//!   `fixtures/prompt/aabahran/wire` and plays each one again, so a test
//!   holds every fixture to the fake that wrote it.
//! - [`map_values`] draws a template from a plain map of prompt vars.
//! - [`designs`] holds the designs many tests draw, and [`at`] and
//!   [`now`] the fixed clocks they read.

pub mod designs;
pub mod game;
pub mod map_values;
pub mod mud;
pub mod wire;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime};

pub use mud::{Affect, Build, Mud, Options, TickOrder, Write};

/// The instant every packet a test feeds arrives at.
pub fn at() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2026-09-29T12:58:02-05:00").expect("a valid time")
}

/// The local time the clock pieces read in a test.
pub fn now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 9, 29)
        .and_then(|d| d.and_hms_opt(8, 42, 10))
        .expect("a valid date")
}

/// A GMCP packet as the game writes it: IAC SB GMCP, the package, a
/// space, the JSON, IAC SE. UTF-8 text holds no IAC byte, so none is
/// doubled.
pub fn gmcp(package: &str, json: &str) -> Vec<u8> {
    use mud::telnet::{GMCP, IAC, SB, SE};
    let mut out = vec![IAC, SB, GMCP];
    out.extend_from_slice(package.as_bytes());
    out.push(b' ');
    out.extend_from_slice(json.as_bytes());
    out.extend_from_slice(&[IAC, SE]);
    out
}

/// `bytes` cut into reads after each offset in `cuts`, which ascend. An
/// offset past the end, or one that repeats, adds no read.
pub fn reads<'a>(bytes: &'a [u8], cuts: &[usize]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut from = 0;
    for &cut in cuts {
        let cut = cut.min(bytes.len());
        if cut > from {
            out.push(&bytes[from..cut]);
            from = cut;
        }
    }
    if from < bytes.len() || out.is_empty() {
        out.push(&bytes[from..]);
    }
    out
}

/// The text a terminal shows for `raw`, with every CSI and telnet
/// command taken out and each GMCP packet dropped, as Vosh's plain text
/// reads it once the telnet parser has taken the packets out.
pub fn shown(raw: &[u8]) -> String {
    use mud::telnet::{IAC, SB, SE};
    let mut text = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == IAC {
            match raw.get(i + 1) {
                Some(&SB) => {
                    let end = raw[i..]
                        .windows(2)
                        .position(|w| w == [IAC, SE])
                        .map_or(raw.len(), |p| i + p + 2);
                    i = end;
                }
                Some(&IAC) => {
                    text.push(IAC);
                    i += 2;
                }
                Some(&b) if (mud::telnet::WILL..=mud::telnet::DONT).contains(&b) => i += 3,
                _ => i += 2,
            }
            continue;
        }
        if raw[i] == 0x1b && raw.get(i + 1) == Some(&b'[') {
            i += 2;
            while i < raw.len() && !(0x40..=0x7e).contains(&raw[i]) {
                i += 1;
            }
            i += 1;
            continue;
        }
        text.push(raw[i]);
        i += 1;
    }
    String::from_utf8_lossy(&text).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packet_is_framed_as_the_game_frames_it() {
        assert_eq!(
            gmcp("Char.Combat", "{}"),
            [&[255, 250, 201][..], b"Char.Combat {}", &[255, 240]].concat()
        );
    }

    #[test]
    fn reads_cut_after_each_offset() {
        let bytes = b"abcdef";
        assert_eq!(reads(bytes, &[]), [&b"abcdef"[..]]);
        assert_eq!(reads(bytes, &[2, 2, 5, 9]), [&b"ab"[..], b"cde", b"f"]);
        assert_eq!(reads(bytes, &[6]), [&b"abcdef"[..]]);
        assert_eq!(reads(b"", &[]), [&b""[..]]);
    }

    #[test]
    fn shown_drops_packets_colors_and_commands() {
        let mut raw = gmcp("Char.Vitals", r#"{"hp":1}"#);
        raw.extend_from_slice(b"\x1b[1;33mHi\x1b[0m there\n\r");
        raw.extend_from_slice(&[255, 249]);
        raw.extend_from_slice(&[255, 251, 201]);
        assert_eq!(shown(&raw), "Hi there\n\r");
    }
}
