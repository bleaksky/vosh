//! Finding and blanking the lines in a log file, and the rebuild that
//! clears their old copies.

use rusqlite::Connection;

use super::*;
use crate::{LogStore, HIDDEN_SENT_TEXT};

// ---- the store ----

/// A log file in a fresh temp folder, removed on drop.
pub(super) struct TempLog {
    dir: std::path::PathBuf,
}

impl TempLog {
    pub(super) fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("vosh-forget-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn path(&self) -> std::path::PathBuf {
        self.dir.join("logs.sqlite")
    }

    /// Every byte of the database file and its sidecars.
    fn bytes(&self) -> Vec<u8> {
        let mut all = Vec::new();
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut name = self.path().into_os_string();
            name.push(suffix);
            if let Ok(b) = std::fs::read(std::path::PathBuf::from(name)) {
                all.extend_from_slice(&b);
            }
        }
        all
    }

    fn holds(&self, secret: &str) -> bool {
        let hay = self.bytes();
        hay.windows(secret.len()).any(|w| w == secret.as_bytes())
    }
}

impl Drop for TempLog {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Log `rows` as one session the way the app does: game lines with
/// their raw bytes, sent lines as `> ` rows with none.
pub(super) fn log_session(store: &mut LogStore, rows: &[Row<'_>], filler: usize) -> i64 {
    let sid = store
        .start_session("play.theforsakenlands.com", 1848, 0)
        .unwrap();
    let mut sent_ts = 0;
    for (n, row) in rows.iter().enumerate() {
        let ts = n as i64;
        match row {
            Out(text) => {
                store
                    .append_raw(sid, ts, format!("\x1b[0m{text}").as_bytes())
                    .unwrap();
            }
            Sent(line) => {
                sent_ts = ts;
                store
                    .append(sid, sent_ts, &format!("> {line}"), None)
                    .unwrap();
            }
            Also(line) => {
                store
                    .append(sid, sent_ts, &format!("> {line}"), None)
                    .unwrap();
            }
        }
    }
    // Enough play after the login that the table spans many pages.
    let entries: Vec<crate::LogEntry> = (0..filler)
        .map(|n| crate::LogEntry {
            session_id: sid,
            ts_ms: 10_000 + n as i64,
            text: format!("The Temple Square hums with voices, line {n}."),
            raw: Some(
                format!("\x1b[1;37mThe Temple Square hums with voices, line {n}.\x1b[0m")
                    .into_bytes(),
            ),
        })
        .collect();
    store.append_batch(&entries).unwrap();
    sid
}

/// One row: id, session id, time, text, raw bytes.
type RowSnapshot = (i64, i64, i64, String, Option<Vec<u8>>);

/// Every row, for comparing a log before and after.
fn snapshot(store: &LogStore) -> Vec<RowSnapshot> {
    let mut stmt = store
        .conn
        .prepare("SELECT id, session_id, ts_ms, text, raw FROM log_lines ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
    })
    .unwrap()
    .collect::<std::result::Result<Vec<_>, _>>()
    .unwrap()
}

/// Three sessions. The first logs in with the account password and
/// an immortal password, the second changes the account password,
/// the third only plays.
pub(super) fn populated(log: &TempLog) -> (LogStore, Vec<i64>) {
    let mut store = LogStore::open(&log.path()).unwrap();
    let first = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("2"),
            Out(""),
            Sent(SECRET_IMM),
            Out(MOTD),
            Sent("look"),
        ],
    ]);
    let second = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("p"),
            Out(""),
            Sent(SECRET_OLD),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Out("Account password changed successfully."),
        ],
    ]);
    let third = vec![Out("The Temple Square"), Sent("look"), Sent("north")];
    let a = log_session(&mut store, &first, 1500);
    let b = log_session(&mut store, &second, 1500);
    let c = log_session(&mut store, &third, 200);
    (store, vec![a, b, c])
}

#[test]
fn the_preview_counts_without_changing_anything() {
    let log = TempLog::new("preview");
    let (store, _) = populated(&log);
    let before = snapshot(&store);
    let found = store.find_password_lines().unwrap();
    assert_eq!(found.count(), 6);
    assert_eq!(found.sessions(), 2);
    assert!(snapshot(&store) == before, "the preview changed the log");
}

#[test]
fn forgetting_blanks_exactly_those_lines_and_a_second_run_finds_none() {
    let log = TempLog::new("blank");
    let (mut store, _) = populated(&log);
    let before = snapshot(&store);
    let found = store.find_password_lines().unwrap();
    let ids: Vec<i64> = found.lines.iter().map(|(id, _)| *id).collect();

    let done = store.blank_password_lines(&found).unwrap();
    assert_eq!(done.lines, 6);
    assert_eq!(done.sessions, 2);
    assert!(done.wiped, "the file was not wiped");

    let after = snapshot(&store);
    assert_eq!(before.len(), after.len(), "rows came or went");
    for (old, new) in before.iter().zip(&after) {
        assert_eq!((old.0, old.1, old.2), (new.0, new.1, new.2), "a row moved");
        assert!(old.4 == new.4, "raw bytes changed on row {}", old.0);
        if ids.contains(&old.0) {
            assert!(new.3 == HIDDEN_SENT_TEXT, "row {} was not blanked", old.0);
        } else {
            assert!(
                old.3 == new.3,
                "row {} changed though it holds no password",
                old.0
            );
        }
    }

    assert_eq!(store.find_password_lines().unwrap().count(), 0);
    let again = store.forget_passwords().unwrap();
    assert_eq!(again.lines, 0);
    assert!(snapshot(&store) == after, "a second run changed the log");
}

