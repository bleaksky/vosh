//! Opens logs.sqlite, sets the WAL pragmas, and creates or upgrades the
//! tables.

use std::path::Path;

use rusqlite::{params, Connection};

use crate::{LogStore, Result};

/// Apply `SQLite` pragmas that turn the per-INSERT fsync storm into
/// a per-checkpoint cost. The session `io_loop` appends one row per
/// server line; with the default `journal_mode=DELETE` +
/// `synchronous=FULL`, each append blocks on two fsyncs (data +
/// journal unlink), which dominates the per-line budget under heavy
/// throughput.
///
/// WAL mode amortises commits across a single rolling write-ahead log
/// (checkpointed in the background by `SQLite`). `synchronous=NORMAL`
/// is the recommended pairing — durable across application crashes,
/// only at risk of losing the last ~few seconds of writes on a
/// host-level power loss / kernel panic. For a scrollback log that
/// trade is the correct one.
///
/// `wal_autocheckpoint = 100` shrinks each automatic checkpoint to
/// roughly 100 pages (~400 KB) of WAL frames instead of the default
/// 1000. Live capture showed periodic 200-340 µs append spikes on
/// a populated database — those were the checkpoint thread folding
/// a full default-sized WAL back into the main file under the
/// append's lock. Smaller, more frequent checkpoints trade a few
/// extra micros of background work for far flatter per-append
/// latency, which is what the `io_loop` budget actually cares about.
///
/// Side effect on disk: WAL produces `<db>-wal` and `<db>-shm` sidecar
/// files next to the main `.db`. They are managed transparently by
/// `SQLite` and removed at clean shutdown. Existing databases open in
/// WAL mode without any migration.
///
/// In-memory databases silently report `memory` for `journal_mode`
/// instead of accepting WAL; the call still succeeds.
fn configure_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA wal_autocheckpoint = 100;",
    )?;
    Ok(())
}

impl LogStore {
    /// Open or create a log database at `path`. The parent directory must
    /// already exist.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        // A new file gives free pages back in steps from the start, so
        // Keep logs for never needs to rebuild it (see `retention.rs`).
        // The setting takes only before the first table.
        let tables: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0))?;
        if tables == 0 {
            conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")?;
        }
        configure_connection(&conn)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Open an in-memory database. Test only. The app's tests open one
    /// through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        configure_connection(&conn)?;
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
        // The character a session belongs to came later. A log an
        // older build wrote gains the column here, and its sessions
        // keep no character.
        if !self.has_column("sessions", "character")? {
            self.conn
                .execute_batch("ALTER TABLE sessions ADD COLUMN character TEXT")?;
        }
        Ok(())
    }

    /// True when `table` has a column named `column`.
    fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")?;
        Ok(stmt.exists(params![table, column])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::tests::store;
    use crate::SearchOptions;

    #[test]
    fn low_fsync_pragmas_are_applied_on_open() {
        // Regression guard for the Phase 2 perf fix: if the pragmas
        // ever get dropped, per-line fsync pressure returns and every
        // server line stalls behind a flush. The synchronous pragma
        // works on every backend so we assert it directly; journal
        // mode silently reports "memory" for in-memory databases, so
        // we accept either "memory" or "wal" rather than depending on
        // a file-backed test fixture.
        let s = store();
        let sync: i64 = s
            .conn
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        // SQLite encodes the pragma as 0=OFF, 1=NORMAL, 2=FULL, 3=EXTRA.
        assert_eq!(sync, 1, "synchronous should be NORMAL, got {sync}");
        let mode: String = s
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert!(
            matches!(mode.as_str(), "memory" | "wal"),
            "journal_mode should be WAL on disk (or memory in-memory), got {mode}"
        );
    }

    /// A folder of its own under the temp folder, for a test that needs
    /// a log on disk.
    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("vosh-log-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_older_log_gains_the_character_column_and_keeps_its_rows() {
        let dir = temp_dir("older");
        let path = dir.join("logs.sqlite");
        {
            // The schema an older build wrote, with no character column.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE sessions (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     host TEXT NOT NULL,
                     port INTEGER NOT NULL,
                     started_at_ms INTEGER NOT NULL,
                     ended_at_ms INTEGER
                 );
                 CREATE TABLE log_lines (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     session_id INTEGER NOT NULL REFERENCES sessions(id),
                     ts_ms INTEGER NOT NULL,
                     text TEXT NOT NULL,
                     raw BLOB
                 );
                 CREATE INDEX idx_log_lines_session ON log_lines(session_id, ts_ms);
                 INSERT INTO sessions (host, port, started_at_ms, ended_at_ms)
                     VALUES ('h', 1, 10, 20);
                 INSERT INTO log_lines (session_id, ts_ms, text) VALUES (1, 11, 'hello');",
            )
            .unwrap();
        }
        let mut s = LogStore::open(&path).unwrap();
        assert!(s.has_column("sessions", "character").unwrap());
        let old = s.get_session(1).unwrap().expect("the old session");
        assert_eq!((old.host.as_str(), old.line_count), ("h", 1));
        assert_eq!(s.session_character(1).unwrap(), None);
        let id = s.start_session("h", 1, 30).unwrap();
        s.set_session_character(id, "Tester").unwrap();
        drop(s);

        // Opening it again finds the column there and adds nothing.
        let s = LogStore::open(&path).unwrap();
        assert_eq!(s.session_character(id).unwrap().as_deref(), Some("Tester"));
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_connection_reads_while_the_first_writes() {
        // The app searches through a second connection so a long scan
        // never holds up the session loop's appends.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vosh-log-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logs.sqlite");
        let mut writer = LogStore::open(&path).unwrap();
        let reader = LogStore::open(&path).unwrap();
        let id = writer.start_session("h", 1, 0).unwrap();
        writer.append(id, 1, "first line", None).unwrap();

        // Hold a read open on the reader while the writer appends.
        let mut stmt = reader.conn.prepare("SELECT text FROM log_lines").unwrap();
        let mut rows = stmt.query([]).unwrap();
        assert!(rows.next().unwrap().is_some());
        writer.append(id, 2, "second line", None).unwrap();
        drop(rows);
        drop(stmt);

        let page = reader
            .search_page("line", &SearchOptions::default(), true)
            .unwrap();
        assert_eq!(page.total, Some(2));
        drop(reader);
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
