//! How a profile reads the game's text, and writes yours. Every consumer
//! after the telnet layer, the line accumulator, the ANSI parser, the
//! triggers, the log, the scrollback and both renderers, takes UTF-8 and
//! used to read the bytes on its own, each a little differently. So the
//! bytes are read once here, as each piece of text leaves the telnet
//! parser, and everything after it sees the same valid UTF-8.
//!
//! [`Charset::Utf8`], the default, takes UTF-8 and reads a byte that is
//! not UTF-8 as Windows Latin 1, the form browsers read the name Latin 1
//! as, so an older world's accented letters and curly quotes still show.
//! Text that is UTF-8 already, nearly all of it, goes through untouched.
//! [`Charset::Latin1`] reads every byte as Windows Latin 1, for a world
//! that never sends UTF-8, and writes what you send the same way.
//!
//! A character can arrive in two reads. The start of one at the end of a
//! read waits for the next, or for the game's GA or EOR, which end a
//! prompt and so let it out as it stands.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use vosh_protocol::telnet::{codes as telnet_codes, Event};

/// How a profile reads the game's text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Charset {
    /// UTF-8, with a byte that is not UTF-8 read as Windows Latin 1.
    #[default]
    Utf8,
    /// Windows Latin 1 alone.
    Latin1,
}

impl Charset {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::Utf8
    }
}

