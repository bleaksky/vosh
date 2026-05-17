//! SQLite-backed log store for Vosh sessions.
//!
//! Each connection opens a `sessions` row, every server line lands in
//! `log_lines` with both its plain-text form (ANSI stripped) and the
//! original ANSI-bearing bytes, and search runs as a regex scan over
//! the `text` column. Export reproduces a session as plain text or with
//! ANSI codes restored.

use std::path::Path;

use vosh_ansi::plain_text;
use regex::{Regex, RegexBuilder};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LogError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("regex: {0}")]
    Regex(#[from] regex::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, LogError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub id: i64,
    pub host: String,
    pub port: u16,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub line_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub session_id: i64,
    pub host: String,
    pub port: u16,
    pub line_id: i64,
    pub ts_ms: i64,
    pub text: String,
    /// Original ANSI-bearing bytes for the line, when stored. Lets the
    /// frontend render the hit with its original colors instead of the
    /// ANSI-stripped plain text.
    pub raw: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    /// Cap the number of hits returned. Zero means "no cap".
    pub max_results: usize,
    /// Optional `session_id` filter; None searches every session.
    pub session_id: Option<i64>,
}

pub struct LogStore {
    conn: Connection,
}

impl LogStore {
    /// Open or create a log database at `path`. The parent directory must
    /// already exist.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Open an in-memory database. Used by tests.
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 host TEXT NOT NULL,
                 port INTEGER NOT NULL,
                 started_at_ms INTEGER NOT NULL,
                 ended_at_ms INTEGER
             );
             CREATE TABLE IF NOT EXISTS log_lines (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id INTEGER NOT NULL REFERENCES sessions(id),
                 ts_ms INTEGER NOT NULL,
                 text TEXT NOT NULL,
                 raw BLOB
             );
             CREATE INDEX IF NOT EXISTS idx_log_lines_session
                 ON log_lines(session_id, ts_ms);",
        )?;
        Ok(())
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
        self.conn.execute(
            "INSERT INTO log_lines (session_id, ts_ms, text, raw) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, ts_ms, text, raw],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Append a line by raw ANSI-bearing bytes; the plain-text form is
    /// derived. Convenience wrapper for the session loop hot path.
    pub fn append_raw(&mut self, session_id: i64, ts_ms: i64, raw: &[u8]) -> Result<i64> {
        let text = plain_text(raw);
        self.append(session_id, ts_ms, &text, Some(raw))
    }

    /// List sessions newest first, capped at `limit` rows. A zero limit
    /// returns all sessions.
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionRow>> {
        let sql = if limit == 0 {
            "SELECT s.id, s.host, s.port, s.started_at_ms, s.ended_at_ms,
                    (SELECT COUNT(*) FROM log_lines l WHERE l.session_id = s.id)
             FROM sessions s
             ORDER BY s.started_at_ms DESC"
                .to_string()
        } else {
            format!(
                "SELECT s.id, s.host, s.port, s.started_at_ms, s.ended_at_ms,
                        (SELECT COUNT(*) FROM log_lines l WHERE l.session_id = s.id)
                 FROM sessions s
                 ORDER BY s.started_at_ms DESC
                 LIMIT {limit}"
            )
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SessionRow {
                    id: row.get(0)?,
                    host: row.get(1)?,
                    port: row.get::<_, i64>(2)? as u16,
                    started_at_ms: row.get(3)?,
                    ended_at_ms: row.get(4)?,
                    line_count: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Run a regex search over log lines. Returns matches newest first.
    pub fn search(&self, pattern: &str, options: &SearchOptions) -> Result<Vec<SearchHit>> {
        let regex: Regex = RegexBuilder::new(pattern)
            .case_insensitive(!options.case_sensitive)
            .build()?;

        let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) =
            if let Some(sid) = options.session_id {
                (
                    "SELECT l.id, l.session_id, l.ts_ms, l.text, l.raw, s.host, s.port
                     FROM log_lines l
                     JOIN sessions s ON s.id = l.session_id
                     WHERE l.session_id = ?1
                     ORDER BY l.id DESC",
                    vec![Box::new(sid)],
                )
            } else {
                (
                    "SELECT l.id, l.session_id, l.ts_ms, l.text, l.raw, s.host, s.port
                     FROM log_lines l
                     JOIN sessions s ON s.id = l.session_id
                     ORDER BY l.id DESC",
                    vec![],
                )
            };

        let mut stmt = self.conn.prepare(sql)?;
        let cap = if options.max_results == 0 {
            usize::MAX
        } else {
            options.max_results
        };
        let mut hits = Vec::new();
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(AsRef::as_ref).collect();
        let mut rows = stmt.query(rusqlite::params_from_iter(param_refs))?;
        while let Some(row) = rows.next()? {
            if hits.len() >= cap {
                break;
            }
            let text: String = row.get(3)?;
            if regex.is_match(&text) {
                hits.push(SearchHit {
                    line_id: row.get(0)?,
                    session_id: row.get(1)?,
                    ts_ms: row.get(2)?,
                    text,
                    raw: row.get(4)?,
                    host: row.get(5)?,
                    port: row.get::<_, i64>(6)? as u16,
                });
            }
        }
        Ok(hits)
    }

    /// Export a session's log as a single string. With `with_ansi=true`
    /// the original raw bytes are concatenated (best-effort UTF-8); with
    /// `with_ansi=false` only the plain-text column is used.
    pub fn export_session(&self, session_id: i64, with_ansi: bool) -> Result<String> {
        let mut stmt = self.conn.prepare(
            "SELECT ts_ms, text, raw FROM log_lines
             WHERE session_id = ?1 ORDER BY id ASC",
        )?;
        let mut rows = stmt.query(params![session_id])?;
        let mut out = String::new();
        while let Some(row) = rows.next()? {
            let text: String = row.get(1)?;
            if with_ansi {
                let raw: Option<Vec<u8>> = row.get(2)?;
                match raw {
                    Some(bytes) => out.push_str(&String::from_utf8_lossy(&bytes)),
                    None => out.push_str(&text),
                }
            } else {
                out.push_str(&text);
            }
            out.push('\n');
        }
        Ok(out)
    }

    /// Look up a single session row.
    pub fn get_session(&self, session_id: i64) -> Result<Option<SessionRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT s.id, s.host, s.port, s.started_at_ms, s.ended_at_ms,
                        (SELECT COUNT(*) FROM log_lines l WHERE l.session_id = s.id)
                 FROM sessions s WHERE s.id = ?1",
                params![session_id],
                |row| {
                    Ok(SessionRow {
                        id: row.get(0)?,
                        host: row.get(1)?,
                        port: row.get::<_, i64>(2)? as u16,
                        started_at_ms: row.get(3)?,
                        ended_at_ms: row.get(4)?,
                        line_count: row.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> LogStore {
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
    fn list_sessions_newest_first() {
        let mut s = store();
        let a = s.start_session("a", 1, 100).unwrap();
        let b = s.start_session("b", 2, 200).unwrap();
        let c = s.start_session("c", 3, 150).unwrap();
        let rows = s.list_sessions(0).unwrap();
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![b, c, a]);
    }

    #[test]
    fn search_matches_lines() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append(id, 1, "the dragon roars", None).unwrap();
        s.append(id, 2, "you draw a sword", None).unwrap();
        s.append(id, 3, "the dragon dies", None).unwrap();
        let hits = s
            .search("dragon", &SearchOptions::default())
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.text.contains("dragon")));
    }

    #[test]
    fn search_respects_case_and_max() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append(id, 1, "Foo", None).unwrap();
        s.append(id, 2, "FOO", None).unwrap();
        s.append(id, 3, "foo", None).unwrap();
        let opts = SearchOptions {
            case_sensitive: true,
            max_results: 0,
            session_id: None,
        };
        let hits = s.search("foo", &opts).unwrap();
        assert_eq!(hits.len(), 1);

        let opts = SearchOptions {
            case_sensitive: false,
            max_results: 2,
            session_id: None,
        };
        let hits = s.search("foo", &opts).unwrap();
        assert_eq!(hits.len(), 2);
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
    fn search_can_filter_by_session() {
        let mut s = store();
        let a = s.start_session("a", 1, 0).unwrap();
        let b = s.start_session("b", 2, 0).unwrap();
        s.append(a, 1, "foo from a", None).unwrap();
        s.append(b, 2, "foo from b", None).unwrap();
        let opts = SearchOptions {
            case_sensitive: false,
            max_results: 0,
            session_id: Some(a),
        };
        let hits = s.search("foo", &opts).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("from a"));
    }
}
