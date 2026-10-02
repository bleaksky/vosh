//! Default policy for telnet option negotiation. Decides which options to
//! accept and produces response bytes for the session loop to send.
//!
//! Each option keeps the state RFC 1143 gives it, once for each side:
//! what the server performs (its WILL and WONT, our DO and DONT) and what
//! Vosh performs (its DO and DONT, our WILL and WONT). An answer goes out
//! only when it changes a state, so a server that offers or asks for an
//! option again, or answers every DO with WILL, never starts a loop.

use crate::codes::{charset, new_environ, option, ttype, DO, DONT, IAC, SB, SE, WILL, WONT};
use crate::parser::Event;
/// Default first TTYPE response (slot 0). MTTS expects the client
/// name plus version here, and MTTS-aware servers iterate to slot 1
/// for the terminal emulation. But many ROM- and Diku-derived servers
/// (Forsaken Lands among them) only ever send a single
/// `IAC SB TTYPE SEND IAC SE` and then substring-scan the reply for
/// "xterm" or "256" to decide 256-color support. Slot 0 is the only
/// thing those servers ever see, so we bake both markers into the
/// client name itself. The lowercase "xterm" form matches even on
/// case-sensitive `strstr` checks, and the embedded "256color"
/// covers the "256" substring scan.
pub const DEFAULT_TERMINAL_TYPE: &str = concat!("VOSH-xterm-256color ", env!("CARGO_PKG_VERSION"));

/// MTTS capability bits the client advertises on the third TTYPE
/// response. Computed from <https://tintin.mudhalla.net/protocols/mtts/>:
///   1   ANSI         — 16-color SGR support
///   2   VT100        — xterm-compatible control sequences
///   4   UTF-8        — UTF-8 over the wire (xterm.js + charset negotiation)
///   8   256 COLORS   — the actual flag servers gate 256-color output on
///   256 TRUE COLOR   — 24-bit SGR (xterm.js renders these)
/// 1 + 2 + 4 + 8 + 256 = 271. Mouse, OSC palette, screen reader, MNES,
/// MSLP, SSL and proxy are off because Vosh either does not implement
/// them or they are not advertised at the telnet layer.
pub const DEFAULT_MTTS_BITS: u32 = 1 | 2 | 4 | 8 | 256;

/// Build the three default TTYPE responses cycled in order:
/// 1. client name + version
/// 2. terminal emulation family
/// 3. MTTS bitmask
fn default_ttype_responses() -> Vec<String> {
    vec![
        DEFAULT_TERMINAL_TYPE.to_string(),
        "XTERM-256COLOR".to_string(),
        format!("MTTS {DEFAULT_MTTS_BITS}"),
    ]
}

/// Where one side of one option stands (RFC 1143, the Q method). Vosh
/// only ever asks to turn an option on, and never changes its mind while
/// that request is out, so the method's WANTNO state and its queue never
/// come up and are left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Side {
    #[default]
    No,
    Yes,
    /// Vosh asked to turn the option on and waits for the answer.
    WantYes,
}

/// The options the server may perform. Vosh answers an offer of these
/// with DO.
fn wants_server(opt: u8) -> bool {
    matches!(
        opt,
        option::EOR | option::SUPPRESS_GO_AHEAD | option::ECHO | option::GMCP
    )
}

/// The options Vosh performs when the server asks. Vosh answers a request
/// for these with WILL.
fn agrees_to(opt: u8) -> bool {
    matches!(
        opt,
        option::TTYPE
            | option::CHARSET
            | option::SUPPRESS_GO_AHEAD
            | option::GMCP
            | option::NEW_ENVIRON
            | option::NAWS
    )
}

#[derive(Debug, Clone)]
pub struct Negotiator {
    pub window_size: (u16, u16),
    /// Successive responses cycled per the MTTS standard. Slot 0 fires on
    /// the first TTYPE SEND, slot 1 on the second, and so on; once the
    /// last slot is reached it keeps repeating (the spec says the cycle
    /// is complete when the server sees the same answer twice).
    pub ttype_responses: Vec<String>,
    /// Index of the next response to send.
    ttype_cycle: usize,
    /// What the server performs, by option.
    server: [Side; 256],
    /// What Vosh performs, by option.
    vosh: [Side; 256],
}

impl Default for Negotiator {
    fn default() -> Self {
        Self {
            window_size: (80, 24),
            ttype_responses: default_ttype_responses(),
            ttype_cycle: 0,
            server: [Side::No; 256],
            vosh: [Side::No; 256],
        }
    }
}

