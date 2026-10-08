//! `#logs forget-passwords`. Counts the lines in the session log where you
//! sent a password, and with `now` blanks them for good. The rule for
//! which lines lives in `vosh_log`. crates/log/src/forget.rs states it,
//! and crates/log/src/forget/ holds the login replay and the wipe.
//!
//! Nothing here reads, prints, logs, or returns the text of a line. The
//! store hands back row ids and counts only, and an error names what
//! failed, never a row.

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tracing::warn;
use vosh_log::{Forgotten, PasswordLines};

use crate::app::state::SharedState;
use crate::input::LogsCommand;
use crate::logs::SharedLogStore;
use crate::sessions::Session;

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Logging is off, so there is no store to look through.
    NoLog,
    /// The pass over the log failed. Nothing changed.
    ReadFailed,
    /// The count, for `#logs forget-passwords`.
    Found(PasswordLines),
    /// What `#logs forget-passwords now` blanked.
    Blanked(Forgotten),
    /// The update failed and rolled back. Nothing changed.
    BlankFailed,
}

/// Find the lines, and with `now` blank them.
///
/// The pass over the log reads through the search connection, so only a
/// search waits on it and the session keeps logging. The blanking holds
/// the writer the way every log write does, so the session's next append
/// waits for it instead of racing it, and holds the search connection too
/// so no search keeps an old snapshot open while the write ahead log is
/// truncated. Both steps tell the runtime they block, so the tasks that
/// share the thread, the session among them, move elsewhere meanwhile.
pub(crate) async fn forget(logs: &SharedLogStore, reader: &SharedLogStore, now: bool) -> Outcome {
    let read = {
        let guard = reader.lock().await;
        guard
            .as_ref()
            .map(|store| blocking(|| store.find_password_lines()))
    };
    let read = match read {
        Some(read) => read,
        None => {
            let guard = logs.lock().await;
            match guard.as_ref() {
                Some(store) => blocking(|| store.find_password_lines()),
                None => return Outcome::NoLog,
            }
        }
    };
    let found = match read {
        Ok(found) => found,
        Err(e) => {
            warn!(error = %e, "#logs forget-passwords could not read the session log");
            return Outcome::ReadFailed;
        }
    };
    if !now {
        return Outcome::Found(found);
    }
    let mut writer = logs.lock().await;
    let _searches_wait = reader.lock().await;
    let Some(store) = writer.as_mut() else {
        return Outcome::NoLog;
    };
    match blocking(|| store.blank_password_lines(&found)) {
        Ok(done) => {
            if !done.wiped {
                warn!(
                    lines = done.lines,
                    resumed = done.resumed,
                    "#logs forget-passwords could not rewrite the log file, the next run tries again"
                );
            }
            Outcome::Blanked(done)
        }
        Err(e) => {
            warn!(error = %e, "#logs forget-passwords could not blank the session log");
            Outcome::BlankFailed
        }
    }
}

/// Run `work`, telling a multi thread runtime that it blocks, so a pass
/// over a large log never stalls the tasks that share its thread.
fn blocking<T>(work: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current().map(|h| h.runtime_flavor()) {
        Ok(tokio::runtime::RuntimeFlavor::MultiThread) => tokio::task::block_in_place(work),
        _ => work(),
    }
}

/// `n` with the singular or plural noun.
fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The line to echo for `outcome`.
pub(crate) fn message(outcome: &Outcome) -> String {
    const NONE: &str =
        "Vosh found no lines where you sent a password. Your log has nothing to blank.";
    const FINISH: &str = "Type #logs forget-passwords now again to finish.";
    match outcome {
        Outcome::NoLog => "Vosh has no session log open, so there is nothing to blank.".to_string(),
        Outcome::ReadFailed => "Vosh could not read your session log. Nothing changed.".to_string(),
        Outcome::BlankFailed => {
            "Vosh could not blank the lines in your session log. Nothing changed.".to_string()
        }
        Outcome::Found(found) if found.count() == 0 && found.wipe_pending => {
            "Vosh found no lines where you sent a password, but old copies of lines \
             it blanked before are still in the log file. \
             Type #logs forget-passwords now to clear them."
                .to_string()
        }
        Outcome::Found(found) if found.count() == 0 => NONE.to_string(),
        // The store calls one logged connection a session, but session
        // means a tab now, so the line counts logs as Settings does (Q21).
        Outcome::Found(found) => format!(
            "Vosh found {} where you sent a password, across {}. \
             Type #logs forget-passwords now to blank {}.",
            count(found.count(), "line", "lines"),
            count(found.sessions(), "log", "logs"),
            if found.count() == 1 { "it" } else { "them" },
        ),
        Outcome::Blanked(done) if done.lines == 0 && !done.resumed => NONE.to_string(),
        Outcome::Blanked(done) if done.lines == 0 && done.wiped => {
            "Vosh cleared old copies of lines it blanked before from the disk. \
             It found no other lines where you sent a password."
                .to_string()
        }
        Outcome::Blanked(done) if done.lines == 0 => format!(
            "Vosh still could not rewrite the log file to clear old copies of lines \
             it blanked before. {FINISH}"
        ),
        Outcome::Blanked(done) if done.wiped => format!(
            "Vosh blanked {}. Your log keeps the fact that you sent {}, never the text.",
            count(done.lines, "line", "lines"),
            if done.lines == 1 { "it" } else { "them" },
        ),
        Outcome::Blanked(done) => format!(
            "Vosh blanked {}, but it could not rewrite the log file to clear \
             old copies of {} text from the disk. {FINISH}",
            count(done.lines, "line", "lines"),
            if done.lines == 1 { "its" } else { "their" },
        ),
    }
}

