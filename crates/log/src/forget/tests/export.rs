//! A saved log leaves out every line forget passwords would blank.

use std::collections::HashSet;

use super::wipe::{log_session, populated, TempLog};
use super::*;
use crate::{LogStore, Scope, HIDDEN_SENT_TEXT};

const SECRETS: [&str; 4] = [SECRET_ACCOUNT, SECRET_IMM, SECRET_OLD, SECRET_NEW];

/// The lines `scope` saves, with colors or without, and with the
/// password lines hidden or not.
fn saved(store: &LogStore, scope: &Scope, with_ansi: bool, hide: bool) -> Vec<u8> {
    let mut out = Vec::new();
    store
        .export_scope(scope, with_ansi, hide, &mut out)
        .unwrap();
    out
}

/// Indexes of the made up secrets somewhere in `bytes`.
fn secrets_in(bytes: &[u8]) -> Vec<usize> {
    (0..SECRETS.len())
        .filter(|&n| {
            bytes
                .windows(SECRETS[n].len())
                .any(|w| w == SECRETS[n].as_bytes())
        })
        .collect()
}

/// Line numbers where `hidden` differs from `shown`, each of which must
/// read as a blanked line. Both must hold the same number of lines.
fn hidden_lines(shown: &[u8], hidden: &[u8]) -> Vec<usize> {
    let shown: Vec<&[u8]> = shown.split(|b| *b == b'\n').collect();
    let hidden: Vec<&[u8]> = hidden.split(|b| *b == b'\n').collect();
    assert_eq!(shown.len(), hidden.len(), "lines came or went");
    let mut changed = Vec::new();
    for (n, (a, b)) in shown.iter().zip(&hidden).enumerate() {
        if a != b {
            assert!(
                *b == HIDDEN_SENT_TEXT.as_bytes(),
                "line {n} changed into something other than a hidden line"
            );
            changed.push(n);
        }
    }
    changed
}

/// The ids of the lines `scope` saves, in the order it saves them.
fn saved_ids(store: &LogStore, scope: &Scope) -> Vec<i64> {
    let logs = store.scoped_logs(scope).unwrap();
    let since = scope.since_ms.unwrap_or(i64::MIN);
    let mut stmt = store
        .conn
        .prepare("SELECT id, session_id, ts_ms FROM log_lines ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })
    .unwrap()
    .map(Result::unwrap)
    .filter(|(_, sid, ts)| logs.contains_key(sid) && *ts >= since)
    .map(|(id, _, _)| id)
    .collect()
}

#[test]
fn a_save_holds_no_password_and_every_other_line_as_it_was() {
    let log = TempLog::new("save");
    let (store, _) = populated(&log);
    let all = Scope::default();
    for with_ansi in [false, true] {
        let shown = saved(&store, &all, with_ansi, false);
        assert_eq!(
            secrets_in(&shown),
            vec![0, 1, 2, 3],
            "the test means something only if the secrets are in the log"
        );
        let hidden = saved(&store, &all, with_ansi, true);
        assert_eq!(
            secrets_in(&hidden),
            Vec::<usize>::new(),
            "a made up secret reached the saved file"
        );
        assert_eq!(hidden_lines(&shown, &hidden).len(), 6);
    }
}

#[test]
fn a_save_hides_the_lines_forget_passwords_finds() {
    let log = TempLog::new("same-lines");
    let (store, sessions) = populated(&log);
    let found: Vec<i64> = store
        .find_password_lines()
        .unwrap()
        .lines
        .iter()
        .map(|(id, _)| *id)
        .collect();
    let scopes = [
        Scope::default(),
        Scope::log(sessions[0]),
        Scope::log(sessions[1]),
        Scope::log(sessions[2]),
    ];
    for (n, scope) in scopes.iter().enumerate() {
        let ids = saved_ids(&store, scope);
        let lines = hidden_lines(
            &saved(&store, scope, false, false),
            &saved(&store, scope, false, true),
        );
        let hidden: Vec<i64> = lines.iter().map(|&l| ids[l]).collect();
        let due: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|id| found.contains(id))
            .collect();
        assert_eq!(hidden, due, "scope {n} hid other lines");
    }
}

