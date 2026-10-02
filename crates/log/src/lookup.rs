//! Reads the lines of the sessions that belong to a profile's characters,
//! newest first.

use rusqlite::{params, OptionalExtension};

use crate::{LogStore, Result};

/// How many sessions a character lookup reads at most, newest first.
const LOOKUP_SESSIONS: usize = 50;

/// The sessions a character lookup reads: those on one host and port
/// that belong to one of the profile's characters.
#[derive(Debug, Clone, Copy)]
pub struct CharacterScope<'a> {
    pub host: &'a str,
    pub port: u16,
    /// The characters the profile claims.
    pub mine: &'a [String],
    /// Every character any profile claims on this host and port.
    pub claimed: &'a [String],
    /// A profile plays this host and port with no character named, so a
    /// session may belong to a character no profile claims.
    pub open: bool,
}

/// A session a character lookup read, with the character it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedSession {
    pub id: i64,
    pub character: String,
}

/// A line of a session a character lookup read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedLine {
    pub id: i64,
    pub ts_ms: i64,
    pub text: String,
    pub raw: Option<Vec<u8>>,
}

impl LogStore {
    /// Walk the sessions on `scope`'s host and port, newest first, at most
    /// `LOOKUP_SESSIONS`, and hand each one that belongs to one of
    /// `scope.mine` its lines that start with one of `prefixes`, newest
    /// first, to `pick`, until `pick` returns something.
    ///
    /// A session belongs to the character the game named for it. An older
    /// session with none belongs to the character its first sent line
    /// names, when that is a character `scope.claimed` holds, and
    /// otherwise to the one character claimed on the host and port when
    /// there is exactly one and `scope.open` is false. The query compares sent lines against those
    /// names only and reads no other sent line. A prefix must not start
    /// with the `> ` of a sent line.
    pub fn find_in_sessions<T>(
        &self,
        scope: &CharacterScope,
        prefixes: &[&str],
        mut pick: impl FnMut(&ScopedSession, &[ScopedLine]) -> Option<T>,
    ) -> Result<Option<T>> {
        let mine: Vec<String> = scope.mine.iter().map(|c| fold(c)).collect();
        if mine.is_empty() || prefixes.is_empty() {
            return Ok(None);
        }
        let mut sessions = self.conn.prepare(
            "SELECT id, character FROM sessions
             WHERE lower(trim(host)) = lower(trim(?1)) AND port = ?2
             ORDER BY started_at_ms DESC, id DESC
             LIMIT ?3",
        )?;
        let rows: Vec<(i64, Option<String>)> = sessions
            .query_map(
                params![scope.host, scope.port, LOOKUP_SESSIONS as i64],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
            .collect::<std::result::Result<_, _>>()?;
        for (id, character) in rows {
            let Some(owner) = self.session_owner(id, character, scope)? else {
                continue;
            };
            if !mine.contains(&fold(&owner)) {
                continue;
            }
            let lines = self.lines_starting(id, prefixes)?;
            if lines.is_empty() {
                continue;
            }
            let session = ScopedSession {
                id,
                character: owner,
            };
            if let Some(found) = pick(&session, &lines) {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    /// The character a session belongs to, as [`Self::find_in_sessions`]
    /// decides it.
    fn session_owner(
        &self,
        session_id: i64,
        character: Option<String>,
        scope: &CharacterScope,
    ) -> Result<Option<String>> {
        if let Some(character) = character.filter(|c| !c.trim().is_empty()) {
            return Ok(Some(character));
        }
        let mut claimed: Vec<String> = scope.claimed.iter().map(|c| fold(c)).collect();
        claimed.sort();
        claimed.dedup();
        claimed.retain(|c| !c.is_empty());
        if claimed.is_empty() {
            return Ok(None);
        }
        // The login line: `> Name`, compared in any case against the
        // claimed names alone.
        let slots: Vec<String> = (2..=claimed.len() + 1).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "SELECT substr(text, 3) FROM log_lines
             WHERE session_id = ?1 AND lower(text) IN ({})
             ORDER BY id LIMIT 1",
            slots.join(", ")
        );
        let mut values: Vec<rusqlite::types::Value> = vec![session_id.into()];
        values.extend(claimed.iter().map(|c| format!("> {c}").into()));
        let named: Option<String> = self
            .conn
            .query_row(&sql, rusqlite::params_from_iter(values), |row| row.get(0))
            .optional()?;
        if let Some(name) = named {
            return Ok(Some(name));
        }
        // With one character claimed here and no profile that plays by
        // host alone, the session was theirs.
        Ok((claimed.len() == 1 && !scope.open).then(|| claimed.remove(0)))
    }

    /// A session's lines that start with one of `prefixes`, newest first.
    fn lines_starting(&self, session_id: i64, prefixes: &[&str]) -> Result<Vec<ScopedLine>> {
        let tests: Vec<String> = (0..prefixes.len())
            .map(|i| format!("substr(text, 1, length(?{0})) = ?{0}", i + 2))
            .collect();
        let sql = format!(
            "SELECT id, ts_ms, text, raw FROM log_lines
             WHERE session_id = ?1 AND ({})
             ORDER BY ts_ms DESC, id DESC",
            tests.join(" OR ")
        );
        let mut values: Vec<rusqlite::types::Value> = vec![session_id.into()];
        values.extend(prefixes.iter().map(|p| (*p).to_string().into()));
        let mut stmt = self.conn.prepare(&sql)?;
        let lines = stmt
            .query_map(rusqlite::params_from_iter(values), |row| {
                Ok(ScopedLine {
                    id: row.get(0)?,
                    ts_ms: row.get(1)?,
                    text: row.get(2)?,
                    raw: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(lines)
    }
}

/// A character name folded for comparing in any case.
fn fold(name: &str) -> String {
    name.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::tests::store;

    const REPLIES: [&str; 2] = ["Current prompt: ", "Prompt set to "];

    /// A session on the game with `lines` in order, each a sent line when
    /// it starts with `> `. `character` is what Char.Status named.
    fn session_with(
        s: &mut LogStore,
        started: i64,
        character: Option<&str>,
        lines: &[&str],
    ) -> i64 {
        let id = s.start_session("play.example.com", 1848, started).unwrap();
        if let Some(name) = character {
            s.set_session_character(id, name).unwrap();
        }
        for (n, line) in lines.iter().enumerate() {
            s.append(id, started + n as i64, line, None).unwrap();
        }
        id
    }

    fn scope<'a>(mine: &'a [String], claimed: &'a [String]) -> CharacterScope<'a> {
        CharacterScope {
            host: "PLAY.example.com ",
            port: 1848,
            mine,
            claimed,
            open: false,
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| (*n).to_string()).collect()
    }

    /// The newest reply line the lookup hands over, with its session.
    fn newest(s: &LogStore, scope: &CharacterScope) -> Option<(i64, String, String)> {
        s.find_in_sessions(scope, &REPLIES, |session, lines| {
            Some((session.id, session.character.clone(), lines[0].text.clone()))
        })
        .unwrap()
    }

    #[test]
    fn a_lookup_finds_the_newest_reply_of_this_profiles_character() {
        let mut s = store();
        let tester = session_with(
            &mut s,
            100,
            Some("Tester"),
            &["> prompt", "Current prompt: %h ", "Prompt set to %h %m "],
        );
        // A later session of another character on the same game.
        session_with(&mut s, 200, Some("Healer"), &["Current prompt: <%h> "]);
        let claimed = names(&["Tester", "Healer"]);
        let mine = names(&["TESTER"]);
        assert_eq!(
            newest(&s, &scope(&mine, &claimed)),
            Some((tester, "Tester".into(), "Prompt set to %h %m ".into()))
        );
        let mine = names(&["healer"]);
        assert_eq!(
            newest(&s, &scope(&mine, &claimed)).map(|h| h.2),
            Some("Current prompt: <%h> ".into())
        );
        // A profile that claims no one reads nothing.
        assert_eq!(newest(&s, &scope(&[], &claimed)), None);
        // Another port is another game.
        let other = CharacterScope {
            port: 4000,
            ..scope(&mine, &claimed)
        };
        assert_eq!(newest(&s, &other), None);
    }

    #[test]
    fn an_older_session_belongs_to_the_character_its_login_line_names() {
        let mut s = store();
        let older = session_with(
            &mut s,
            100,
            None,
            &[
                "By what name do you wish to be known?",
                "> tester",
                "> hunter2",
                "Prompt set to %h ",
            ],
        );
        let claimed = names(&["Tester", "Healer"]);
        let mine = names(&["Tester"]);
        assert_eq!(
            newest(&s, &scope(&mine, &claimed)),
            Some((older, "tester".into(), "Prompt set to %h ".into()))
        );
        assert_eq!(newest(&s, &scope(&names(&["Healer"]), &claimed)), None);
    }

    #[test]
    fn an_unnamed_session_counts_only_when_one_character_is_claimed() {
        let mut s = store();
        session_with(&mut s, 100, None, &["> look", "Current prompt: %h "]);
        let mine = names(&["Tester"]);
        assert_eq!(
            newest(&s, &scope(&mine, &names(&["Tester"]))).map(|h| h.1),
            Some("tester".into())
        );
        // Two characters on the game, and nothing says whose it was.
        assert_eq!(
            newest(&s, &scope(&mine, &names(&["Tester", "Healer"]))),
            None
        );
        // A profile that plays the game with no character named could have
        // played it as anyone.
        let claimed = names(&["Tester"]);
        let open = CharacterScope {
            open: true,
            ..scope(&mine, &claimed)
        };
        assert_eq!(newest(&s, &open), None);
    }

    #[test]
    fn a_lookup_reads_no_sent_line_but_a_claimed_name() {
        let mut s = store();
        let id = session_with(
            &mut s,
            100,
            None,
            &[
                "> Current prompt: typed by you",
                "> secret",
                "Current prompt: %h ",
            ],
        );
        let mine = names(&["Tester"]);
        let found = s
            .find_in_sessions(
                &scope(&mine, &names(&["Tester"])),
                &REPLIES,
                |session, lines| {
                    assert_eq!(session.id, id);
                    Some(lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>())
                },
            )
            .unwrap();
        assert_eq!(found, Some(vec!["Current prompt: %h ".to_string()]));
    }

    #[test]
    fn a_lookup_stops_at_the_first_session_pick_takes_and_reads_at_most_fifty() {
        let mut s = store();
        let old = session_with(&mut s, 1, Some("Tester"), &["Current prompt: old "]);
        for n in 0..LOOKUP_SESSIONS as i64 {
            session_with(&mut s, 10 + n, Some("Tester"), &["You are hungry."]);
        }
        let mine = names(&["Tester"]);
        let claimed = names(&["Tester"]);
        assert_eq!(
            newest(&s, &scope(&mine, &claimed)),
            None,
            "fifty newer sessions"
        );
        let mut s = store();
        let first = session_with(&mut s, 1, Some("Tester"), &["Current prompt: first "]);
        session_with(&mut s, 2, Some("Tester"), &["Prompt set to skipped "]);
        let mut offered = Vec::new();
        let found = s
            .find_in_sessions(&scope(&mine, &claimed), &REPLIES, |session, lines| {
                offered.push(session.id);
                lines[0].text.contains("first").then_some(session.id)
            })
            .unwrap();
        assert_eq!(found, Some(first));
        assert_eq!(offered.len(), 2);
        let _ = old;
    }
}