impl Negotiator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Map a parser event to bytes that the session loop should send back to
    /// the server. Returns an empty Vec when no response is needed.
    pub fn handle(&mut self, event: &Event) -> Vec<u8> {
        match event {
            Event::Will(opt) => self.respond_will(*opt),
            Event::Wont(opt) => self.respond_wont(*opt),
            Event::Do(opt) => self.respond_do(*opt),
            Event::Dont(opt) => self.respond_dont(*opt),
            Event::Subnegotiation { option, payload } => {
                self.respond_subnegotiation(*option, payload)
            }
            _ => Vec::new(),
        }
    }

    /// Ask the server to perform `opt`, as Vosh asks for EOR when it
    /// connects. Returns the DO to send, or nothing when the option is on
    /// already or a request for it is out.
    pub fn ask(&mut self, opt: u8) -> Vec<u8> {
        let side = &mut self.server[usize::from(opt)];
        if *side != Side::No {
            return Vec::new();
        }
        *side = Side::WantYes;
        vec![IAC, DO, opt]
    }

    /// The server performs `opt`: it said WILL and Vosh agreed.
    pub fn server_does(&self, opt: u8) -> bool {
        self.server[usize::from(opt)] == Side::Yes
    }

    /// Vosh performs `opt`: the server said DO and Vosh agreed.
    pub fn vosh_does(&self, opt: u8) -> bool {
        self.vosh[usize::from(opt)] == Side::Yes
    }

    /// Update the cached window size. Call when the renderer reports a resize
    /// after NAWS has been agreed.
    pub fn set_window_size(&mut self, cols: u16, rows: u16) {
        self.window_size = (cols, rows);
    }

    /// Rewind the TTYPE cycle so the next SEND answers from the first
    /// slot. Only tests rewind it, since the session builds a new
    /// Negotiator for each connection.
    #[cfg(test)]
    pub fn reset_ttype_cycle(&mut self) {
        self.ttype_cycle = 0;
    }

    /// Wrap a GMCP wire payload (`package <json>` bytes from
    /// `vosh_gmcp::build`) in `IAC SB GMCP ... IAC SE`. Any literal
    /// 0xFF inside the payload is doubled per the telnet escape rule.
    pub fn build_gmcp_subnegotiation(payload: &[u8]) -> Vec<u8> {
        subnegotiation(option::GMCP, payload)
    }

    /// Build a NAWS subnegotiation block for the current window size. A
    /// size byte that equals 0xFF goes out doubled.
    pub fn naws_subnegotiation(&self) -> Vec<u8> {
        let (cols, rows) = self.window_size;
        let [cols_hi, cols_lo] = cols.to_be_bytes();
        let [rows_hi, rows_lo] = rows.to_be_bytes();
        subnegotiation(option::NAWS, &[cols_hi, cols_lo, rows_hi, rows_lo])
    }

    /// The server offers to perform `opt`. An offer of an option that is
    /// on already, or one Vosh asked for, gets no answer.
    fn respond_will(&mut self, opt: u8) -> Vec<u8> {
        let side = &mut self.server[usize::from(opt)];
        match *side {
            Side::No if wants_server(opt) => {
                *side = Side::Yes;
                vec![IAC, DO, opt]
            }
            Side::No => vec![IAC, DONT, opt],
            Side::Yes => Vec::new(),
            // The answer to Vosh's own DO.
            Side::WantYes => {
                *side = Side::Yes;
                Vec::new()
            }
        }
    }

    /// The server will not perform `opt`, or stops. Only an option that
    /// was on gets the DONT that agrees.
    fn respond_wont(&mut self, opt: u8) -> Vec<u8> {
        let was = std::mem::take(&mut self.server[usize::from(opt)]);
        if was == Side::Yes {
            vec![IAC, DONT, opt]
        } else {
            Vec::new()
        }
    }

    /// The server asks Vosh to perform `opt`. A request for an option
    /// that is on already gets no answer.
    fn respond_do(&mut self, opt: u8) -> Vec<u8> {
        let side = &mut self.vosh[usize::from(opt)];
        match *side {
            Side::No if agrees_to(opt) => {
                *side = Side::Yes;
                let mut out = vec![IAC, WILL, opt];
                if opt == option::NAWS {
                    out.extend(self.naws_subnegotiation());
                }
                out
            }
            Side::No => vec![IAC, WONT, opt],
            // Vosh never offers an option first, so it waits for no DO.
            Side::Yes | Side::WantYes => {
                *side = Side::Yes;
                Vec::new()
            }
        }
    }

    /// The server tells Vosh not to perform `opt`. Only an option that
    /// was on gets the WONT that agrees.
    fn respond_dont(&mut self, opt: u8) -> Vec<u8> {
        let was = std::mem::take(&mut self.vosh[usize::from(opt)]);
        if was == Side::Yes {
            vec![IAC, WONT, opt]
        } else {
            Vec::new()
        }
    }

    fn respond_subnegotiation(&mut self, opt: u8, payload: &[u8]) -> Vec<u8> {
        match opt {
            option::TTYPE => self.respond_ttype(payload),
            option::CHARSET => self.respond_charset(payload),
            option::NEW_ENVIRON => self.respond_new_environ(payload),
            _ => Vec::new(),
        }
    }

    fn respond_ttype(&mut self, payload: &[u8]) -> Vec<u8> {
        if payload.first() != Some(&ttype::SEND) || self.ttype_responses.is_empty() {
            return Vec::new();
        }
        // Per MTTS: each SEND walks to the next slot; once exhausted,
        // keep returning the last (which signals the cycle is complete
        // when the server sees the same answer twice in a row).
        let idx = self.ttype_cycle;
        self.ttype_cycle += 1;
        let slot = idx.min(self.ttype_responses.len() - 1);
        let mut reply = vec![ttype::IS];
        reply.extend_from_slice(self.ttype_responses[slot].as_bytes());
        subnegotiation(option::TTYPE, &reply)
    }

    /// Reply to an `IAC SB NEW-ENVIRON SEND ... IAC SE` request with the
    /// environment variables MUD servers actually look at for color
    /// detection. We send `TERM=xterm-256color` and `COLORTERM=truecolor`
    /// regardless of which variables the server specifically asked for —
    /// RFC 1572 permits returning every value the client knows when the
    /// payload is a bare SEND or lists unsupported variables, and most
    /// MUDs only care about TERM anyway.
    ///
    /// The MTTS bitmask we send via TTYPE already advertises 256-color
    /// and truecolor, but a number of ROM- and Diku-derived servers
    /// gate their per-character color flag on the TERM env var as a
    /// second check, so without this responder the flag flips back to
    /// 16-color on every reconnect.
    fn respond_new_environ(&self, payload: &[u8]) -> Vec<u8> {
        if payload.first() != Some(&new_environ::SEND) {
            return Vec::new();
        }
        let mut reply = vec![new_environ::IS];
        push_environ_var(&mut reply, new_environ::USERVAR, b"TERM", b"xterm-256color");
        push_environ_var(&mut reply, new_environ::USERVAR, b"COLORTERM", b"truecolor");
        subnegotiation(option::NEW_ENVIRON, &reply)
    }

    fn respond_charset(&self, payload: &[u8]) -> Vec<u8> {
        // CHARSET REQUEST: 0x01 SEP charset1 SEP charset2 ...
        if payload.first() != Some(&charset::REQUEST) || payload.len() < 3 {
            return Vec::new();
        }
        let separator = payload[1];
        let charsets = &payload[2..];
        let utf8 = b"UTF-8";
        let accepted = charsets
            .split(|&b| b == separator)
            .any(|cs| cs.eq_ignore_ascii_case(utf8));
        if accepted {
            let mut reply = vec![charset::ACCEPTED];
            reply.extend_from_slice(utf8);
            subnegotiation(option::CHARSET, &reply)
        } else {
            subnegotiation(option::CHARSET, &[charset::REJECTED])
        }
    }
}

