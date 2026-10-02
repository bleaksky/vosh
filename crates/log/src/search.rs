//! The regex search the log view runs, one page at a time.

use regex::{Regex, RegexBuilder};
use serde::Serialize;

use crate::sessions::not_local_sql;
use crate::{LogStore, Result};

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

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    /// Cap the number of hits returned. Zero means "no cap".
    pub max_results: usize,
    /// Optional `session_id` filter; None searches every session.
    pub session_id: Option<i64>,
    /// Only lines older than this line id. The log view pages back
    /// through a long result with the oldest line id it already holds.
    pub before_line_id: Option<i64>,
    /// Leave out sessions to this machine (see `LOCAL_HOSTS`), like
    /// a test server run next to the client.
    pub hide_local: bool,
}

/// One page of a search: the newest matches in scope, oldest first,
/// and optionally how many lines in scope match in all.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    /// Every matching line in scope, past the cap too. `None` when the
    /// caller did not ask for it, which lets a capped search stop at
    /// the cap instead of reading the whole log.
    pub total: Option<u64>,
}

impl LogStore {
    /// [`Self::search_page`] without the total. Test only. The app's
    /// tests search through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn search(&self, pattern: &str, options: &SearchOptions) -> Result<Vec<SearchHit>> {
        Ok(self.search_page(pattern, options, false)?.hits)
    }

    /// Run a regex search over log lines. The most recent
    /// `max_results` matches are returned, in chronological (oldest
    /// first) order — that way a UI rendering top-to-bottom reads
    /// like the original conversation. Internally the SQL walks the
    /// table id-descending so the cap selects the newest matches;
    /// the gathered slice is reversed before return.
    ///
    /// Also returns the number of lines in scope that match
    /// when `with_total` is set, counted past the cap. The scope is
    /// every filter in `options`, so a page taken with
    /// `before_line_id` counts only the lines older than it. Without
    /// `with_total` the scan stops at the cap. An empty pattern matches
    /// every line, so SQL takes the cap and counts the lines instead of
    /// the regex reading each one.
    pub fn search_page(
        &self,
        pattern: &str,
        options: &SearchOptions,
        with_total: bool,
    ) -> Result<SearchPage> {
        let regex: Regex = RegexBuilder::new(pattern)
            .case_insensitive(!options.case_sensitive)
            .build()?;
        let match_all = pattern.is_empty();

        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<i64> = Vec::new();
        if let Some(sid) = options.session_id {
            params.push(sid);
            conditions.push(format!("l.session_id = ?{}", params.len()));
        }
        if let Some(before) = options.before_line_id {
            params.push(before);
            conditions.push(format!("l.id < ?{}", params.len()));
        }
        if options.hide_local {
            conditions.push(not_local_sql());
        }
        let filter = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let cap = if options.max_results == 0 {
            usize::MAX
        } else {
            options.max_results
        };
        let limit = if match_all && cap != usize::MAX {
            format!("LIMIT {cap}")
        } else {
            String::new()
        };
        let sql = format!(
            "SELECT l.id, l.session_id, l.ts_ms, l.text, l.raw, s.host, s.port
             FROM log_lines l
             JOIN sessions s ON s.id = l.session_id
             {filter}
             ORDER BY l.id DESC
             {limit}"
        );
        // Keep reading past the cap only to count regex matches.
        let count_rows = with_total && !match_all;

        let mut stmt = self.conn.prepare(&sql)?;
        let mut hits = Vec::new();
        let mut matched: u64 = 0;
        let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
        while let Some(row) = rows.next()? {
            if hits.len() >= cap && !count_rows {
                break;
            }
            let text: String = row.get(3)?;
            if !match_all && !regex.is_match(&text) {
                continue;
            }
            matched += 1;
            if hits.len() < cap {
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
        drop(rows);
        hits.reverse();

        let total = if !with_total {
            None
        } else if match_all {
            let count: i64 = self.conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM log_lines l
                     JOIN sessions s ON s.id = l.session_id
                     {filter}"
                ),
                rusqlite::params_from_iter(params.iter()),
                |r| r.get(0),
            )?;
            Some(u64::try_from(count).unwrap_or(0))
        } else {
            Some(matched)
        };
        Ok(SearchPage { hits, total })
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
            max_results: 0,
            session_id: None,
            ..SearchOptions::default()
        };
        let hits = s.search("foo", &opts).unwrap();
        assert_eq!(hits.len(), 1);

        let opts = SearchOptions {
            case_sensitive: false,
            max_results: 2,
            session_id: None,
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
            case_sensitive: false,
            max_results: 3,
            session_id: None,
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
            case_sensitive: false,
            max_results: 0,
            session_id: Some(a),
            ..SearchOptions::default()
        };
        let hits = s.search("foo", &opts).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("from a"));
    }

    #[test]
    fn search_can_hide_local_sessions() {
        let (s, _, _) = store_with_local_sessions();
        let every = s.search("line", &SearchOptions::default()).unwrap();
        assert_eq!(every.len(), 15);
        let opts = SearchOptions {
            hide_local: true,
            ..SearchOptions::default()
        };
        let page = s.search_page("line", &opts, true).unwrap();
        assert_eq!(page.total, Some(6));
        assert!(page.hits.iter().all(|h| !is_local_host(&h.host)));
        // An empty pattern counts through SQL and hides them the same way.
        let page = s.search_page("", &opts, true).unwrap();
        assert_eq!(page.total, Some(6));
        assert_eq!(page.hits.len(), 6);
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
