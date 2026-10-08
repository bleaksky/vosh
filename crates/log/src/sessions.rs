//! Starts, ends and lists sessions, writes their lines, and exports one.
//! It also owns the `> ` rows that record what you sent and the rows
//! of the players you snoop.

use rusqlite::params;
#[cfg(any(test, feature = "testkit"))]
use rusqlite::OptionalExtension;
use serde::Serialize;
use vosh_protocol::ansi::plain_text;

use crate::{LogStore, Result, Scope};

#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    pub id: i64,
    pub host: String,
    pub port: u16,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub line_count: i64,
}

/// The columns [`session_row`] reads, in its order, for a query that
/// joins `sessions` as `s`.
const SESSION_COLUMNS: &str = "SELECT s.id, s.host, s.port, s.started_at_ms, s.ended_at_ms,
        (SELECT COUNT(*) FROM log_lines l WHERE l.session_id = s.id)";

/// A [`SessionRow`] from a row that selects [`SESSION_COLUMNS`].
fn session_row(row: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: row.get(0)?,
        host: row.get(1)?,
        port: row.get::<_, i64>(2)? as u16,
        started_at_ms: row.get(3)?,
        ended_at_ms: row.get(4)?,
        line_count: row.get(5)?,
    })
}

/// Hosts the log view leaves out: sessions to this machine.
const LOCAL_HOSTS: [&str; 2] = ["127.0.0.1", "localhost"];

/// True when `host` names this machine, ignoring case, spaces, and a
/// trailing dot. Test only. The log view leaves these sessions out in
/// SQL through [`not_local_sql`], and the tests check both fold a host
/// alike.
#[cfg(test)]
pub(crate) fn is_local_host(host: &str) -> bool {
    let clean = host.trim().trim_end_matches('.').to_ascii_lowercase();
    LOCAL_HOSTS.contains(&clean.as_str())
}

/// The SQL test that keeps a session off this machine, for a query
/// that joins `sessions` as `s`. Built from [`LOCAL_HOSTS`] so the two
/// never drift. It ignores case, spaces, and a trailing dot.
pub(crate) fn not_local_sql() -> String {
    let list = LOCAL_HOSTS
        .iter()
        .map(|h| format!("'{h}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("rtrim(lower(trim(s.host)), '.') NOT IN ({list})")
}

/// One pending log line, owned so the session loop can collect a
/// socket read's worth before flushing them via [`LogStore::append_batch`].
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub session_id: i64,
    pub ts_ms: i64,
    pub text: String,
    pub raw: Option<Vec<u8>>,
}

/// The row the session log keeps for a line you send while your input
/// is hidden. The password wipe blanks an old sent line to the same
/// text, so a blanked line reads like one that was never saved.
pub const HIDDEN_SENT_TEXT: &str = "> (hidden)";

/// The session log rows for bytes sent to the server. The wire payload
/// is one or more commands ended by `\r\n`. Each command becomes one
/// `> ` row so a reader tells input from output at a glance, and blank
/// lines (a bare Enter) leave no row.
///
/// The rule for hidden input. While `hidden` is true every command
/// becomes the fixed row [`HIDDEN_SENT_TEXT`] and its text is dropped
/// here, before anything reaches the store. The row keeps the fact that
/// a line went out, so a login in the transcript still reads as the
/// prompt, your answer, and the game's reply, and a failed login shows
/// where the answer went. It carries nothing of the line, not even its
/// length. Leaving the row out entirely would hide that a line was sent
/// and protect nothing more.
pub fn sent_rows(bytes: &[u8], hidden: bool) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut rows = Vec::new();
    for raw in text.split('\n') {
        let line = raw.trim_end_matches('\r').trim_end();
        if line.is_empty() {
            continue;
        }
        rows.push(if hidden {
            HIDDEN_SENT_TEXT.to_string()
        } else {
            format!("> {line}")
        });
    }
    rows
}

