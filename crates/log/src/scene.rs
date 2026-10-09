//! The rows of one log over a span of time, for Save a scene, each with
//! what the session said it is. A line you sent with a password reads as
//! [`HIDDEN_SENT_TEXT`], as a saved file shows it. It also says which logs
//! a scope holds and when their lines start and end, for the header of a
//! saved file.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::{LineKind, LogStore, Result, Scope, HIDDEN_SENT_TEXT};

/// A log a scene comes from: where it played, who, and when it started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneLog {
    pub id: i64,
    pub host: String,
    pub port: u16,
    pub character: Option<String>,
    pub started_at_ms: i64,
}

/// A log a scope holds, with the times of its first and last line in
/// that scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedLogSpan {
    pub log: SceneLog,
    pub first_ms: i64,
    pub last_ms: i64,
}

/// One row of a scene's span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneLine {
    pub id: i64,
    pub ts_ms: i64,
    /// The plain text.
    pub text: String,
    /// The bytes the game sent, colors included, or None for a row that
    /// kept none, such as a line you sent.
    pub raw: Option<Vec<u8>>,
    /// What the session said the row is, or None for a row an older build
    /// wrote.
    pub kind: Option<LineKind>,
}

impl LogStore {
    /// The log `log`, or None when there is no such log.
    pub fn scene_log(&self, log: i64) -> Result<Option<SceneLog>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, host, port, character, started_at_ms FROM sessions WHERE id = ?1",
                params![log],
                |row| {
                    Ok(SceneLog {
                        id: row.get(0)?,
                        host: row.get(1)?,
                        port: row.get::<_, i64>(2)? as u16,
                        character: row.get(3)?,
                        started_at_ms: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Each log in `scope` that holds a line in it, oldest first, with the
    /// times its lines in scope start and end. It reads one entry of the
    /// `(session_id, ts_ms)` index and two rows by id for each log.
    pub fn scope_spans(&self, scope: &Scope) -> Result<Vec<ScopedLogSpan>> {
        let logs = self.scoped_logs(scope)?;
        let mut at = self
            .conn
            .prepare_cached("SELECT ts_ms FROM log_lines WHERE id = ?1")?;
        let mut spans = Vec::with_capacity(logs.len());
        for (id, scoped) in &logs {
            let Some(log) = self.scene_log(*id)? else {
                continue;
            };
            let first_ms: i64 = at.query_row([scoped.first], |r| r.get(0))?;
            let last_ms: i64 = at.query_row([scoped.last], |r| r.get(0))?;
            spans.push(ScopedLogSpan {
                log,
                first_ms,
                last_ms: last_ms.max(first_ms),
            });
        }
        spans.sort_by_key(|span| (span.log.started_at_ms, span.log.id));
        Ok(spans)
    }

    /// The rows of `log` from `from_ms` to `to_ms`, both ends kept, oldest
    /// first, at most `cap` of them. It reads the `(session_id, ts_ms)`
    /// index. A sent line forget passwords would blank reads as it would
    /// once blanked, as a saved file shows it.
    pub fn scene_lines(
        &self,
        log: i64,
        from_ms: i64,
        to_ms: i64,
        cap: usize,
    ) -> Result<Vec<SceneLine>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ts_ms, text, raw, kind, channel FROM log_lines
             WHERE session_id = ?1 AND ts_ms >= ?2 AND ts_ms <= ?3
             ORDER BY ts_ms, id LIMIT ?4",
        )?;
        let cap = i64::try_from(cap).unwrap_or(i64::MAX);
        let mut lines = stmt
            .query_map(params![log, from_ms, to_ms, cap], |row| {
                Ok(SceneLine {
                    id: row.get(0)?,
                    ts_ms: row.get(1)?,
                    text: row.get(2)?,
                    raw: row.get(3)?,
                    kind: LineKind::from_columns(row.get(4)?, row.get(5)?),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let Some(last) = lines.last().map(|l| l.id) else {
            return Ok(lines);
        };
        // The replay reads the log from its own first line, so a password
        // asked for before the span still hides.
        let mut logs = self.scoped_logs(&Scope::log(log))?;
        if let Some(scoped) = logs.get_mut(&log) {
            scoped.last = scoped.last.min(last);
        }
        let hidden = self.password_lines_in(&logs)?;
        for line in &mut lines {
            if hidden.contains(&line.id) {
                line.text = HIDDEN_SENT_TEXT.to_string();
                line.raw = None;
            }
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::tests::store;
    use crate::LogEntry;

    #[test]
    fn a_scope_says_which_logs_it_holds_and_when() {
        let mut s = store();
        let early = s
            .start_session("play.theforsakenlands.com", 1848, 0)
            .unwrap();
        s.set_session_character(early, "Orla").unwrap();
        s.append(early, 1_000, "Maren walks in.", None).unwrap();
        s.append(early, 5_000, "Maren leaves north.", None).unwrap();
        let late = s
            .start_session("play.theforsakenlands.com", 1848, 9_000)
            .unwrap();
        s.append(late, 9_500, "Tolliver waves.", None).unwrap();
        // A log with no line holds nothing in scope.
        s.start_session("play.theforsakenlands.com", 1848, 10_000)
            .unwrap();
        let spans = s.scope_spans(&Scope::default()).unwrap();
        let got: Vec<_> = spans
            .iter()
            .map(|span| {
                (
                    span.log.id,
                    span.log.character.clone(),
                    span.first_ms,
                    span.last_ms,
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (early, Some("Orla".to_string()), 1_000, 5_000),
                (late, None, 9_500, 9_500),
            ]
        );
        let since = Scope {
            since_ms: Some(2_000),
            ..Scope::default()
        };
        let got: Vec<_> = s
            .scope_spans(&since)
            .unwrap()
            .iter()
            .map(|span| (span.first_ms, span.last_ms))
            .collect();
        assert_eq!(got, vec![(5_000, 5_000), (9_500, 9_500)]);
    }

    #[test]
    fn a_span_reads_its_rows_with_their_kinds() {
        let mut s = store();
        let id = s
            .start_session("play.theforsakenlands.com", 1848, 0)
            .unwrap();
        s.set_session_character(id, "Orla").unwrap();
        // An older build's row, then the rows this build writes.
        s.append(id, 5, "Maren walks in.", None).unwrap();
        s.conn
            .execute("UPDATE log_lines SET kind = NULL", [])
            .unwrap();
        let row = |ts_ms: i64, text: &str, kind: LineKind| LogEntry {
            session_id: id,
            ts_ms,
            text: text.into(),
            raw: None,
            kind,
        };
        s.append_batch(&[
            row(10, "<1020hp 800m 930mv> ", LineKind::Prompt),
            row(20, "> look", LineKind::Sent),
            row(
                30,
                "Tolliver tells you '[Exits: north east south west]'",
                LineKind::Channel("tell".into()),
            ),
            row(40, "Thickening Woods", LineKind::Text),
        ])
        .unwrap();
        let log = s.scene_log(id).unwrap().unwrap();
        assert_eq!(log.character.as_deref(), Some("Orla"));
        assert_eq!(
            (log.host.as_str(), log.port),
            ("play.theforsakenlands.com", 1848)
        );
        let lines = s.scene_lines(id, 5, 30, 100).unwrap();
        let kinds: Vec<_> = lines.iter().map(|l| l.kind.clone()).collect();
        assert_eq!(
            kinds,
            [
                None,
                Some(LineKind::Prompt),
                Some(LineKind::Sent),
                Some(LineKind::Channel("tell".into())),
            ]
        );
        assert_eq!(s.scene_lines(id, 0, 100, 2).unwrap().len(), 2);
        assert_eq!(s.scene_lines(id, 50, 60, 10).unwrap(), Vec::new());
        assert_eq!(s.scene_log(id + 1).unwrap(), None);
    }

    #[test]
    fn a_password_in_the_span_reads_hidden() {
        // A made up secret. It is nobody's password.
        const SECRET: &str = "Zq7vellumSparrow";
        let mut s = store();
        let id = s
            .start_session("play.theforsakenlands.com", 1848, 0)
            .unwrap();
        s.append_raw(id, 0, b"\x1b[0mAbandon hope, all ye who enter here...")
            .unwrap();
        s.append(id, 1, "> e", None).unwrap();
        s.append_raw(id, 2, b"\x1b[0m").unwrap();
        s.append(id, 3, "> tester", None).unwrap();
        s.append_raw(id, 4, b"\x1b[0m").unwrap();
        s.append(id, 5, &format!("> {SECRET}"), None).unwrap();
        // The span starts after the game asked, and the line still hides.
        let lines = s.scene_lines(id, 5, 5, 10).unwrap();
        assert_eq!(lines[0].text, HIDDEN_SENT_TEXT);
        assert_eq!(lines[0].raw, None);
    }
}
