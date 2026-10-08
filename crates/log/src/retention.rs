//! Keep logs for. Finds the logs that ended before a cutoff, deletes each
//! whole, and gives the file's space back a little at a time (D34).
//!
//! `SQLite` gives free pages back in steps only once the file keeps
//! `auto_vacuum = INCREMENTAL`. A new file starts that way (see
//! `sqlite.rs`), and a file an older build wrote needs one full rebuild,
//! [`LogStore::turn_on_compaction`], before its first step.

use rusqlite::params;

use crate::{LogStore, Result};

impl LogStore {
    /// The logs that ended before `before_ms`, oldest first. A log still
    /// open is never one of them.
    pub fn logs_ended_before(&self, before_ms: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM sessions WHERE ended_at_ms IS NOT NULL AND ended_at_ms < ?1
             ORDER BY id",
        )?;
        let ids = stmt
            .query_map(params![before_ms], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(ids)
    }

    /// Delete log `id` and every line it holds, in one transaction.
    pub fn delete_log(&mut self, id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM log_lines WHERE session_id = ?1", params![id])?;
        tx.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    /// True when the file gives free pages back in steps.
    pub fn compacts_in_steps(&self) -> Result<bool> {
        let mode: i64 = self
            .conn
            .query_row("PRAGMA auto_vacuum", [], |r| r.get(0))?;
        Ok(mode == 2)
    }

    /// Rebuild the file so it gives free pages back in steps from now
    /// on. It takes time in proportion to the file and needs free disk
    /// space about its size, so the app runs it once, off the input
    /// path.
    pub fn turn_on_compaction(&mut self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA auto_vacuum = INCREMENTAL; VACUUM;")?;
        Ok(())
    }

    /// Give back up to `pages` free pages. True while free pages remain.
    pub fn compact_step(&mut self, pages: u32) -> Result<bool> {
        self.conn
            .execute_batch(&format!("PRAGMA incremental_vacuum({pages});"))?;
        let left: i64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        Ok(left > 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::sessions::tests::store;
    use crate::{LogStore, Scope};

    #[test]
    fn only_logs_that_ended_before_the_cutoff_go() {
        let mut s = store();
        let old = s.start_session("h", 1, 0).unwrap();
        s.append(old, 10, "Orla waves.", None).unwrap();
        s.end_session(old, 100).unwrap();
        let recent = s.start_session("h", 1, 200).unwrap();
        s.end_session(recent, 300).unwrap();
        let open = s.start_session("h", 1, 50).unwrap();
        assert_eq!(s.logs_ended_before(250).unwrap(), vec![old]);
        s.delete_log(old).unwrap();
        let left: Vec<i64> = s
            .list_sessions(0, &Scope::default())
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(left, vec![recent, open]);
        assert_eq!(
            s.search("Orla", &crate::SearchOptions::default())
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn an_older_file_compacts_in_steps_once_turned_on() {
        let dir = std::env::temp_dir().join(format!(
            "vosh-log-compact-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logs.sqlite");
        {
            // A file an older build made, with no auto_vacuum.
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE t (x INTEGER);").unwrap();
        }
        let mut s = LogStore::open(&path).unwrap();
        assert!(!s.compacts_in_steps().unwrap());
        s.turn_on_compaction().unwrap();
        assert!(s.compacts_in_steps().unwrap());
        let id = s.start_session("h", 1, 0).unwrap();
        let rows: Vec<crate::LogEntry> = (0..5_000)
            .map(|n| crate::LogEntry {
                session_id: id,
                ts_ms: n,
                text: format!("Maren says, 'line {n} of a long and wordy evening.'"),
                raw: None,
                kind: crate::LineKind::Text,
            })
            .collect();
        s.append_batch(&rows).unwrap();
        s.end_session(id, 5_000).unwrap();
        s.delete_log(id).unwrap();
        s.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
        let mut steps = 0;
        while s.compact_step(16).unwrap() {
            steps += 1;
        }
        assert!(steps > 1, "{steps}");
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_file_compacts_in_steps_from_the_start() {
        let dir = std::env::temp_dir().join(format!(
            "vosh-log-new-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let s = LogStore::open(&dir.join("logs.sqlite")).unwrap();
        assert!(s.compacts_in_steps().unwrap());
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