#[test]
fn no_old_copy_of_a_secret_stays_in_the_file() {
    let log = TempLog::new("wipe");
    let (mut store, _) = populated(&log);
    let secrets = [SECRET_ACCOUNT, SECRET_IMM, SECRET_OLD, SECRET_NEW];
    // The test means something only if the secrets start on disk.
    for (n, secret) in secrets.iter().enumerate() {
        assert!(
            log.holds(secret),
            "made up secret {n} never reached the file"
        );
    }
    let done = store.forget_passwords().unwrap();
    assert!(done.wiped, "the file was not wiped");
    for (n, secret) in secrets.iter().enumerate() {
        assert!(
            !log.holds(secret),
            "made up secret {n} is still in the file"
        );
    }
    // The rest of the log is still on disk.
    assert!(log.holds("line 1499."), "ordinary lines went missing");
}

const SECRETS: [&str; 4] = [SECRET_ACCOUNT, SECRET_IMM, SECRET_OLD, SECRET_NEW];

/// Indexes of the made up secrets still somewhere in the file.
fn secrets_left(log: &TempLog) -> Vec<usize> {
    (0..SECRETS.len())
        .filter(|&n| log.holds(SECRETS[n]))
        .collect()
}

#[test]
fn a_wipe_that_could_not_finish_is_finished_by_the_next_run() {
    let log = TempLog::new("unfinished");
    let (mut store, _) = populated(&log);
    // Another reader holds an old snapshot open, so the last
    // checkpoint cannot empty the write ahead log. A full disk under
    // the rebuild ends the same way.
    let reader = Connection::open(log.path()).unwrap();
    reader.execute_batch("BEGIN;").unwrap();
    let _: i64 = reader
        .query_row("SELECT count(*) FROM log_lines", [], |r| r.get(0))
        .unwrap();
    let done = store.forget_passwords().unwrap();
    assert_eq!((done.lines, done.resumed, done.wiped), (6, false, false));

    // The preview finds no line left to blank and says the wipe is due.
    let found = store.find_password_lines().unwrap();
    assert_eq!(found.count(), 0);
    assert!(found.wipe_pending, "the preview forgot the unfinished wipe");

    reader.execute_batch("COMMIT;").unwrap();
    drop(reader);
    let again = store.forget_passwords().unwrap();
    assert_eq!((again.lines, again.resumed, again.wiped), (0, true, true));
    assert_eq!(
        secrets_left(&log),
        Vec::<usize>::new(),
        "made up secrets are still in the file"
    );

    // Nothing is due after that.
    assert!(!store.find_password_lines().unwrap().wipe_pending);
    let before = snapshot(&store);
    let third = store.forget_passwords().unwrap();
    assert_eq!((third.lines, third.resumed, third.wiped), (0, false, true));
    assert!(
        snapshot(&store) == before,
        "a run with nothing due changed the log"
    );
}

#[test]
fn a_wipe_cut_short_by_a_quit_is_finished_after_a_restart() {
    let log = TempLog::new("cut-short");
    let (mut store, _) = populated(&log);
    let found = store.find_password_lines().unwrap();
    // Vosh quits once the lines are blanked, before the rebuild ends.
    assert_eq!(store.blank_rows(&found).unwrap(), (6, 2));
    drop(store);
    assert!(
        !secrets_left(&log).is_empty(),
        "the test means something only if old copies stay behind"
    );

    let mut store = LogStore::open(&log.path()).unwrap();
    let found = store.find_password_lines().unwrap();
    assert_eq!(found.count(), 0);
    assert!(found.wipe_pending, "the preview forgot the unfinished wipe");
    let done = store.blank_password_lines(&found).unwrap();
    assert_eq!((done.lines, done.resumed, done.wiped), (0, true, true));
    assert_eq!(
        secrets_left(&log),
        Vec::<usize>::new(),
        "made up secrets are still in the file"
    );
    assert!(!store.wipe_pending().unwrap());
}

#[test]
fn the_live_log_keeps_working_after_forgetting() {
    let log = TempLog::new("live");
    let (mut store, sessions) = populated(&log);
    let reader = LogStore::open(&log.path()).unwrap();
    store.forget_passwords().unwrap();

    // The session that was open keeps appending, one line and a batch.
    let sid = sessions[2];
    store.append(sid, 90_000, "> kill rat", None).unwrap();
    store
        .append_batch(&[crate::LogEntry {
            session_id: sid,
            ts_ms: 90_001,
            text: "You slay the rat.".into(),
            raw: Some(b"\x1b[31mYou slay the rat.\x1b[0m".to_vec()),
        }])
        .unwrap();
    store.end_session(sid, 90_002).unwrap();
    // A new session opens and logs too.
    let next = store
        .start_session("play.theforsakenlands.com", 1848, 95_000)
        .unwrap();
    store.append(next, 95_001, "> look", None).unwrap();

    // Both connections read the new lines.
    for s in [&store, &reader] {
        let page = s
            .search_page(
                "slay the rat|^> kill rat$",
                &crate::SearchOptions::default(),
                true,
            )
            .unwrap();
        assert_eq!(page.total, Some(2));
        assert_eq!(s.get_session(next).unwrap().unwrap().line_count, 1);
        let hidden = s
            .search(r"^> \(hidden\)$", &crate::SearchOptions::default())
            .unwrap();
        assert_eq!(hidden.len(), 6);
    }
}
