//! The SQL that finds the password lines, blanks them, and rebuilds the
//! file so no old copy is left.

use std::borrow::Cow;
use std::collections::BTreeSet;

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, Row, TransactionBehavior};

use super::replay::PasswordFinder;
use super::{Forgotten, PasswordLines};
use crate::{LogStore, Result, HIDDEN_SENT_TEXT};

/// A table that says an earlier run blanked lines and has not yet cleared
/// the old copies of their text from the file. The blanking transaction
/// creates it, so it outlasts a quit, a full disk, or a busy checkpoint
/// between the update and the rebuild. Only a finished rebuild drops it,
/// and until then every blanking run rebuilds the file again.
const WIPE_PENDING_TABLE: &str = "forget_passwords_wipe_pending";

impl LogStore {
    /// Find the sent lines that still hold a password. Reads only, one
    /// pass over the log in id order.
    pub fn find_password_lines(&self) -> Result<PasswordLines> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {FINDER_COLUMNS} FROM log_lines ORDER BY id"
        ))?;
        let mut rows = stmt.query([])?;
        let mut finder = PasswordFinder::new();
        while let Some(row) = rows.next()? {
            feed(&mut finder, row)?;
        }
        drop(rows);
        let mut found = finder.finish();
        found.wipe_pending = self.wipe_pending()?;
        Ok(found)
    }

    /// Replace the text of each line in `found` with [`HIDDEN_SENT_TEXT`]
    /// and wipe every old copy of it from the database file. Only sent
    /// lines change, and a line already blanked is not counted again.
    ///
    /// `secure_delete` zeroes the old bytes in the pages the update
    /// rewrites. A checkpoint moves those pages into the main file and
    /// truncates the write ahead log, which still holds the old pages.
    /// Then `VACUUM` rebuilds the file, because a page split from before
    /// this run can leave a stale copy of a row in free space no update
    /// reaches (the table's first page keeps its old rows when it
    /// splits), and a last checkpoint truncates the log again. The
    /// rebuild takes time in proportion to the log, so the app runs this
    /// off the input path.
    ///
    /// When the rebuild cannot finish, the lines stay blanked and a
    /// marker table stays behind. The next run then rebuilds the file
    /// even with no new line to blank, and reports it as `resumed`.
    pub fn blank_password_lines(&mut self, found: &PasswordLines) -> Result<Forgotten> {
        let before: i64 = self
            .conn
            .query_row("PRAGMA secure_delete", [], |r| r.get(0))?;
        self.conn.execute_batch("PRAGMA secure_delete = ON;")?;
        let outcome = self.blank_lines(found);
        let restore = match before {
            0 => "OFF",
            2 => "FAST",
            _ => "ON",
        };
        let restored = self
            .conn
            .execute_batch(&format!("PRAGMA secure_delete = {restore};"));
        let outcome = outcome?;
        restored?;
        Ok(outcome)
    }

    fn blank_lines(&mut self, found: &PasswordLines) -> Result<Forgotten> {
        let resumed = self.wipe_pending()?;
        let (lines, sessions) = self.blank_rows(found)?;
        let wiped = (lines == 0 && !resumed) || wipe(&self.conn);
        Ok(Forgotten {
            lines,
            sessions,
            resumed,
            wiped,
        })
    }

    /// The update itself, in one transaction that also leaves the marker
    /// of a wipe due. Returns how many lines it blanked and in how many
    /// sessions.
    pub(super) fn blank_rows(&mut self, found: &PasswordLines) -> Result<(usize, usize)> {
        let mut lines = 0;
        let mut sessions = BTreeSet::new();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut stmt = tx.prepare(
                "UPDATE log_lines SET text = ?1
                 WHERE id = ?2 AND raw IS NULL AND substr(text, 1, 2) = '> ' AND text <> ?1",
            )?;
            for (id, session_id) in &found.lines {
                if stmt.execute(params![HIDDEN_SENT_TEXT, id])? > 0 {
                    lines += 1;
                    sessions.insert(*session_id);
                }
            }
        }
        if lines > 0 {
            tx.execute_batch(&format!(
                "CREATE TABLE IF NOT EXISTS {WIPE_PENDING_TABLE} (id INTEGER PRIMARY KEY);"
            ))?;
        }
        tx.commit()?;
        Ok((lines, sessions.len()))
    }

    /// True when an earlier run blanked lines and could not clear the
    /// old copies of their text from the file.
    pub(super) fn wipe_pending(&self) -> Result<bool> {
        let pending = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [WIPE_PENDING_TABLE],
            |r| r.get(0),
        )?;
        Ok(pending)
    }

    /// [`Self::find_password_lines`], then [`Self::blank_password_lines`].
    /// Test only, since the app finds the lines through the search
    /// connection and blanks them through the writer, in two steps.
    #[cfg(test)]
    pub fn forget_passwords(&mut self) -> Result<Forgotten> {
        let found = self.find_password_lines()?;
        self.blank_password_lines(&found)
    }
}

/// The columns [`feed`] reads, in its order.
const FINDER_COLUMNS: &str = "id, session_id, ts_ms, text, raw IS NULL";

/// Hand one row, read with [`FINDER_COLUMNS`], to `finder` as a line
/// you sent or a line of output.
fn feed(finder: &mut PasswordFinder, row: &Row<'_>) -> rusqlite::Result<()> {
    let id: i64 = row.get(0)?;
    let session_id: i64 = row.get(1)?;
    let ts_ms: i64 = row.get(2)?;
    let no_raw: bool = row.get(4)?;
    // Read the bytes as they are, so a line that is not valid UTF-8
    // never stops the pass.
    let text = match row.get_ref(3)? {
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => String::from_utf8_lossy(bytes),
        _ => Cow::Borrowed(""),
    };
    let sent = no_raw && text.starts_with("> ");
    finder.row(id, session_id, ts_ms, &text, sent);
    Ok(())
}

/// Push every page into the main file and truncate the write ahead log.
/// False when another connection still reads an older snapshot.
fn checkpoint(conn: &Connection) -> bool {
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
        r.get::<_, i64>(0)
    })
    .is_ok_and(|busy| busy == 0)
}

/// Clear old copies of changed rows out of the file, then drop the
/// marker of a wipe due. True when no old copy is left.
fn wipe(conn: &Connection) -> bool {
    // The first checkpoint lands the zeroed pages even if the rebuild
    // below cannot run, for lack of disk space say.
    checkpoint(conn);
    let rebuilt = conn.execute_batch("VACUUM;").is_ok();
    // The rebuild writes the new file through the write ahead log, and
    // this checkpoint moves it in and truncates the log to nothing.
    if !(rebuilt && checkpoint(conn)) {
        return false;
    }
    // The file is clean now. A marker that fails to drop only means the
    // next run rebuilds the file once more.
    if conn
        .execute_batch(&format!("DROP TABLE IF EXISTS {WIPE_PENDING_TABLE};"))
        .is_ok()
    {
        checkpoint(conn);
    }
    true
}