#[test]
fn a_span_that_starts_after_the_prompt_still_hides_the_password() {
    let log = TempLog::new("since");
    let (store, sessions) = populated(&log);
    // log_session times row n at n. The account password is row 12,
    // right after the empty row the `Password> ` prompt leaves.
    let rows = session(&[&greeting(), &login(SECRET_ACCOUNT)]);
    let at = rows.len() as i64 - 1;
    let scope = Scope {
        logs: Some(vec![sessions[0]]),
        since_ms: Some(at),
        ..Scope::default()
    };
    let shown = saved(&store, &scope, false, false);
    assert!(
        shown.starts_with(format!("> {SECRET_ACCOUNT}\n").as_bytes()),
        "the span does not start at the password"
    );
    let hidden = saved(&store, &scope, false, true);
    assert!(hidden.starts_with(format!("{HIDDEN_SENT_TEXT}\n").as_bytes()));
    assert_eq!(secrets_in(&hidden), Vec::<usize>::new());
}

#[test]
fn a_log_left_out_of_the_scope_is_neither_saved_nor_counted() {
    let mut store = LogStore::in_memory().unwrap();
    let a = store
        .start_session("play.theforsakenlands.com", 1848, 0)
        .unwrap();
    let b = store
        .start_session("play.theforsakenlands.com", 1848, 0)
        .unwrap();
    let first = session(&[&greeting(), &login(SECRET_ACCOUNT)]);
    let second = session(&[&greeting(), &login(SECRET_OLD)]);
    // The two logs take turns, row by row.
    let mut ids = vec![];
    for (n, (x, y)) in first.iter().zip(&second).enumerate() {
        for (sid, row) in [(a, x), (b, y)] {
            let id = match row {
                Out(text) => store
                    .append_raw(sid, n as i64, format!("\x1b[0m{text}").as_bytes())
                    .unwrap(),
                Sent(line) | Also(line) => store
                    .append(sid, n as i64, &format!("> {line}"), None)
                    .unwrap(),
            };
            ids.push(id);
        }
    }
    let scope = Scope::log(a);
    let logs = store.scoped_logs(&scope).unwrap();
    let password = ids[2 * (first.len() - 1)];
    assert_eq!(
        store.password_lines_in(&logs).unwrap(),
        HashSet::from([password])
    );
    let hidden = saved(&store, &scope, true, true);
    assert_eq!(secrets_in(&hidden), Vec::<usize>::new());
    let count = hidden
        .split(|b| *b == b'\n')
        .filter(|l| *l == HIDDEN_SENT_TEXT.as_bytes())
        .count();
    assert_eq!(count, 1);
}

#[test]
fn a_save_after_forgetting_reads_the_same() {
    let log = TempLog::new("after");
    let (mut store, _) = populated(&log);
    let all = Scope::default();
    let before = [
        saved(&store, &all, false, true),
        saved(&store, &all, true, true),
    ];
    store.forget_passwords().unwrap();
    for (n, with_ansi) in [false, true].into_iter().enumerate() {
        assert!(
            saved(&store, &all, with_ansi, true) == before[n],
            "a save changed once the passwords were forgotten"
        );
        assert!(
            saved(&store, &all, with_ansi, false) == before[n],
            "the forgotten log reads differently from a save"
        );
    }
}

#[test]
fn a_log_with_no_password_saves_as_it_was() {
    let mut store = LogStore::in_memory().unwrap();
    let rows = [Out("The Temple Square"), Sent("look"), Sent("north")];
    let sid = log_session(&mut store, &rows, 3);
    let scope = Scope::log(sid);
    for with_ansi in [false, true] {
        assert_eq!(
            saved(&store, &scope, with_ansi, true),
            saved(&store, &scope, with_ansi, false)
        );
    }
}