/// The echo for a `#logs` line that is not a known command.
pub(crate) const USAGE: &str = "[usage #logs forget-passwords [now]]";

/// Run `command` off the input path and echo what it found or did in
/// the terminal of `session`, where you typed it.
pub(crate) fn start<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Arc<Session>,
    command: LogsCommand,
) {
    let now = match command {
        LogsCommand::Usage => {
            echo(app, session, USAGE);
            return;
        }
        LogsCommand::Preview => false,
        LogsCommand::Forget => true,
    };
    let app = app.clone();
    let session = Arc::clone(session);
    tauri::async_runtime::spawn(async move {
        let state: SharedState = app.state::<SharedState>().inner().clone();
        let outcome = forget(&state.logs, &state.log_reader, now).await;
        echo(&app, &session, &message(&outcome));
    });
}

/// Print `line` in the terminal of `session` the way other slash
/// commands do, through the one path that feeds the native renderer and
/// xterm alike.
fn echo<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, line: &str) {
    crate::output::emit_output(app, session, format!("{line}\r\n").into_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use vosh_log::LogStore;

    #[test]
    fn the_preview_says_how_many_and_how_to_blank_them() {
        let found = PasswordLines {
            lines: vec![(3, 1), (9, 1), (40, 2), (41, 2), (77, 5)],
            wipe_pending: false,
        };
        assert_eq!(
            message(&Outcome::Found(found)),
            "Vosh found 5 lines where you sent a password, across 3 logs. \
                 Type #logs forget-passwords now to blank them."
        );
        let one = PasswordLines {
            lines: vec![(3, 1)],
            wipe_pending: false,
        };
        assert_eq!(
            message(&Outcome::Found(one)),
            "Vosh found 1 line where you sent a password, across 1 log. \
                 Type #logs forget-passwords now to blank it."
        );
    }

    #[test]
    fn the_real_run_says_what_it_blanked() {
        let done = Forgotten {
            lines: 12,
            sessions: 9,
            resumed: false,
            wiped: true,
        };
        assert_eq!(
            message(&Outcome::Blanked(done)),
            "Vosh blanked 12 lines. Your log keeps the fact that you sent them, never the text."
        );
        let one = Forgotten {
            lines: 1,
            sessions: 1,
            resumed: false,
            wiped: true,
        };
        assert_eq!(
            message(&Outcome::Blanked(one)),
            "Vosh blanked 1 line. Your log keeps the fact that you sent it, never the text."
        );
        let unwiped = Forgotten {
            lines: 2,
            sessions: 1,
            resumed: false,
            wiped: false,
        };
        assert_eq!(
            message(&Outcome::Blanked(unwiped)),
            "Vosh blanked 2 lines, but it could not rewrite the log file to clear \
                 old copies of their text from the disk. \
                 Type #logs forget-passwords now again to finish."
        );
    }

    #[test]
    fn a_wipe_left_unfinished_says_how_to_finish_it() {
        // The preview finds nothing new but knows the file still holds
        // old copies of lines an earlier run blanked.
        let pending = PasswordLines {
            lines: Vec::new(),
            wipe_pending: true,
        };
        assert_eq!(
            message(&Outcome::Found(pending)),
            "Vosh found no lines where you sent a password, but old copies of lines \
                 it blanked before are still in the log file. \
                 Type #logs forget-passwords now to clear them."
        );
        // New lines to blank take the usual count, and the real run
        // clears the old copies with them.
        let both = PasswordLines {
            lines: vec![(3, 1)],
            wipe_pending: true,
        };
        assert_eq!(
            message(&Outcome::Found(both)),
            "Vosh found 1 line where you sent a password, across 1 log. \
                 Type #logs forget-passwords now to blank it."
        );

        let finished = Forgotten {
            lines: 0,
            sessions: 0,
            resumed: true,
            wiped: true,
        };
        assert_eq!(
            message(&Outcome::Blanked(finished)),
            "Vosh cleared old copies of lines it blanked before from the disk. \
                 It found no other lines where you sent a password."
        );
        let still = Forgotten {
            lines: 0,
            sessions: 0,
            resumed: true,
            wiped: false,
        };
        assert_eq!(
            message(&Outcome::Blanked(still)),
            "Vosh still could not rewrite the log file to clear old copies of lines \
                 it blanked before. Type #logs forget-passwords now again to finish."
        );
        let more = Forgotten {
            lines: 3,
            sessions: 2,
            resumed: true,
            wiped: true,
        };
        assert_eq!(
            message(&Outcome::Blanked(more)),
            "Vosh blanked 3 lines. Your log keeps the fact that you sent them, never the text."
        );
    }

    #[test]
    fn nothing_to_blank_says_so() {
        let none = "Vosh found no lines where you sent a password. Your log has nothing to blank.";
        assert_eq!(message(&Outcome::Found(PasswordLines::default())), none);
        assert_eq!(message(&Outcome::Blanked(Forgotten::default())), none);
    }

    #[test]
    fn failures_say_nothing_changed() {
        assert_eq!(
            message(&Outcome::NoLog),
            "Vosh has no session log open, so there is nothing to blank."
        );
        assert_eq!(
            message(&Outcome::ReadFailed),
            "Vosh could not read your session log. Nothing changed."
        );
        assert_eq!(
            message(&Outcome::BlankFailed),
            "Vosh could not blank the lines in your session log. Nothing changed."
        );
    }

    #[test]
    fn no_message_sends_you_to_the_log_from_before_the_rename() {
        // The rename left a copy of logs.sqlite in the old app data
        // folder. The builds that wrote it logged game output only, and
        // the game never prints a password, so that copy holds none and
        // no outcome points at it.
        let found = PasswordLines {
            lines: vec![(3, 1)],
            wipe_pending: true,
        };
        let done = Forgotten {
            lines: 1,
            sessions: 1,
            resumed: true,
            wiped: false,
        };
        for outcome in [
            Outcome::NoLog,
            Outcome::ReadFailed,
            Outcome::BlankFailed,
            Outcome::Found(PasswordLines::default()),
            Outcome::Found(found),
            Outcome::Blanked(Forgotten::default()),
            Outcome::Blanked(done),
        ] {
            let text = message(&outcome);
            assert!(!text.contains("took its name"), "{outcome:?}");
            assert!(!text.contains("delete"), "{outcome:?}");
        }
    }

    /// A log with one session that holds two made up passwords: a line
    /// after a logged prompt and a password command.
    fn shared_log(dir: &Path) -> (SharedLogStore, SharedLogStore, i64) {
        let path = dir.join("logs.sqlite");
        let mut writer = LogStore::open(&path).unwrap();
        let sid = writer
            .start_session("play.theforsakenlands.com", 1848, 0)
            .unwrap();
        writer.append_raw(sid, 1, b"Password: ").unwrap();
        writer.append(sid, 2, "> Hb3flintOtter", None).unwrap();
        writer.append_raw(sid, 3, b"Welcome.").unwrap();
        writer.append(sid, 4, "> look", None).unwrap();
        writer
            .append(sid, 5, "> password Hb3flintOtter Rt8emberFinch", None)
            .unwrap();
        let reader = LogStore::open(&path).unwrap();
        (
            Arc::new(Mutex::new(Some(writer))),
            Arc::new(Mutex::new(Some(reader))),
            sid,
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_preview_then_a_real_run_then_the_session_keeps_logging() {
        let dir = tempfile::tempdir().unwrap();
        let (logs, reader, sid) = shared_log(dir.path());

        let Outcome::Found(found) = forget(&logs, &reader, false).await else {
            panic!("the preview did not count");
        };
        assert_eq!((found.count(), found.sessions()), (2, 1));

        let Outcome::Blanked(done) = forget(&logs, &reader, true).await else {
            panic!("the real run did not blank");
        };
        assert_eq!((done.lines, done.sessions, done.wiped), (2, 1, true));

        // The session loop appends through the same shared store.
        {
            let mut guard = logs.lock().await;
            let store = guard.as_mut().unwrap();
            store.append(sid, 6, "> north", None).unwrap();
            store
                .append_batch(&[vosh_log::LogEntry {
                    session_id: sid,
                    ts_ms: 7,
                    text: "A dusty road.".into(),
                    raw: Some(b"A dusty road.".to_vec()),
                    kind: vosh_log::LineKind::Text,
                }])
                .unwrap();
        }
        let guard = reader.lock().await;
        let store = guard.as_ref().unwrap();
        let hits = store
            .search(
                "^> north$|^A dusty road\\.$",
                &vosh_log::SearchOptions::default(),
            )
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(store.get_session(sid).unwrap().unwrap().line_count, 7);
        drop(guard);

        let Outcome::Blanked(again) = forget(&logs, &reader, true).await else {
            panic!("the second run did not report");
        };
        assert_eq!(again.lines, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn no_store_is_no_log() {
        let none: SharedLogStore = Arc::new(Mutex::new(None));
        assert_eq!(forget(&none, &none, false).await, Outcome::NoLog);
        assert_eq!(forget(&none, &none, true).await, Outcome::NoLog);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_writer_alone_serves_when_the_reader_did_not_open() {
        let dir = tempfile::tempdir().unwrap();
        let (logs, _reader, _) = shared_log(dir.path());
        let none: SharedLogStore = Arc::new(Mutex::new(None));
        let Outcome::Found(found) = forget(&logs, &none, false).await else {
            panic!("the preview did not count");
        };
        assert_eq!(found.count(), 2);
    }
}
