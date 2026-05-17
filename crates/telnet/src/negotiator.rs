//! Default policy for telnet option negotiation. Decides which options to
//! accept and produces response bytes for the session loop to send.

use crate::codes::{charset, option, ttype, DO, DONT, IAC, SB, SE, WILL, WONT};
use crate::parser::Event;

/// Default terminal type advertised in TTYPE subnegotiation.
pub const DEFAULT_TERMINAL_TYPE: &str = "MUDCLIENT";

#[derive(Debug, Clone)]
pub struct Negotiator {
    pub terminal_type: String,
    pub window_size: (u16, u16),
}

impl Default for Negotiator {
    fn default() -> Self {
        Self {
            terminal_type: DEFAULT_TERMINAL_TYPE.to_string(),
            window_size: (80, 24),
        }
    }
}

impl Negotiator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Map a parser event to bytes that the session loop should send back to
    /// the server. Returns an empty Vec when no response is needed.
    pub fn handle(&self, event: &Event) -> Vec<u8> {
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

    /// Update the cached window size. Call when the renderer reports a resize
    /// after NAWS has been agreed.
    pub fn set_window_size(&mut self, cols: u16, rows: u16) {
        self.window_size = (cols, rows);
    }

    /// Wrap a GMCP wire payload (`package <json>` bytes from
    /// `vosh_gmcp::build`) in `IAC SB GMCP ... IAC SE`. Any literal
    /// 0xFF inside the payload is doubled per the telnet escape rule.
    pub fn build_gmcp_subnegotiation(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![IAC, SB, option::GMCP];
        for &b in payload {
            if b == IAC {
                out.push(IAC);
            }
            out.push(b);
        }
        out.extend_from_slice(&[IAC, SE]);
        out
    }

    /// Build a NAWS subnegotiation block for the current window size, with
    /// the IAC IAC escaping required when any byte happens to equal 0xFF.
    pub fn naws_subnegotiation(&self) -> Vec<u8> {
        let (cols, rows) = self.window_size;
        let bytes = [
            cols.to_be_bytes()[0],
            cols.to_be_bytes()[1],
            rows.to_be_bytes()[0],
            rows.to_be_bytes()[1],
        ];
        let mut out = vec![IAC, SB, option::NAWS];
        for b in bytes {
            if b == IAC {
                out.push(IAC);
            }
            out.push(b);
        }
        out.extend_from_slice(&[IAC, SE]);
        out
    }

    fn respond_will(&self, opt: u8) -> Vec<u8> {
        // Server says it WILL do something. Reply DO if we want it.
        match opt {
            option::EOR | option::SUPPRESS_GO_AHEAD | option::ECHO | option::GMCP => {
                vec![IAC, DO, opt]
            }
            _ => vec![IAC, DONT, opt],
        }
    }

    fn respond_wont(&self, opt: u8) -> Vec<u8> {
        // Server retracts. Acknowledge with DONT.
        vec![IAC, DONT, opt]
    }

    fn respond_do(&self, opt: u8) -> Vec<u8> {
        // Server requests we DO an option.
        match opt {
            option::TTYPE | option::CHARSET | option::SUPPRESS_GO_AHEAD | option::GMCP => {
                vec![IAC, WILL, opt]
            }
            option::NAWS => {
                let mut out = vec![IAC, WILL, option::NAWS];
                out.extend(self.naws_subnegotiation());
                out
            }
            _ => vec![IAC, WONT, opt],
        }
    }

    fn respond_dont(&self, opt: u8) -> Vec<u8> {
        // Server tells us not to. Acknowledge.
        vec![IAC, WONT, opt]
    }

    fn respond_subnegotiation(&self, opt: u8, payload: &[u8]) -> Vec<u8> {
        match opt {
            option::TTYPE => self.respond_ttype(payload),
            option::CHARSET => self.respond_charset(payload),
            _ => Vec::new(),
        }
    }

    fn respond_ttype(&self, payload: &[u8]) -> Vec<u8> {
        if payload.first() != Some(&ttype::SEND) {
            return Vec::new();
        }
        let mut out = vec![IAC, SB, option::TTYPE, ttype::IS];
        out.extend_from_slice(self.terminal_type.as_bytes());
        out.extend_from_slice(&[IAC, SE]);
        out
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
            let mut out = vec![IAC, SB, option::CHARSET, charset::ACCEPTED];
            out.extend_from_slice(utf8);
            out.extend_from_slice(&[IAC, SE]);
            out
        } else {
            vec![IAC, SB, option::CHARSET, charset::REJECTED, IAC, SE]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_unknown_will() {
        let n = Negotiator::new();
        let bytes = n.handle(&Event::Will(option::MCCP2));
        assert_eq!(bytes, vec![IAC, DONT, option::MCCP2]);
    }

    #[test]
    fn accepts_will_gmcp() {
        let n = Negotiator::new();
        let bytes = n.handle(&Event::Will(option::GMCP));
        assert_eq!(bytes, vec![IAC, DO, option::GMCP]);
    }

    #[test]
    fn accepts_do_gmcp() {
        let n = Negotiator::new();
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
        let n = Negotiator::new();
        let bytes = n.handle(&Event::Will(option::EOR));
        assert_eq!(bytes, vec![IAC, DO, option::EOR]);
    }

    #[test]
    fn accepts_do_ttype() {
        let n = Negotiator::new();
        let bytes = n.handle(&Event::Do(option::TTYPE));
        assert_eq!(bytes, vec![IAC, WILL, option::TTYPE]);
    }

    #[test]
    fn refuses_do_unknown() {
        let n = Negotiator::new();
        let bytes = n.handle(&Event::Do(option::MCCP2));
        assert_eq!(bytes, vec![IAC, WONT, option::MCCP2]);
    }

    #[test]
    fn responds_to_ttype_send() {
        let mut n = Negotiator::new();
        n.terminal_type = "XTERM-256COLOR".to_string();
        let bytes = n.handle(&Event::Subnegotiation {
            option: option::TTYPE,
            payload: vec![ttype::SEND],
        });
        let mut expected = vec![IAC, SB, option::TTYPE, ttype::IS];
        expected.extend_from_slice(b"XTERM-256COLOR");
        expected.extend_from_slice(&[IAC, SE]);
        assert_eq!(bytes, expected);
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
        let n = Negotiator::new();
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
    fn charset_request_rejects_when_no_utf8() {
        let n = Negotiator::new();
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