/// The characters Windows Latin 1 puts at 0x80 to 0x9F, where ISO 8859-1
/// has controls. The five it leaves open read as the controls.
const WINDOWS_80_9F: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// The character Windows Latin 1 gives `byte`.
fn latin1_char(byte: u8) -> char {
    match byte {
        0x80..=0x9F => WINDOWS_80_9F[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

/// The byte Windows Latin 1 gives `c`, None for one it lacks.
fn latin1_byte(c: char) -> Option<u8> {
    let code = u32::from(c);
    if code < 0x80 || (0xA0..=0xFF).contains(&code) {
        return u8::try_from(code).ok();
    }
    WINDOWS_80_9F
        .iter()
        .position(|&w| w == c)
        .and_then(|at| u8::try_from(0x80 + at).ok())
}

fn push_latin1(out: &mut Vec<u8>, bytes: &[u8]) {
    for &byte in bytes {
        if byte.is_ascii() {
            out.push(byte);
        } else {
            let mut buf = [0u8; 4];
            out.extend_from_slice(latin1_char(byte).encode_utf8(&mut buf).as_bytes());
        }
    }
}

/// The reader of one connection: its charset and the start of a
/// character a read ended on.
#[derive(Debug, Default)]
pub(crate) struct Decoder {
    charset: Charset,
    /// The start of a UTF-8 character the last read ended on.
    pending: Vec<u8>,
}

impl Decoder {
    pub(crate) fn new(charset: Charset) -> Self {
        Self {
            charset,
            pending: Vec::new(),
        }
    }

    /// `bytes` as valid UTF-8, borrowed when they are already.
    pub(crate) fn decode<'a>(&mut self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        if self.charset == Charset::Utf8
            && self.pending.is_empty()
            && std::str::from_utf8(bytes).is_ok()
        {
            return Cow::Borrowed(bytes);
        }
        let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 2);
        if self.charset == Charset::Latin1 {
            push_latin1(&mut out, bytes);
            return Cow::Owned(out);
        }
        let joined;
        let mut rest: &[u8] = if self.pending.is_empty() {
            bytes
        } else {
            let mut both = std::mem::take(&mut self.pending);
            both.extend_from_slice(bytes);
            joined = both;
            &joined
        };
        loop {
            match std::str::from_utf8(rest) {
                Ok(text) => {
                    out.extend_from_slice(text.as_bytes());
                    break;
                }
                Err(e) => {
                    let (good, bad) = rest.split_at(e.valid_up_to());
                    out.extend_from_slice(good);
                    match e.error_len() {
                        Some(len) => {
                            push_latin1(&mut out, &bad[..len]);
                            rest = &bad[len..];
                        }
                        None => {
                            // The start of a character the next read
                            // ends, at most three bytes.
                            self.pending = bad.to_vec();
                            break;
                        }
                    }
                }
            }
        }
        Cow::Owned(out)
    }

    /// Let out the start of a character that waits, read as Latin 1,
    /// since the game ended its text there.
    pub(crate) fn flush(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        push_latin1(&mut out, &std::mem::take(&mut self.pending));
        out
    }

    /// The telnet `events` of one read with their text read, and the
    /// start of a character let out before a GA or an EOR. Hands them
    /// back as they came when every piece of text is UTF-8 already.
    pub(crate) fn events(&mut self, events: Vec<Event>) -> Vec<Event> {
        let plain = self.charset == Charset::Utf8
            && self.pending.is_empty()
            && events.iter().all(|event| match event {
                Event::Data(data) => std::str::from_utf8(data).is_ok(),
                _ => true,
            });
        if plain {
            return events;
        }
        let mut out = Vec::with_capacity(events.len() + 1);
        for event in events {
            match event {
                Event::Data(data) => {
                    let text = self.decode(&data).into_owned();
                    if !text.is_empty() {
                        out.push(Event::Data(text));
                    }
                }
                Event::Command(command)
                    if command == telnet_codes::GA || command == telnet_codes::EOR =>
                {
                    let rest = self.flush();
                    if !rest.is_empty() {
                        out.push(Event::Data(rest));
                    }
                    out.push(Event::Command(command));
                }
                other => out.push(other),
            }
        }
        out
    }
}

/// `buf`, a line Vosh sends as UTF-8, as `charset` writes it. Latin 1
/// writes each character as its byte, a `?` for one it lacks, and
/// doubles a 0xFF, which telnet would read as IAC. Bytes that are not
/// UTF-8 go as they are, since only a telnet command, with its IAC,
/// holds them.
pub(crate) fn encode(charset: Charset, buf: &[u8]) -> Cow<'_, [u8]> {
    if charset == Charset::Utf8 || buf.is_ascii() {
        return Cow::Borrowed(buf);
    }
    let Ok(text) = std::str::from_utf8(buf) else {
        return Cow::Borrowed(buf);
    };
    let mut out = Vec::with_capacity(buf.len());
    for c in text.chars() {
        let byte = latin1_byte(c).unwrap_or(b'?');
        out.push(byte);
        if byte == telnet_codes::IAC {
            out.push(telnet_codes::IAC);
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use vosh_protocol::telnet::{codes, Event};

    use super::{encode, Charset, Decoder};

    fn read(decoder: &mut Decoder, bytes: &[u8]) -> String {
        String::from_utf8(decoder.decode(bytes).into_owned()).expect("valid UTF-8")
    }

    #[test]
    fn utf8_text_goes_through_as_it_came() {
        let mut d = Decoder::new(Charset::Utf8);
        let text = "Maren says, 'café, 日本'\r\n".as_bytes();
        assert!(matches!(d.decode(text), std::borrow::Cow::Borrowed(_)));
        assert_eq!(read(&mut d, text), "Maren says, 'café, 日本'\r\n");
    }

    #[test]
    fn a_byte_that_is_not_utf8_reads_as_windows_latin1() {
        let mut d = Decoder::new(Charset::Utf8);
        // café in Latin 1 inside a colored line, with curly quotes.
        assert_eq!(
            read(&mut d, b"\x1b[1;37mcaf\xe9 au \x93lait\x94\x1b[0m"),
            "\x1b[1;37mcafé au \u{201C}lait\u{201D}\x1b[0m"
        );
    }

    #[test]
    fn a_character_split_across_two_reads_waits_for_the_rest() {
        let mut d = Decoder::new(Charset::Utf8);
        let text = "Orla, 日本".as_bytes();
        let (first, second) = text.split_at(text.len() - 2);
        assert_eq!(read(&mut d, first), "Orla, 日");
        assert_eq!(read(&mut d, second), "本");
    }

    #[test]
    fn a_prompt_that_ends_on_a_latin1_byte_shows_at_its_ga() {
        let mut d = Decoder::new(Charset::Utf8);
        let events = d.events(vec![
            Event::Data(b"Maren> caf\xe9".to_vec()),
            Event::Command(codes::GA),
        ]);
        assert_eq!(
            events,
            [
                Event::Data(b"Maren> caf".to_vec()),
                Event::Data("é".as_bytes().to_vec()),
                Event::Command(codes::GA),
            ]
        );
    }

    #[test]
    fn latin1_reads_every_byte_as_its_character() {
        let mut d = Decoder::new(Charset::Latin1);
        // Bytes that happen to form UTF-8 still read one by one.
        assert_eq!(read(&mut d, b"\xc3\xa9 \xe9 \x80"), "Ã© é €");
    }

    #[test]
    fn latin1_writes_your_text_as_its_bytes() {
        assert_eq!(
            &*encode(Charset::Latin1, "café €5 日\r\n".as_bytes()),
            b"caf\xe9 \x805 ?\r\n"
        );
        // ÿ is 0xFF, which telnet doubles.
        assert_eq!(
            &*encode(Charset::Latin1, "ÿ\r\n".as_bytes()),
            b"\xff\xff\r\n"
        );
        // UTF-8 and telnet commands go as they are.
        assert_eq!(
            &*encode(Charset::Utf8, "café\r\n".as_bytes()),
            "café\r\n".as_bytes()
        );
        assert_eq!(&*encode(Charset::Latin1, &[255, 252, 10]), [255, 252, 10]);
    }
}
