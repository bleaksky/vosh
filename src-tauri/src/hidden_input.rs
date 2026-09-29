//! Lines you type while the server hides your input.
//!
//! A server that sends IAC WILL ECHO takes over echoing what you type,
//! and IAC WONT ECHO hands echo back. ROM derivatives, Aabahran among
//! them, take echo for every password prompt. Vosh treats a line sent
//! while the server holds echo as a secret and writes its text nowhere.

use tracing::warn;
use vosh_telnet::{option as telnet_option, Event as TelnetEvent};

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

    /// Whether the server holds echo, so a line leaving now is hidden
    /// input.
    pub(crate) fn held(&self) -> bool {
        self.held
    }
}

/// What the session log keeps of a line sent while your input is
/// hidden.
pub(crate) const HIDDEN_ROW: &str = "> (hidden)";

/// The session log rows for bytes sent to the server. The wire payload
/// is one or more commands ended by `\r\n`. Each command becomes one
/// `> ` row so a reader tells input from output at a glance, and blank
/// lines (a bare Enter) leave no row.
///
/// The rule for hidden input. While `hidden` is true every command
/// becomes the fixed row [`HIDDEN_ROW`] and its text is dropped here,
/// before anything reaches the store. The row keeps the fact that a
/// line went out, so a login in the transcript still reads as the
/// prompt, your answer, and the game's reply, and a failed login shows
/// where the answer went. It carries nothing of the line, not even its
/// length. Leaving the row out entirely would hide that a line was sent
/// and protect nothing more.
pub(crate) fn sent_log_rows(bytes: &[u8], hidden: bool) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut rows = Vec::new();
    for raw in text.split('\n') {
        let line = raw.trim_end_matches('\r').trim_end();
        if line.is_empty() {
            continue;
        }
        rows.push(if hidden {
            HIDDEN_ROW.to_string()
        } else {
            format!("> {line}")
        });
    }
    rows
}

/// Append the rows for one send to a session in the log store.
pub(crate) fn append_sent_rows(
    store: &mut vosh_log::LogStore,
    session_id: i64,
    ts_ms: i64,
    rows: &[String],
) {
    for row in rows {
        if let Err(e) = store.append(session_id, ts_ms, row, None) {
            warn!(error = %e, "log append (input) failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{append_sent_rows, sent_log_rows, ServerEcho};
    use vosh_telnet::{codes, option, Parser, IAC};

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

        fn send(&mut self, line: &str) {
            let rows = sent_log_rows(format!("{line}\r\n").as_bytes(), self.echo.held());
            append_sent_rows(&mut self.store, self.id, 1, &rows);
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
    fn a_hidden_send_of_several_lines_keeps_one_row_each_and_no_text() {
        let rows = sent_log_rows(format!("{SECRET}\r\n\r\n{SECRET}\r\n").as_bytes(), true);
        assert_eq!(rows, vec!["> (hidden)", "> (hidden)"]);
        assert_eq!(sent_log_rows(b"\r\n", true), Vec::<String>::new());
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
        assert!(!echo.held());
    }
}
