//! Lines you type while the server hides your input.
//!
//! A server that sends IAC WILL ECHO takes over echoing what you type,
//! and IAC WONT ECHO hands echo back. ROM derivatives, Aabahran among
//! them, take echo for every password prompt. Vosh treats a line sent
//! while the server holds echo as a secret and writes its text nowhere.
//! The log rows for what you send, hidden or not, come from
//! `vosh_log::sent_rows`.

use vosh_protocol::telnet::{option as telnet_option, Event as TelnetEvent};

/// Who echoes your input on one connection. The session task owns one
/// per connection. It flips the state as it handles each WILL or WONT
/// ECHO, in order with the rest of a read, and reads it as each
/// outgoing line leaves. So a line is judged by the state in force when
/// its bytes go out, never by a copy that lags behind the wire. A new
/// connection starts a new one with echo yours, so a session that
/// dropped mid prompt leaves nothing behind for the next.
#[derive(Debug, Default)]
pub(crate) struct ServerEcho {
    held: bool,
}

impl ServerEcho {
    /// Note one telnet event. Returns the new state when the event is a
    /// WILL or WONT ECHO, so the session can tell the input row to mask
    /// or unmask. Every other event returns None.
    pub(crate) fn observe(&mut self, event: &TelnetEvent) -> Option<bool> {
        match event {
            TelnetEvent::Will(opt) if *opt == telnet_option::ECHO => {
                self.held = true;
                Some(true)
            }
            TelnetEvent::Wont(opt) if *opt == telnet_option::ECHO => {
                self.held = false;
                Some(false)
            }
            _ => None,
        }
    }

    /// Whether a line leaving now is hidden input. That is true while
    /// the server holds echo, and always for a `masked` line, one typed
    /// into the masked password field. The masked case covers the
    /// moment after the server hands echo back and before the input row
    /// unmasks, when what you submit was still typed as a secret.
    pub(crate) fn hides(&self, masked: bool) -> bool {
        masked || self.held
    }
}

/// The wire bytes for a line typed into the masked password field: the
/// line exactly as typed, then a line end. It never runs through the
/// input pipeline, so no alias, variable, `;` split, target keyword,
/// macro recording, Lua alias body, or `#` command sees a password or
/// echoes any part of it back to the terminal.
pub(crate) fn masked_line_bytes(line: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(line.len() + 2);
    bytes.extend_from_slice(line.as_bytes());
    bytes.extend_from_slice(b"\r\n");
    bytes
}

#[cfg(test)]
mod tests {
    use super::{masked_line_bytes, ServerEcho};
    use vosh_protocol::telnet::{codes, option, Parser, IAC};

    // Made up values only. None of these is anyone's password.
    const SECRET: &str = "Tr0ub4dor&3";
    const WILL_ECHO: [u8; 3] = [IAC, codes::WILL, option::ECHO];
    const WONT_ECHO: [u8; 3] = [IAC, codes::WONT, option::ECHO];

    /// One connection as the session task sees it: the server reads go
    /// through the telnet parser and the echo state in order, and each
    /// send is logged by the state in force when it leaves.
    struct Session {
        parser: Parser,
        echo: ServerEcho,
        store: vosh_log::LogStore,
        id: i64,
    }

    impl Session {
        fn start(store: vosh_log::LogStore) -> Self {
            let mut store = store;
            let id = store.start_session("mud.example", 4000, 0).unwrap();
            Self {
                parser: Parser::new(),
                echo: ServerEcho::default(),
                store,
                id,
            }
        }

        fn read(&mut self, chunk: &[u8]) {
            for event in self.parser.feed(chunk) {
                let _ = self.echo.observe(&event);
            }
        }

        /// A line from the input pipeline, as `SessionHandle::send`
        /// queues it.
        fn send(&mut self, line: &str) {
            self.leave(format!("{line}\r\n").as_bytes(), false);
        }

        /// A line typed into the masked field, as `session_send_masked`
        /// queues it.
        fn send_masked(&mut self, line: &str) {
            self.leave(&masked_line_bytes(line), true);
        }

        fn leave(&mut self, bytes: &[u8], masked: bool) {
            let rows = vosh_log::sent_rows(bytes, self.echo.hides(masked));
            let entries: Vec<_> =
                vosh_log::sent_entries(self.id, 1, rows, vosh_log::LineKind::Sent).collect();
            self.store.append_batch(&entries).expect("the rows go in");
        }

        fn log(&self) -> String {
            self.store.export_session(self.id, false).unwrap()
        }
    }

