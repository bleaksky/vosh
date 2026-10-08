//! The regex search the log view runs, one page at a time.
//!
//! A search first turns its [`Scope`] into the logs it may read and the
//! span of line ids they hold. Line ids grow with time, so a time scope
//! needs no index of its own: each log's first line at or after the
//! start comes from the `(session_id, ts_ms)` index. Then it reads that
//! span newest chunk first, each chunk in ascending id order, which the
//! system reads ahead well, and stops once it holds a page unless it was
//! asked to count every match. It reads the colored bytes of the lines
//! it keeps only, at the end.

use std::collections::HashMap;

use regex::RegexBuilder;
use rusqlite::params;
use serde::Serialize;

use crate::sessions::not_local_sql;
use crate::{LogError, LogStore, Result};

/// How many line ids one chunk of a search reads.
const CHUNK: i64 = 32_768;

/// How many rows a search reads between two looks at whether it should
/// stop.
const STOP_EVERY: usize = 4_096;

#[derive(Debug, Clone, Serialize)]
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

/// The logs a search, a list or a save reads.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// Only these logs. None reads every log the rest allows.
    pub logs: Option<Vec<i64>>,
    /// Only the logs of connections to this host and port. The host
    /// matches without regard to case or the spaces around it.
    pub world: Option<(String, u16)>,
    /// Only the lines at or after this time, in Unix ms, and the logs
    /// that hold one.
    pub since_ms: Option<i64>,
    /// Leave out logs of connections to this machine (see `LOCAL_HOSTS`),
    /// like a test server run next to the client.
    pub hide_local: bool,
}

impl Scope {
    /// One log.
    pub fn log(id: i64) -> Self {
        Self {
            logs: Some(vec![id]),
            ..Self::default()
        }
    }

    /// The SQL test on `sessions` as `s` for every part of the scope but
    /// the time, with its parameters from `?first` on.
    pub(crate) fn session_filter(&self, first: usize) -> (String, Vec<rusqlite::types::Value>) {
        use rusqlite::types::Value;
        let mut conditions = Vec::new();
        let mut values = Vec::new();
        if let Some(logs) = &self.logs {
            if logs.is_empty() {
                conditions.push("0".to_string());
            } else {
                let list = logs
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                conditions.push(format!("s.id IN ({list})"));
            }
        }
        if let Some((host, port)) = &self.world {
            values.push(Value::Text(host.clone()));
            values.push(Value::Integer(i64::from(*port)));
            let at = first + values.len() - 2;
            conditions.push(format!(
                "lower(trim(s.host)) = lower(trim(?{at})) AND s.port = ?{}",
                at + 1
            ));
        }
        if self.hide_local {
            conditions.push(not_local_sql());
        }
        if let Some(since) = self.since_ms {
            // A log still open, or one that ended after the start, may
            // hold a line in scope.
            values.push(Value::Integer(since));
            let at = first + values.len() - 1;
            conditions.push(format!("(s.ended_at_ms IS NULL OR s.ended_at_ms >= ?{at})"));
        }
        let filter = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        (filter, values)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    /// Cap the number of hits returned. Zero means "no cap".
    pub max_results: usize,
    pub scope: Scope,
    /// Only lines older than this line id. The log view pages back
    /// through a long result with the oldest line id it already holds.
    pub before_line_id: Option<i64>,
}

/// One page of a search: the newest matches in scope, oldest first,
/// and optionally how many lines in scope match in all.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    /// Every matching line in scope, past the cap too. `None` when the
    /// caller did not ask for it, which lets a capped search stop at
    /// the cap instead of reading the whole scope.
    pub total: Option<u64>,
}

/// A log in scope, with its world and the span of line ids it holds in
/// scope.
#[derive(Debug, Clone)]
pub(crate) struct ScopedLog {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) first: i64,
    pub(crate) last: i64,
}

/// A match a search keeps before it reads the line's bytes.
struct Kept {
    line_id: i64,
    session_id: i64,
    ts_ms: i64,
    text: String,
}