/// Frame a subnegotiation as `IAC SB option payload IAC SE`. RFC 854
/// doubles every 0xFF inside a subnegotiation, and doing it here once
/// means no builder escapes IAC on its own.
fn subnegotiation(option: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![IAC, SB, option];
    for &b in payload {
        if b == IAC {
            out.push(IAC);
        }
        out.push(b);
    }
    out.extend_from_slice(&[IAC, SE]);
    out
}

/// Append one NEW-ENVIRON entry (a kind byte, the variable name, a
/// VALUE byte, and the value bytes) onto the running response buffer.
/// Per RFC 1572 the kind/value/escape bytes inside a name or value
/// must be escaped with ESC; in practice TERM and COLORTERM never
/// contain those bytes, so the escape is a belt-and-braces for future
/// callers that might pass arbitrary values.
fn push_environ_var(out: &mut Vec<u8>, kind: u8, name: &[u8], value: &[u8]) {
    out.push(kind);
    push_environ_token(out, name);
    out.push(new_environ::VALUE);
    push_environ_token(out, value);
}

/// Append a name or value with its NEW-ENVIRON control bytes escaped by
/// ESC. An IAC passes through as is, since `subnegotiation` doubles it
/// when it frames the whole response.
fn push_environ_token(out: &mut Vec<u8>, bytes: &[u8]) {
    for &b in bytes {
        if matches!(
            b,
            new_environ::VAR | new_environ::VALUE | new_environ::ESC | new_environ::USERVAR
        ) {
            out.push(new_environ::ESC);
        }
        out.push(b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_unknown_will() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Will(option::MCCP2));
        assert_eq!(bytes, vec![IAC, DONT, option::MCCP2]);
    }

    #[test]
    fn accepts_will_gmcp() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Will(option::GMCP));
        assert_eq!(bytes, vec![IAC, DO, option::GMCP]);
    }

    #[test]
    fn accepts_do_gmcp() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Do(option::GMCP));
        assert_eq!(bytes, vec![IAC, WILL, option::GMCP]);
    }

    #[test]
    fn build_gmcp_subnegotiation_wraps_payload() {
        let bytes = Negotiator::build_gmcp_subnegotiation(b"Core.Hello {}");
        let mut expected = vec![IAC, SB, option::GMCP];
        expected.extend_from_slice(b"Core.Hello {}");
        expected.extend_from_slice(&[IAC, SE]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn build_gmcp_subnegotiation_escapes_iac() {
        let bytes = Negotiator::build_gmcp_subnegotiation(&[b'a', IAC, b'b']);
        let expected = vec![IAC, SB, option::GMCP, b'a', IAC, IAC, b'b', IAC, SE];
        assert_eq!(bytes, expected);
    }

    #[test]
    fn accepts_will_eor() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Will(option::EOR));
        assert_eq!(bytes, vec![IAC, DO, option::EOR]);
    }

    #[test]
    fn accepts_do_ttype() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Do(option::TTYPE));
        assert_eq!(bytes, vec![IAC, WILL, option::TTYPE]);
    }

    #[test]
    fn refuses_do_unknown() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Do(option::MCCP2));
        assert_eq!(bytes, vec![IAC, WONT, option::MCCP2]);
    }

    fn ttype_response_for(text: &[u8]) -> Vec<u8> {
        let mut out = vec![IAC, SB, option::TTYPE, ttype::IS];
        out.extend_from_slice(text);
        out.extend_from_slice(&[IAC, SE]);
        out
    }

    #[test]
    fn ttype_first_send_returns_client_signature() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Subnegotiation {
            option: option::TTYPE,
            payload: vec![ttype::SEND],
        });
        assert_eq!(bytes, ttype_response_for(DEFAULT_TERMINAL_TYPE.as_bytes()));
    }

    #[test]
    fn ttype_cycles_through_mtts_then_repeats() {
        let mut n = Negotiator::new();
        let send = Event::Subnegotiation {
            option: option::TTYPE,
            payload: vec![ttype::SEND],
        };
        assert_eq!(
            n.handle(&send),
            ttype_response_for(DEFAULT_TERMINAL_TYPE.as_bytes()),
        );
        assert_eq!(n.handle(&send), ttype_response_for(b"XTERM-256COLOR"));
        let mtts = format!("MTTS {DEFAULT_MTTS_BITS}");
        assert_eq!(n.handle(&send), ttype_response_for(mtts.as_bytes()));
        // Server keeps asking past the end. We keep returning the last
        // slot so it can confirm the cycle is complete.
        assert_eq!(n.handle(&send), ttype_response_for(mtts.as_bytes()));
        assert_eq!(n.handle(&send), ttype_response_for(mtts.as_bytes()));
    }

    #[test]
    fn ttype_reset_replays_from_first_slot() {
        let mut n = Negotiator::new();
        let send = Event::Subnegotiation {
            option: option::TTYPE,
            payload: vec![ttype::SEND],
        };
        let _ = n.handle(&send);
        let _ = n.handle(&send);
        n.reset_ttype_cycle();
        assert_eq!(
            n.handle(&send),
            ttype_response_for(DEFAULT_TERMINAL_TYPE.as_bytes()),
        );
    }

    #[test]
    fn ttype_slot0_contains_color_markers_for_non_mtts_servers() {
        // Servers that only send a single SB TTYPE SEND (Forsaken
        // Lands, various ROM 2.4 derivatives) substring-scan slot 0
        // for "xterm" / "256" to decide on 256-color. Both markers
        // must stay in the default first response, in lowercase so
        // case-sensitive scans also pass.
        assert!(
            DEFAULT_TERMINAL_TYPE.contains("xterm"),
            "slot 0 lost the lowercase \"xterm\" marker: {DEFAULT_TERMINAL_TYPE}",
        );
        assert!(
            DEFAULT_TERMINAL_TYPE.contains("256"),
            "slot 0 lost the \"256\" marker: {DEFAULT_TERMINAL_TYPE}",
        );
    }

    #[test]
    fn mtts_bits_include_ansi_utf8_256_truecolor() {
        // If anyone changes the constant, the assertion documents which
        // capability bits Vosh actually claims so the change is deliberate.
        assert_eq!(DEFAULT_MTTS_BITS & 1, 1, "ANSI");
        assert_eq!(DEFAULT_MTTS_BITS & 4, 4, "UTF-8");
        assert_eq!(DEFAULT_MTTS_BITS & 8, 8, "256 COLORS");
        assert_eq!(DEFAULT_MTTS_BITS & 256, 256, "TRUE COLOR");
    }

    #[test]
    fn naws_uses_current_window_size() {
        let mut n = Negotiator::new();
        n.set_window_size(132, 50);
        let bytes = n.naws_subnegotiation();
        let expected = vec![IAC, SB, option::NAWS, 0, 132, 0, 50, IAC, SE];
        assert_eq!(bytes, expected);
    }

    #[test]
    fn naws_escapes_iac_in_dimensions() {
        let mut n = Negotiator::new();
        n.set_window_size(255, 24);
        let bytes = n.naws_subnegotiation();
        // Width high byte is 0, low byte is 0xFF and must be doubled.
        let expected = vec![IAC, SB, option::NAWS, 0, IAC, IAC, 0, 24, IAC, SE];
        assert_eq!(bytes, expected);
    }

    #[test]
    fn do_naws_responds_with_will_then_size() {
        let mut n = Negotiator::new();
        n.set_window_size(80, 24);
        let bytes = n.handle(&Event::Do(option::NAWS));
        let mut expected = vec![IAC, WILL, option::NAWS];
        expected.extend(n.naws_subnegotiation());
        assert_eq!(bytes, expected);
    }

    #[test]
    fn charset_request_accepts_utf8() {
        let mut n = Negotiator::new();
        let mut payload = vec![charset::REQUEST, b' '];
        payload.extend_from_slice(b"UTF-8 LATIN-1");
        let bytes = n.handle(&Event::Subnegotiation {
            option: option::CHARSET,
            payload,
        });
        let mut expected = vec![IAC, SB, option::CHARSET, charset::ACCEPTED];
        expected.extend_from_slice(b"UTF-8");
        expected.extend_from_slice(&[IAC, SE]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn accepts_do_new_environ() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Do(option::NEW_ENVIRON));
        assert_eq!(bytes, vec![IAC, WILL, option::NEW_ENVIRON]);
    }

    #[test]
    fn new_environ_send_returns_color_env_vars() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Subnegotiation {
            option: option::NEW_ENVIRON,
            payload: vec![new_environ::SEND],
        });
        let mut expected = vec![IAC, SB, option::NEW_ENVIRON, new_environ::IS];
        expected.push(new_environ::USERVAR);
        expected.extend_from_slice(b"TERM");
        expected.push(new_environ::VALUE);
        expected.extend_from_slice(b"xterm-256color");
        expected.push(new_environ::USERVAR);
        expected.extend_from_slice(b"COLORTERM");
        expected.push(new_environ::VALUE);
        expected.extend_from_slice(b"truecolor");
        expected.extend_from_slice(&[IAC, SE]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn new_environ_ignores_non_send_payloads() {
        let mut n = Negotiator::new();
        let bytes = n.handle(&Event::Subnegotiation {
            option: option::NEW_ENVIRON,
            payload: vec![new_environ::IS],
        });
        let leftover = &bytes;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_repeated_will_is_acknowledged_once() {
        let mut n = Negotiator::new();
        assert_eq!(
            n.handle(&Event::Will(option::GMCP)),
            vec![IAC, DO, option::GMCP]
        );
        let leftover = &n.handle(&Event::Will(option::GMCP));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(n.handle(&Event::Will(option::ECHO)).len(), 3);
        let leftover = &n.handle(&Event::Will(option::ECHO));
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_repeated_do_is_acknowledged_once() {
        let mut n = Negotiator::new();
        assert_eq!(
            n.handle(&Event::Do(option::TTYPE)),
            vec![IAC, WILL, option::TTYPE]
        );
        let leftover = &n.handle(&Event::Do(option::TTYPE));
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_refused_option_is_refused_each_time_it_is_offered() {
        // Refusing changes no state, so the answer never loops.
        let mut n = Negotiator::new();
        assert_eq!(
            n.handle(&Event::Will(option::MCCP2)),
            vec![IAC, DONT, option::MCCP2]
        );
        assert_eq!(
            n.handle(&Event::Will(option::MCCP2)),
            vec![IAC, DONT, option::MCCP2]
        );
        assert_eq!(
            n.handle(&Event::Do(option::MXP)),
            vec![IAC, WONT, option::MXP]
        );
    }

    #[test]
    fn wont_and_dont_for_an_option_that_is_off_get_no_answer() {
        let mut n = Negotiator::new();
        let leftover = &n.handle(&Event::Wont(option::ECHO));
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &n.handle(&Event::Dont(option::NAWS));
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &n.handle(&Event::Wont(option::MCCP2));
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &n.handle(&Event::Dont(option::MXP));
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn wont_and_dont_turn_an_option_off_with_one_answer() {
        let mut n = Negotiator::new();
        let _ = n.handle(&Event::Will(option::ECHO));
        assert_eq!(
            n.handle(&Event::Wont(option::ECHO)),
            vec![IAC, DONT, option::ECHO]
        );
        let leftover = &n.handle(&Event::Wont(option::ECHO));
        assert!(leftover.is_empty(), "{leftover:?}");
        // The server can turn it on again, as a password prompt does.
        assert_eq!(
            n.handle(&Event::Will(option::ECHO)),
            vec![IAC, DO, option::ECHO]
        );
        let _ = n.handle(&Event::Do(option::NAWS));
        assert_eq!(
            n.handle(&Event::Dont(option::NAWS)),
            vec![IAC, WONT, option::NAWS]
        );
        let leftover = &n.handle(&Event::Dont(option::NAWS));
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_server_that_answers_each_do_eor_once_ends_negotiation_in_one_round() {
        // Vosh asks for EOR as it connects. A server that answers every
        // DO EOR with WILL EOR hears nothing more, so the asking stops
        // after one round instead of going back and forth.
        let mut n = Negotiator::new();
        assert_eq!(n.ask(option::EOR), vec![IAC, DO, option::EOR]);
        assert!(!n.server_does(option::EOR));
        let leftover = &n.handle(&Event::Will(option::EOR));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(n.server_does(option::EOR));
        // Asking again once it is on sends nothing.
        let leftover = &n.ask(option::EOR);
        assert!(leftover.is_empty(), "{leftover:?}");
        // A server that offered it before it read the ask, then answered
        // the ask too, ends the same way.
        let mut n = Negotiator::new();
        let _ = n.ask(option::EOR);
        let leftover = &n.handle(&Event::Will(option::EOR));
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &n.handle(&Event::Will(option::EOR));
        assert!(leftover.is_empty(), "{leftover:?}");
        // A server that will not, says so once and hears nothing back.
        let mut n = Negotiator::new();
        let _ = n.ask(option::EOR);
        let leftover = &n.handle(&Event::Wont(option::EOR));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(!n.server_does(option::EOR));
    }

    #[test]
    fn the_state_of_each_side_follows_the_answers() {
        let mut n = Negotiator::new();
        assert!(!n.server_does(option::GMCP));
        let _ = n.handle(&Event::Will(option::GMCP));
        assert!(n.server_does(option::GMCP));
        assert!(!n.vosh_does(option::NAWS));
        let _ = n.handle(&Event::Do(option::NAWS));
        assert!(n.vosh_does(option::NAWS));
        let _ = n.handle(&Event::Dont(option::NAWS));
        assert!(!n.vosh_does(option::NAWS));
        // A refused offer leaves the option off.
        let _ = n.handle(&Event::Will(option::MCCP2));
        assert!(!n.server_does(option::MCCP2));
    }

    #[test]
    fn charset_request_rejects_when_no_utf8() {
        let mut n = Negotiator::new();
        let mut payload = vec![charset::REQUEST, b' '];
        payload.extend_from_slice(b"LATIN-1 ASCII");
        let bytes = n.handle(&Event::Subnegotiation {
            option: option::CHARSET,
            payload,
        });
        assert_eq!(
            bytes,
            vec![IAC, SB, option::CHARSET, charset::REJECTED, IAC, SE]
        );
    }
}