    fn chunk(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn a_password_prompt_logs_the_next_command_and_never_the_password() {
        let mut s = Session::start(vosh_log::LogStore::in_memory().unwrap());
        s.read(b"Account name: ");
        s.send("wanderer");
        s.read(&WILL_ECHO);
        s.read(b"Password: ");
        s.send(SECRET);
        s.read(&WONT_ECHO);
        s.read(b"\r\nWelcome back.\r\n");
        s.send("look");

        let log = s.log();
        assert!(!log.contains(SECRET), "the password reached the log: {log}");
        assert_eq!(log, "> wanderer\n> (hidden)\n> look\n");
    }

    #[test]
    fn a_password_sent_after_the_read_that_brings_will_echo_is_hidden() {
        // The prompt, the WILL ECHO, and more output arrive in one read,
        // and the hand back comes in one read with the game's reply. The
        // state each read leaves is the one the next send meets.
        let mut s = Session::start(vosh_log::LogStore::in_memory().unwrap());
        s.read(&chunk(&[
            b"Account accepted.\r\n",
            &WILL_ECHO,
            b"Password: ",
        ]));
        s.send(SECRET);
        s.read(&chunk(&[&WONT_ECHO, b"\r\nWelcome back.\r\n> "]));
        s.send("score");

        let log = s.log();
        assert!(!log.contains(SECRET), "the password reached the log: {log}");
        assert_eq!(log, "> (hidden)\n> score\n");
    }

    #[test]
    fn a_will_echo_split_across_reads_still_hides_the_password() {
        let mut s = Session::start(vosh_log::LogStore::in_memory().unwrap());
        s.read(&[IAC]);
        s.read(&[codes::WILL]);
        s.read(&chunk(&[&[option::ECHO], b"Password: "]));
        s.send(SECRET);

        assert_eq!(s.log(), "> (hidden)\n");
    }

    #[test]
    fn a_disconnect_mid_prompt_leaves_the_next_session_logging_normally() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logs.sqlite");

        let mut first = Session::start(vosh_log::LogStore::open(&path).unwrap());
        first.read(b"Account name: ");
        first.send("wanderer");
        first.read(&chunk(&[&WILL_ECHO, b"Password: "]));
        first.send(SECRET);
        first.read(&chunk(&[&WILL_ECHO, b"Retype password: "]));
        let first_id = first.id;
        // The connection drops at the second prompt, with the server
        // still holding echo and no WONT ECHO to come.
        drop(first);

        let mut next = Session::start(vosh_log::LogStore::open(&path).unwrap());
        next.read(b"Account name: ");
        next.send("wanderer");
        next.send("look");

        assert_eq!(next.log(), "> wanderer\n> look\n");
        let store = vosh_log::LogStore::open(&path).unwrap();
        let earlier = store.export_session(first_id, false).unwrap();
        assert_eq!(earlier, "> wanderer\n> (hidden)\n");
    }

    #[test]
    fn only_echo_negotiation_moves_the_state() {
        let mut echo = ServerEcho::default();
        let mut parser = Parser::new();
        let events = parser.feed(&chunk(&[
            &[IAC, codes::WILL, option::SUPPRESS_GO_AHEAD],
            &[IAC, codes::DO, option::ECHO],
            b"text",
        ]));
        for event in &events {
            assert_eq!(echo.observe(event), None);
        }
        assert!(!echo.hides(false));
    }

    #[test]
    fn a_line_typed_masked_stays_hidden_after_the_server_hands_echo_back() {
        // The server handed echo back, but the input row had not yet
        // unmasked when you pressed Enter.
        let mut s = Session::start(vosh_log::LogStore::in_memory().unwrap());
        s.read(&chunk(&[&WILL_ECHO, b"Password: "]));
        s.read(&WONT_ECHO);
        s.send_masked(SECRET);
        s.send("look");

        let log = s.log();
        assert!(!log.contains(SECRET), "the password reached the log: {log}");
        assert_eq!(log, "> (hidden)\n> look\n");
    }

    #[test]
    fn a_masked_line_goes_out_exactly_as_typed() {
        // A password can hold characters the input pipeline acts on. From
        // the masked field none of them runs as an alias, a variable, a
        // command separator, a target keyword, or a # command.
        for line in ["#tr0ub4dor", "pa;ss$word", "tarn", "a b c", ""] {
            assert_eq!(masked_line_bytes(line), format!("{line}\r\n").into_bytes());
        }
    }
}
