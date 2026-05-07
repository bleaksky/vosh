//! Streaming telnet parser. Feed bytes, get events.

use crate::codes::{DO, DONT, IAC, SB, SE, WILL, WONT};

/// Events emitted by the parser as bytes flow through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// User data bytes that should be passed through to the renderer.
    Data(Vec<u8>),
    /// Server announces it WILL do an option.
    Will(u8),
    /// Server announces it WONT do an option.
    Wont(u8),
    /// Server requests we DO an option.
    Do(u8),
    /// Server requests we DONT do an option.
    Dont(u8),
    /// Subnegotiation payload for an option, IAC SE terminated.
    Subnegotiation { option: u8, payload: Vec<u8> },
    /// Single-byte command after IAC (GA, EOR, NOP, AYT, IP, AO, EL, EC, BRK, DM).
    Command(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Stream,
    Iac,
    Will,
    Wont,
    Do,
    Dont,
    SbOption,
    SbData,
    SbIac,
}

/// Streaming telnet parser. Holds enough state across calls to handle IAC
/// sequences that straddle byte chunks.
#[derive(Debug, Clone)]
pub struct Parser {
    state: State,
    sb_option: u8,
    sb_buffer: Vec<u8>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self {
            state: State::Stream,
            sb_option: 0,
            sb_buffer: Vec::new(),
        }
    }

    /// Feed raw bytes from the socket. Returns one or more events in order.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut events = Vec::new();
        let mut data_buf: Vec<u8> = Vec::new();

        for &b in bytes {
            match self.state {
                State::Stream => {
                    if b == IAC {
                        self.state = State::Iac;
                    } else {
                        data_buf.push(b);
                    }
                }
                State::Iac => match b {
                    IAC => {
                        // Escaped IAC. Literal 0xFF stays in the data stream
                        // without splitting the surrounding Data event.
                        data_buf.push(0xFF);
                        self.state = State::Stream;
                    }
                    WILL => {
                        flush_data(&mut data_buf, &mut events);
                        self.state = State::Will;
                    }
                    WONT => {
                        flush_data(&mut data_buf, &mut events);
                        self.state = State::Wont;
                    }
                    DO => {
                        flush_data(&mut data_buf, &mut events);
                        self.state = State::Do;
                    }
                    DONT => {
                        flush_data(&mut data_buf, &mut events);
                        self.state = State::Dont;
                    }
                    SB => {
                        flush_data(&mut data_buf, &mut events);
                        self.state = State::SbOption;
                    }
                    other => {
                        flush_data(&mut data_buf, &mut events);
                        events.push(Event::Command(other));
                        self.state = State::Stream;
                    }
                },
                State::Will => {
                    events.push(Event::Will(b));
                    self.state = State::Stream;
                }
                State::Wont => {
                    events.push(Event::Wont(b));
                    self.state = State::Stream;
                }
                State::Do => {
                    events.push(Event::Do(b));
                    self.state = State::Stream;
                }
                State::Dont => {
                    events.push(Event::Dont(b));
                    self.state = State::Stream;
                }
                State::SbOption => {
                    self.sb_option = b;
                    self.sb_buffer.clear();
                    self.state = State::SbData;
                }
                State::SbData => {
                    if b == IAC {
                        self.state = State::SbIac;
                    } else {
                        self.sb_buffer.push(b);
                    }
                }
                State::SbIac => match b {
                    IAC => {
                        // Escaped IAC inside subnegotiation. Literal 0xFF.
                        self.sb_buffer.push(0xFF);
                        self.state = State::SbData;
                    }
                    SE => {
                        events.push(Event::Subnegotiation {
                            option: self.sb_option,
                            payload: std::mem::take(&mut self.sb_buffer),
                        });
                        self.state = State::Stream;
                    }
                    _ => {
                        // Malformed SB. Drop the subnegotiation and resync.
                        self.sb_buffer.clear();
                        self.state = State::Stream;
                    }
                },
            }
        }

        if !data_buf.is_empty() {
            events.push(Event::Data(data_buf));
        }

        events
    }

    /// Reset the parser to a fresh state. Use when the connection drops.
    pub fn reset(&mut self) {
        self.state = State::Stream;
        self.sb_option = 0;
        self.sb_buffer.clear();
    }
}