/// The log entries for one send to a session, its rows as plain text
/// with no raw bytes, all at the time the send left.
pub fn sent_entries(
    session_id: i64,
    ts_ms: i64,
    rows: Vec<String>,
) -> impl Iterator<Item = LogEntry> {
    rows.into_iter().map(move |text| LogEntry {
        session_id,
        ts_ms,
        text,
        raw: None,
    })
}

/// The session log rows for whole lines of a snooped player's screen,
/// as the game sent them, each as its plain text and its raw bytes. A
/// row starts with the player's name and a bar, `Tolliver| `, the mark
/// the game puts on each snooped line for a client that has no snoop
/// pane (`snoop_relay` in comm.c). The search reads a regex, so
/// `^Tolliver\|` finds that player's text, and a search anchored on a
/// line's start never takes it for the snooper's own. The
/// raw bytes carry the mark too, so the export with color names the
/// player as well.
///
/// The game ends a line with `\n\r`, so a `\r` on either end of a line
/// goes, as it does for your own lines. Each line that ends in `\n` is a
/// row, a blank one too. Text after the last `\n` is the partial a snoop
/// ended on, and it is a row when anything but `\r` is left of it.
pub fn snoop_rows(name: &str, text: &str) -> Vec<(String, Vec<u8>)> {
    let mut rows = Vec::new();
    let mut pieces = text.split('\n').peekable();
    while let Some(piece) = pieces.next() {
        let line = piece.strip_prefix('\r').unwrap_or(piece);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if pieces.peek().is_none() && line.is_empty() {
            break;
        }
        let raw = format!("{name}| {line}").into_bytes();
        rows.push((plain_text(&raw), raw));
    }
    rows
}

/// The statement that writes one log line, shared by
/// [`LogStore::append`] and [`LogStore::append_batch`] so both hit the
/// same entry in the statement cache.
const INSERT_LINE: &str =
    "INSERT INTO log_lines (session_id, ts_ms, text, raw) VALUES (?1, ?2, ?3, ?4)";