impl LogStore {
    /// [`Self::search_page`] without the total. Test only. The app's
    /// tests search through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn search(&self, pattern: &str, options: &SearchOptions) -> Result<Vec<SearchHit>> {
        Ok(self.search_page(pattern, options, false)?.hits)
    }

    /// [`Self::search_page_until`] that never stops early.
    pub fn search_page(
        &self,
        pattern: &str,
        options: &SearchOptions,
        with_total: bool,
    ) -> Result<SearchPage> {
        self.search_page_until(pattern, options, with_total, &|| false)
    }

    /// The logs in `scope`, by id, with the span of line ids each holds
    /// in scope. A log with no line in scope is left out.
    pub(crate) fn scoped_logs(&self, scope: &Scope) -> Result<HashMap<i64, ScopedLog>> {
        let (filter, values) = scope.session_filter(1);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT s.id, s.host, s.port FROM sessions s {filter}"
        ))?;
        let rows: Vec<(i64, String, i64)> = stmt
            .query_map(rusqlite::params_from_iter(values), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<std::result::Result<_, _>>()?;
        // Each reads one entry of the (session_id, ts_ms) index.
        let mut first_at = self.conn.prepare_cached(
            "SELECT id FROM log_lines WHERE session_id = ?1 AND ts_ms >= ?2
             ORDER BY ts_ms, id LIMIT 1",
        )?;
        let mut last = self.conn.prepare_cached(
            "SELECT id FROM log_lines WHERE session_id = ?1
             ORDER BY ts_ms DESC, id DESC LIMIT 1",
        )?;
        let since = scope.since_ms.unwrap_or(i64::MIN);
        let mut logs = HashMap::new();
        for (id, host, port) in rows {
            let first: Option<i64> = first_at
                .query_row(params![id, since], |r| r.get(0))
                .map(Some)
                .or_else(none_when_empty)?;
            let Some(first) = first else {
                continue;
            };
            let last: i64 = last.query_row(params![id], |r| r.get(0))?;
            logs.insert(
                id,
                ScopedLog {
                    host,
                    port: port as u16,
                    first,
                    last: last.max(first),
                },
            );
        }
        Ok(logs)
    }

    /// Run a regex search over the lines in `options.scope`. The newest
    /// `max_results` matches come back oldest first, so a page reads
    /// like the original conversation from the top.
    ///
    /// With `with_total` the page also counts every line in scope that
    /// matches, past the cap too. The scope includes `before_line_id`, so
    /// a page taken before a line counts only the lines older than it.
    /// Without it the search stops once it holds a page. An empty pattern
    /// matches every line, and its count comes from the index alone.
    ///
    /// `stop` is asked now and then while the search reads, and a true
    /// ends it with [`LogError::Stopped`], so a search the next keystroke
    /// replaces lets the reader go.
    pub fn search_page_until(
        &self,
        pattern: &str,
        options: &SearchOptions,
        with_total: bool,
        stop: &dyn Fn() -> bool,
    ) -> Result<SearchPage> {
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(!options.case_sensitive)
            .build()?;
        let match_all = pattern.is_empty();
        let cap = if options.max_results == 0 {
            usize::MAX
        } else {
            options.max_results
        };
        let logs = self.scoped_logs(&options.scope)?;
        let since = options.scope.since_ms.unwrap_or(i64::MIN);
        let before = options.before_line_id.unwrap_or(i64::MAX);
        let low = logs.values().map(|l| l.first).min();
        let high = logs.values().map(|l| l.last).max();
        let (Some(low), Some(high)) = (low, high) else {
            return Ok(SearchPage {
                hits: Vec::new(),
                total: with_total.then_some(0),
            });
        };
        // Read every chunk only to count regex matches past the cap.
        let count_rows = with_total && !match_all;

        let mut stmt = self.conn.prepare_cached(
            "SELECT id, session_id, ts_ms, text FROM log_lines
             WHERE id >= ?1 AND id < ?2 ORDER BY id",
        )?;
        // Newest chunk first, and the newest matches of each chunk.
        let mut chunks: Vec<Vec<Kept>> = Vec::new();
        let mut held = 0usize;
        let mut matched: u64 = 0;
        let mut read = 0usize;
        let mut end = high.saturating_add(1).min(before);
        while end > low {
            if held >= cap && !count_rows {
                break;
            }
            if stop() {
                return Err(LogError::Stopped);
            }
            let start = end.saturating_sub(CHUNK).max(low);
            let mut found = Vec::new();
            let mut rows = stmt.query(params![start, end])?;
            while let Some(row) = rows.next()? {
                read += 1;
                if read % STOP_EVERY == 0 && stop() {
                    return Err(LogError::Stopped);
                }
                let session_id: i64 = row.get(1)?;
                if !logs.contains_key(&session_id) {
                    continue;
                }
                let ts_ms: i64 = row.get(2)?;
                if ts_ms < since {
                    continue;
                }
                let text: String = row.get(3)?;
                if !match_all && !regex.is_match(&text) {
                    continue;
                }
                found.push(Kept {
                    line_id: row.get(0)?,
                    session_id,
                    ts_ms,
                    text,
                });
            }
            drop(rows);
            matched += found.len() as u64;
            let room = cap - held;
            if found.len() > room {
                found.drain(..found.len() - room);
            }
            held += found.len();
            chunks.push(found);
            end = start;
        }

        let mut hits = Vec::with_capacity(held);
        let mut raw = self
            .conn
            .prepare_cached("SELECT raw FROM log_lines WHERE id = ?1")?;
        for kept in chunks.into_iter().rev().flatten() {
            let log = &logs[&kept.session_id];
            hits.push(SearchHit {
                raw: raw.query_row(params![kept.line_id], |r| r.get(0))?,
                session_id: kept.session_id,
                host: log.host.clone(),
                port: log.port,
                line_id: kept.line_id,
                ts_ms: kept.ts_ms,
                text: kept.text,
            });
        }

        let total = if !with_total {
            None
        } else if match_all {
            Some(self.count_in_scope(&logs, since, before)?)
        } else {
            Some(matched)
        };
        Ok(SearchPage { hits, total })
    }

    /// How many lines the logs in scope hold at or after `since` and
    /// before line `before`, from the `(session_id, ts_ms)` index alone.
    fn count_in_scope(
        &self,
        logs: &HashMap<i64, ScopedLog>,
        since: i64,
        before: i64,
    ) -> Result<u64> {
        let mut count = self.conn.prepare_cached(
            "SELECT COUNT(*) FROM log_lines
             WHERE session_id = ?1 AND ts_ms >= ?2 AND id < ?3",
        )?;
        let mut total = 0u64;
        for id in logs.keys() {
            let n: i64 = count.query_row(params![id, since, before], |r| r.get(0))?;
            total += u64::try_from(n).unwrap_or(0);
        }
        Ok(total)
    }
}