fn flush_data(buf: &mut Vec<u8>, events: &mut Vec<Event>) {
    if !buf.is_empty() {
        events.push(Event::Data(std::mem::take(buf)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::option;

    #[test]
    fn passes_plain_data_through() {
        let mut p = Parser::new();
        let events = p.feed(b"hello");
        assert_eq!(events, vec![Event::Data(b"hello".to_vec())]);
    }

    #[test]
    fn parses_iac_will() {
        let mut p = Parser::new();
        let events = p.feed(&[IAC, WILL, option::TTYPE]);
        assert_eq!(events, vec![Event::Will(option::TTYPE)]);
    }

    #[test]
    fn parses_iac_do() {
        let mut p = Parser::new();
        let events = p.feed(&[IAC, DO, option::NAWS]);
        assert_eq!(events, vec![Event::Do(option::NAWS)]);
    }

    #[test]
    fn parses_iac_dont_and_wont() {
        let mut p = Parser::new();
        let events = p.feed(&[IAC, WONT, option::ECHO, IAC, DONT, option::NAWS]);
        assert_eq!(
            events,
            vec![Event::Wont(option::ECHO), Event::Dont(option::NAWS)]
        );
    }

    #[test]
    fn escaped_iac_in_data() {
        let mut p = Parser::new();
        let events = p.feed(&[b'a', IAC, IAC, b'b']);
        assert_eq!(events, vec![Event::Data(vec![b'a', 0xFF, b'b'])]);
    }

    #[test]
    fn parses_subnegotiation() {
        let mut p = Parser::new();
        let bytes = [
            IAC,
            SB,
            option::TTYPE,
            crate::codes::ttype::SEND,
            IAC,
            SE,
        ];
        let events = p.feed(&bytes);
        assert_eq!(
            events,
            vec![Event::Subnegotiation {
                option: option::TTYPE,
                payload: vec![crate::codes::ttype::SEND],
            }]
        );
    }

    #[test]
    fn escaped_iac_in_subnegotiation() {
        let mut p = Parser::new();
        let bytes = [
            IAC,
            SB,
            option::GMCP,
            b'a',
            IAC,
            IAC,
            b'b',
            IAC,
            SE,
        ];
        let events = p.feed(&bytes);
        assert_eq!(
            events,
            vec![Event::Subnegotiation {
                option: option::GMCP,
                payload: vec![b'a', 0xFF, b'b'],
            }]
        );
    }

    #[test]
    fn handles_iac_split_across_chunks() {
        let mut p = Parser::new();
        let mut events = p.feed(&[b'x', IAC]);
        events.extend(p.feed(&[WILL, option::EOR]));
        assert_eq!(
            events,
            vec![Event::Data(b"x".to_vec()), Event::Will(option::EOR)]
        );
    }

    #[test]
    fn handles_data_around_negotiation() {
        let mut p = Parser::new();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"hi ");
        bytes.extend_from_slice(&[IAC, WILL, option::TTYPE]);
        bytes.extend_from_slice(b" world");
        let events = p.feed(&bytes);
        assert_eq!(
            events,
            vec![
                Event::Data(b"hi ".to_vec()),
                Event::Will(option::TTYPE),
                Event::Data(b" world".to_vec()),
            ]
        );
    }

    #[test]
    fn parses_single_byte_commands() {
        let mut p = Parser::new();
        let events = p.feed(&[IAC, crate::codes::GA, IAC, crate::codes::EOR]);
        assert_eq!(
            events,
            vec![
                Event::Command(crate::codes::GA),
                Event::Command(crate::codes::EOR),
            ]
        );
    }

    #[test]
    fn malformed_subnegotiation_resyncs() {
        let mut p = Parser::new();
        let bytes = [IAC, SB, option::GMCP, b'x', IAC, b'?', b'y'];
        let events = p.feed(&bytes);
        // SB was abandoned. The trailing 'y' is normal data.
        assert_eq!(events, vec![Event::Data(b"y".to_vec())]);
    }

    #[test]
    fn reset_clears_state() {
        let mut p = Parser::new();
        let _ = p.feed(&[IAC, SB, option::GMCP, b'x']);
        p.reset();
        let events = p.feed(b"clean");
        assert_eq!(events, vec![Event::Data(b"clean".to_vec())]);
    }
}