impl LogStore {
    /// Name the character a session belongs to, the first time the game
    /// names it. A session that already has one keeps it.
    pub fn set_session_character(&mut self, session_id: i64, character: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET character = ?1 WHERE id = ?2 AND character IS NULL",
            params![character, session_id],
        )?;
        Ok(())
    }

    /// The character a session belongs to, when the game named one.
    /// Test only. The app's tests read it through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn session_character(&self, session_id: i64) -> Result<Option<String>> {
        let found = self
            .conn
            .query_row(
                "SELECT character FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        Ok(found.flatten())
    }

    /// Open a new session row and return its id.
    pub fn start_session(&mut self, host: &str, port: u16, started_at_ms: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions (host, port, started_at_ms) VALUES (?1, ?2, ?3)",
            params![host, port, started_at_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Mark a session as ended.
    pub fn end_session(&mut self, session_id: i64, ended_at_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET ended_at_ms = ?1 WHERE id = ?2",
            params![ended_at_ms, session_id],
        )?;
        Ok(())
    }

    /// Append one line to a session. `raw` may carry ANSI codes; `text`
    /// is the plain-text form. When `raw` is None, the plain text doubles
    /// as the raw payload on export.
    pub fn append(
        &mut self,
        session_id: i64,
        ts_ms: i64,
        text: &str,
        raw: Option<&[u8]>,
    ) -> Result<i64> {
        // `prepare_cached` keeps the parsed statement in the connection's
        // statement cache, so repeated appends skip the SQL parse.
        self.conn
            .prepare_cached(INSERT_LINE)?
            .execute(params![session_id, ts_ms, text, raw])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Append many lines in a single transaction with one cached
    /// statement. The session loop collects a socket read's worth of
    /// lines and flushes them here, turning N per-line transactions +
    /// N lock acquisitions into one of each. Empty input is a no-op.
    pub fn append_batch(&mut self, entries: &[LogEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(INSERT_LINE)?;
            for e in entries {
                stmt.execute(params![e.session_id, e.ts_ms, e.text, e.raw])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Append a line by raw ANSI-bearing bytes; the plain-text form is
    /// derived. Test only, since the session hands rows to
    /// [`Self::append_batch`] with the plain text it already holds. The
    /// app's tests write rows through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn append_raw(&mut self, session_id: i64, ts_ms: i64, raw: &[u8]) -> Result<i64> {
        let text = plain_text(raw);
        self.append(session_id, ts_ms, &text, Some(raw))
    }

    /// List the logs in `scope` newest first, capped at `limit` rows. A
    /// zero limit returns them all.
    pub fn list_sessions(&self, limit: usize, scope: &Scope) -> Result<Vec<SessionRow>> {
        let (filter, values) = scope.session_filter(1);
        let cap = if limit == 0 {
            String::new()
        } else {
            format!("LIMIT {limit}")
        };
        let sql = format!(
            "{SESSION_COLUMNS}
             FROM sessions s
             {filter}
             ORDER BY s.started_at_ms DESC
             {cap}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(values), session_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Export a session's log as a single string, see
    /// [`Self::export_scope`].
    pub fn export_session(&self, session_id: i64, with_ansi: bool) -> Result<String> {
        let mut out = Vec::new();
        self.export_scope(&Scope::log(session_id), with_ansi, &mut out)?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// Write every line in `scope` to `out`, oldest first, each ended by
    /// `\n`. With `with_ansi` a line goes out as the bytes the game sent,
    /// colors included, or as its plain text when it kept none, such as a
    /// line you sent. Without it every line is its plain text. Returns
    /// how many lines it wrote.
    pub fn export_scope(
        &self,
        scope: &Scope,
        with_ansi: bool,
        out: &mut dyn std::io::Write,
    ) -> Result<u64> {
        let logs = self.scoped_logs(scope)?;
        let (Some(low), Some(high)) = (
            logs.values().map(|l| l.first).min(),
            logs.values().map(|l| l.last).max(),
        ) else {
            return Ok(0);
        };
        let since = scope.since_ms.unwrap_or(i64::MIN);
        let mut stmt = self.conn.prepare(
            "SELECT session_id, ts_ms, text, raw FROM log_lines
             WHERE id >= ?1 AND id <= ?2 ORDER BY id",
        )?;
        let mut rows = stmt.query(params![low, high])?;
        let mut written = 0u64;
        while let Some(row) = rows.next()? {
            let session_id: i64 = row.get(0)?;
            let ts_ms: i64 = row.get(1)?;
            if !logs.contains_key(&session_id) || ts_ms < since {
                continue;
            }
            let raw = if with_ansi {
                row.get::<_, Option<Vec<u8>>>(3)?
            } else {
                None
            };
            match raw {
                Some(bytes) => out.write_all(&bytes)?,
                None => out.write_all(row.get_ref(2)?.as_bytes().unwrap_or_default())?,
            }
            out.write_all(b"\n")?;
            written += 1;
        }
        Ok(written)
    }

    /// Look up a single session row. Test only. The app's tests read it
    /// through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn get_session(&self, session_id: i64) -> Result<Option<SessionRow>> {
        let row = self
            .conn
            .query_row(
                &format!("{SESSION_COLUMNS} FROM sessions s WHERE s.id = ?1"),
                params![session_id],
                session_row,
            )
            .optional()?;
        Ok(row)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::SearchOptions;

    pub(crate) fn store() -> LogStore {
        LogStore::in_memory().unwrap()
    }

    #[test]
    fn round_trips_a_session() {
        let mut s = store();
        let id = s.start_session("host", 1234, 100).unwrap();
        s.append(id, 110, "hello world", None).unwrap();
        s.append(id, 120, "second line", None).unwrap();
        s.end_session(id, 200).unwrap();

        let row = s.get_session(id).unwrap().unwrap();
        assert_eq!(row.host, "host");
        assert_eq!(row.port, 1234);
        assert_eq!(row.line_count, 2);
        assert_eq!(row.ended_at_ms, Some(200));
    }

    #[test]
    fn append_batch_persists_all_rows_in_one_transaction() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append_batch(&[
            LogEntry {
                session_id: id,
                ts_ms: 1,
                text: "alpha".into(),
                raw: None,
            },
            LogEntry {
                session_id: id,
                ts_ms: 2,
                text: "beta".into(),
                raw: Some(b"\x1b[31mbeta".to_vec()),
            },
            LogEntry {
                session_id: id,
                ts_ms: 3,
                text: "gamma".into(),
                raw: None,
            },
        ])
        .unwrap();
        assert_eq!(s.get_session(id).unwrap().unwrap().line_count, 3);
        let hits = s.search("beta", &SearchOptions::default()).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].raw.as_deref(), Some(&b"\x1b[31mbeta"[..]));
    }

    #[test]
    fn append_batch_empty_is_noop() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append_batch(&[]).unwrap();
        assert_eq!(s.get_session(id).unwrap().unwrap().line_count, 0);
    }

    #[test]
    fn list_sessions_newest_first() {
        let mut s = store();
        let a = s.start_session("a", 1, 100).unwrap();
        let b = s.start_session("b", 2, 200).unwrap();
        let c = s.start_session("c", 3, 150).unwrap();
        let rows = s.list_sessions(0, &Scope::default()).unwrap();
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![b, c, a]);
    }

    #[test]
    fn append_raw_strips_ansi_for_text_column() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append_raw(id, 1, b"\x1b[31mred text\x1b[0m").unwrap();
        let hits = s.search("red text", &SearchOptions::default()).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, "red text");
    }

    #[test]
    fn export_with_and_without_ansi() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append_raw(id, 1, b"\x1b[31mred\x1b[0m").unwrap();
        s.append(id, 2, "plain", None).unwrap();
        let plain = s.export_session(id, false).unwrap();
        assert_eq!(plain, "red\nplain\n");
        let ansi = s.export_session(id, true).unwrap();
        assert_eq!(ansi, "\x1b[31mred\x1b[0m\nplain\n");
    }

    #[test]
    fn export_a_span_of_time_across_logs() {
        let mut s = store();
        let a = s.start_session("h", 1, 0).unwrap();
        s.append(a, 100, "old", None).unwrap();
        s.end_session(a, 150).unwrap();
        let b = s.start_session("h", 1, 900).unwrap();
        let c = s.start_session("other", 1, 900).unwrap();
        s.append_raw(b, 1_000, b"\x1b[33mOrla waves.\x1b[0m")
            .unwrap();
        s.append(c, 1_001, "elsewhere", None).unwrap();
        s.append(b, 1_002, "> wave", None).unwrap();
        let scope = Scope {
            world: Some(("h".into(), 1)),
            since_ms: Some(500),
            ..Scope::default()
        };
        let mut plain = Vec::new();
        assert_eq!(s.export_scope(&scope, false, &mut plain).unwrap(), 2);
        assert_eq!(plain, b"Orla waves.\n> wave\n");
        let mut ansi = Vec::new();
        s.export_scope(&scope, true, &mut ansi).unwrap();
        assert_eq!(ansi, b"\x1b[33mOrla waves.\x1b[0m\n> wave\n");
        let mut none = Vec::new();
        assert_eq!(
            s.export_scope(&Scope::log(99), false, &mut none).unwrap(),
            0
        );
        assert_eq!(none, Vec::<u8>::new());
    }

    #[test]
    fn a_hidden_send_of_several_lines_keeps_one_row_each_and_no_text() {
        // Made up. It is nobody's password.
        const SECRET: &str = "Tr0ub4dor&3";
        let rows = sent_rows(format!("{SECRET}\r\n\r\n{SECRET}\r\n").as_bytes(), true);
        assert_eq!(rows, vec!["> (hidden)", "> (hidden)"]);
        assert_eq!(sent_rows(b"\r\n", true), Vec::<String>::new());
    }

    #[test]
    fn snoop_rows_mark_each_line_with_the_name_and_keep_its_color() {
        let rows = snoop_rows(
            "Tolliver",
            "\x1b[0;33mA Ramshackle Tent City\x1b[0;0m\n\r\n\r[Exits: east west]\n",
        );
        let text: Vec<&str> = rows.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(
            text,
            [
                "Tolliver| A Ramshackle Tent City",
                "Tolliver| ",
                "Tolliver| [Exits: east west]"
            ]
        );
        assert_eq!(
            rows[0].1,
            b"Tolliver| \x1b[0;33mA Ramshackle Tent City\x1b[0;0m".to_vec()
        );
    }

    #[test]
    fn snoop_rows_keep_a_partial_and_drop_the_line_end_debris() {
        let rows = snoop_rows("Maren", "\r<612hp 480m 702mv> ");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "Maren| <612hp 480m 702mv> ");
        assert_eq!(snoop_rows("Maren", "\r"), Vec::new());
        assert_eq!(snoop_rows("Maren", ""), Vec::new());
    }

    #[test]
    fn snoop_rows_find_by_the_mark_and_stay_out_of_other_searches() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append(id, 1, "Orla says, 'East.'", None).unwrap();
        let rows: Vec<LogEntry> = snoop_rows("Tolliver", "You go east.\n\r")
            .into_iter()
            .map(|(text, raw)| LogEntry {
                session_id: id,
                ts_ms: 2,
                text,
                raw: Some(raw),
            })
            .collect();
        s.append_batch(&rows).unwrap();
        // The search reads a regex, so the bar takes a backslash.
        let marked = s.search(r"^Tolliver\|", &SearchOptions::default()).unwrap();
        assert_eq!(marked.len(), 1);
        assert_eq!(marked[0].text, "Tolliver| You go east.");
        let others = s.search("^Orla", &SearchOptions::default()).unwrap();
        assert_eq!(others.len(), 1);
    }

    /// Five sessions: two to a MUD, one each to 127.0.0.1, localhost,
    /// and `LocalHost.` Each line reads `<host> line <n>`.
    pub(crate) fn store_with_local_sessions() -> (LogStore, i64, i64) {
        let mut s = store();
        let mud = s
            .start_session("play.theforsakenlands.com", 1848, 100)
            .unwrap();
        let loopback = s.start_session("127.0.0.1", 4000, 200).unwrap();
        let named = s.start_session("localhost", 4000, 300).unwrap();
        let shouted = s.start_session(" LocalHost. ", 4000, 400).unwrap();
        let later = s
            .start_session("play.theforsakenlands.com", 1848, 500)
            .unwrap();
        for (sid, host) in [
            (mud, "mud"),
            (loopback, "loopback"),
            (named, "named"),
            (shouted, "shouted"),
            (later, "later"),
        ] {
            for n in 0..3 {
                s.append(sid, n, &format!("{host} line {n}"), None).unwrap();
            }
        }
        (s, mud, later)
    }

    #[test]
    fn local_hosts_are_this_machine() {
        assert!(is_local_host("127.0.0.1"));
        assert!(is_local_host("localhost"));
        assert!(is_local_host(" LocalHost. "));
        assert!(!is_local_host("play.theforsakenlands.com"));
        assert!(!is_local_host("localhost.example.org"));
        assert!(!is_local_host("127.0.0.2"));
    }

    #[test]
    fn list_sessions_can_hide_local_sessions() {
        let (s, mud, later) = store_with_local_sessions();
        assert_eq!(s.list_sessions(0, &Scope::default()).unwrap().len(), 5);
        let hidden = Scope {
            hide_local: true,
            ..Scope::default()
        };
        let rows = s.list_sessions(0, &hidden).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![later, mud]
        );
        assert!(rows.iter().all(|r| r.line_count == 3));
        let capped = s.list_sessions(1, &hidden).unwrap();
        assert_eq!(capped.iter().map(|r| r.id).collect::<Vec<_>>(), vec![later]);
    }

    #[test]
    fn a_session_is_named_once() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        assert_eq!(s.session_character(id).unwrap(), None);
        s.set_session_character(id, "Tester").unwrap();
        assert_eq!(s.session_character(id).unwrap().as_deref(), Some("Tester"));
        // A later name leaves the first one.
        s.set_session_character(id, "Other").unwrap();
        assert_eq!(s.session_character(id).unwrap().as_deref(), Some("Tester"));
        assert_eq!(
            s.session_character(id + 1).unwrap(),
            None,
            "no such session"
        );
    }
}