/// `QueryReturnedNoRows` as None, every other error as itself.
fn none_when_empty<T>(e: rusqlite::Error) -> rusqlite::Result<Option<T>> {
    match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        e => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::is_local_host;
    use crate::sessions::tests::{store, store_with_local_sessions};
    use crate::LogError;

    #[test]
    fn search_matches_lines() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append(id, 1, "the dragon roars", None).unwrap();
        s.append(id, 2, "you draw a sword", None).unwrap();
        s.append(id, 3, "the dragon dies", None).unwrap();
        let hits = s.search("dragon", &SearchOptions::default()).unwrap();
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
            ..SearchOptions::default()
        };
        let hits = s.search("foo", &opts).unwrap();
        assert_eq!(hits.len(), 1);

        let opts = SearchOptions {
            max_results: 2,
            ..SearchOptions::default()
        };
        let hits = s.search("foo", &opts).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn search_returns_oldest_first_within_cap() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append(id, 1, "match one", None).unwrap();
        s.append(id, 2, "match two", None).unwrap();
        s.append(id, 3, "match three", None).unwrap();
        s.append(id, 4, "match four", None).unwrap();
        // Cap to the 3 most-recent matches. Results should be those
        // three, ordered oldest to newest: two, three, four.
        let opts = SearchOptions {
            max_results: 3,
            ..SearchOptions::default()
        };
        let hits = s.search("match", &opts).unwrap();
        let texts: Vec<_> = hits.iter().map(|h| h.text.clone()).collect();
        assert_eq!(texts, vec!["match two", "match three", "match four"]);
    }

    #[test]
    fn search_can_filter_by_session() {
        let mut s = store();
        let a = s.start_session("a", 1, 0).unwrap();
        let b = s.start_session("b", 2, 0).unwrap();
        s.append(a, 1, "foo from a", None).unwrap();
        s.append(b, 2, "foo from b", None).unwrap();
        let opts = SearchOptions {
            scope: Scope::log(a),
            ..SearchOptions::default()
        };
        let hits = s.search("foo", &opts).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("from a"));
        assert_eq!((hits[0].host.as_str(), hits[0].port), ("a", 1));
    }

    #[test]
    fn search_can_hide_local_sessions() {
        let (s, _, _) = store_with_local_sessions();
        let every = s.search("line", &SearchOptions::default()).unwrap();
        assert_eq!(every.len(), 15);
        let opts = SearchOptions {
            scope: Scope {
                hide_local: true,
                ..Scope::default()
            },
            ..SearchOptions::default()
        };
        let page = s.search_page("line", &opts, true).unwrap();
        assert_eq!(page.total, Some(6));
        assert!(page.hits.iter().all(|h| !is_local_host(&h.host)));
        // An empty pattern counts through the index and hides them the
        // same way.
        let page = s.search_page("", &opts, true).unwrap();
        assert_eq!(page.total, Some(6));
        assert_eq!(page.hits.len(), 6);
    }

    #[test]
    fn a_world_scope_reads_the_logs_of_one_host_and_port() {
        let mut s = store();
        let game = s
            .start_session("Play.TheForsakenLands.com ", 9009, 0)
            .unwrap();
        let other_port = s
            .start_session("play.theforsakenlands.com", 4000, 10)
            .unwrap();
        let other_host = s.start_session("mud.example.org", 9009, 20).unwrap();
        for (id, n) in [(game, 1), (other_port, 2), (other_host, 3)] {
            s.append(id, n, &format!("Tolliver waves {n}"), None)
                .unwrap();
        }
        let opts = SearchOptions {
            scope: Scope {
                world: Some(("play.theforsakenlands.com".into(), 9009)),
                ..Scope::default()
            },
            ..SearchOptions::default()
        };
        let page = s.search_page("Tolliver", &opts, true).unwrap();
        assert_eq!(page.total, Some(1));
        assert_eq!(page.hits[0].session_id, game);
    }

    #[test]
    fn a_time_scope_reads_the_lines_since_its_start() {
        let mut s = store();
        let old = s.start_session("h", 1, 0).unwrap();
        s.append(old, 100, "Maren nods, old", None).unwrap();
        s.end_session(old, 150).unwrap();
        let both = s.start_session("h", 1, 900).unwrap();
        s.append(both, 950, "Maren nods, early", None).unwrap();
        s.append(both, 1_050, "Maren nods, late", None).unwrap();
        let opts = SearchOptions {
            scope: Scope {
                since_ms: Some(1_000),
                ..Scope::default()
            },
            ..SearchOptions::default()
        };
        let page = s.search_page("Maren", &opts, true).unwrap();
        let texts: Vec<_> = page.hits.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, ["Maren nods, late"]);
        assert_eq!(page.total, Some(1));
        let every = s.search_page("", &opts, true).unwrap();
        assert_eq!(every.total, Some(1));
        // Nothing since a later start.
        let later = SearchOptions {
            scope: Scope {
                since_ms: Some(5_000),
                ..Scope::default()
            },
            ..SearchOptions::default()
        };
        let none = s.search_page("", &later, true).unwrap();
        assert_eq!((none.hits.len(), none.total), (0, Some(0)));
    }

    #[test]
    fn a_page_spans_chunks_in_order() {
        let mut s = store();
        let a = s.start_session("h", 1, 0).unwrap();
        let b = s.start_session("h", 1, 0).unwrap();
        let rows: Vec<crate::LogEntry> = (0..(CHUNK * 2 + 10))
            .map(|n| crate::LogEntry {
                session_id: if n % 2 == 0 { a } else { b },
                ts_ms: n,
                text: if n % 1_000 == 0 {
                    format!("Orla waves {n}")
                } else {
                    format!("filler {n}")
                },
                raw: None,
            })
            .collect();
        s.append_batch(&rows).unwrap();
        let opts = SearchOptions {
            max_results: 40,
            ..SearchOptions::default()
        };
        let page = s.search_page("Orla", &opts, true).unwrap();
        assert_eq!(page.total, Some(66));
        let ts: Vec<i64> = page.hits.iter().map(|h| h.ts_ms).collect();
        let want: Vec<i64> = (26..66).map(|n| n * 1_000).collect();
        assert_eq!(ts, want);
        // A log scope reads only that log's lines in every chunk.
        let opts = SearchOptions {
            scope: Scope::log(b),
            ..SearchOptions::default()
        };
        let page = s.search_page("Orla", &opts, true).unwrap();
        assert_eq!(page.total, Some(0));
    }

    #[test]
    fn a_search_stops_when_asked() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        s.append(id, 1, "Tolliver bows", None).unwrap();
        let stopped = s.search_page_until("Tolliver", &SearchOptions::default(), true, &|| true);
        assert!(matches!(stopped, Err(LogError::Stopped)));
    }

    #[test]
    fn search_page_counts_every_match_past_the_cap() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        for n in 0..10 {
            s.append(id, n, &format!("match {n}"), None).unwrap();
            s.append(id, n, &format!("other {n}"), None).unwrap();
        }
        let opts = SearchOptions {
            max_results: 3,
            ..SearchOptions::default()
        };
        let page = s.search_page("match", &opts, true).unwrap();
        assert_eq!(page.total, Some(10));
        let texts: Vec<_> = page.hits.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, vec!["match 7", "match 8", "match 9"]);
        // Without the total the page is the same and carries no count.
        let quick = s.search_page("match", &opts, false).unwrap();
        assert_eq!(quick.total, None);
        assert_eq!(quick.hits.len(), 3);
        // An empty pattern takes every line, newest three, all twenty.
        let all = s.search_page("", &opts, true).unwrap();
        assert_eq!(all.total, Some(20));
        let texts: Vec<_> = all.hits.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, vec!["other 8", "match 9", "other 9"]);
    }

    #[test]
    fn search_page_pages_back_before_a_line() {
        let mut s = store();
        let id = s.start_session("h", 1, 0).unwrap();
        for n in 0..7 {
            s.append(id, n, &format!("match {n}"), None).unwrap();
        }
        let mut opts = SearchOptions {
            max_results: 3,
            ..SearchOptions::default()
        };
        let first = s.search_page("match", &opts, true).unwrap();
        assert_eq!(first.total, Some(7));
        opts.before_line_id = Some(first.hits[0].line_id);
        let second = s.search_page("match", &opts, true).unwrap();
        let texts: Vec<_> = second.hits.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, vec!["match 1", "match 2", "match 3"]);
        // The total covers the lines older than the cursor.
        assert_eq!(second.total, Some(4));
        // An empty pattern counts the same lines.
        assert_eq!(s.search_page("", &opts, true).unwrap().total, Some(4));
        opts.before_line_id = Some(second.hits[0].line_id);
        let last = s.search_page("match", &opts, false).unwrap();
        let texts: Vec<_> = last.hits.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, vec!["match 0"]);
    }

    #[test]
    fn search_page_reports_a_bad_pattern() {
        let s = store();
        assert!(matches!(
            s.search_page("(", &SearchOptions::default(), true),
            Err(LogError::Regex(_))
        ));
    }
}
