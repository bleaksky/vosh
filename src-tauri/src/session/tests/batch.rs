//! The frame, the log rows and the name a burst of reads owes.

use super::{PerfCounters, Settle, FRAME_BUDGET};

fn row(session_id: i64, text: &str) -> vosh_log::LogEntry {
    vosh_log::LogEntry {
        session_id,
        ts_ms: 0,
        text: text.to_string(),
        raw: None,
    }
}

#[test]
fn a_burst_that_never_pauses_owes_its_frame_once_the_budget_runs_out() {
    let mut settle = Settle::default();
    assert!(!settle.frame_overdue(), "nothing drew yet");
    settle.drew();
    assert!(!settle.frame_overdue());
    std::thread::sleep(FRAME_BUDGET);
    assert!(settle.frame_overdue());
}

#[test]
fn rows_with_nothing_drawn_come_due_once_the_budget_runs_out() {
    let mut settle = Settle::default();
    settle.queue_rows(Vec::new());
    assert!(!settle.log_overdue(), "no rows wait yet");
    settle.queue_rows([row(1, "> east")]);
    assert!(!settle.log_overdue());
    std::thread::sleep(FRAME_BUDGET);
    assert!(settle.log_overdue(), "the row waited out the budget");
    assert!(!settle.frame_overdue(), "nothing drew");
    settle.write_log(None, &mut PerfCounters::default());
    assert!(!settle.log_overdue(), "the clock stops with the write");
    settle.queue_rows([row(1, "> west")]);
    assert!(!settle.log_overdue(), "a new row starts a new clock");
}

#[test]
fn the_waiting_rows_go_in_once_in_the_order_they_came() {
    let mut store = vosh_log::LogStore::in_memory().expect("a log");
    let id = store.start_session("h", 1, 0).expect("a session");
    let mut settle = Settle::default();
    settle.queue_rows([row(id, "a room"), row(id, "> east")]);
    settle.queue_rows([row(id, "the next room")]);
    settle.write_log(Some(&mut store), &mut PerfCounters::default());
    assert!(settle.log.is_empty());
    settle.write_log(Some(&mut store), &mut PerfCounters::default());
    assert_eq!(
        store.export_session(id, false).expect("the rows"),
        "a room\n> east\nthe next room\n"
    );
}

#[test]
fn the_character_waits_with_the_rows_and_goes_in_with_them() {
    let mut store = vosh_log::LogStore::in_memory().expect("a log");
    let id = store.start_session("h", 1, 0).expect("a session");
    let mut settle = Settle::default();
    assert!(!settle.owes_log());
    settle.queue_name((id, "Orla".to_string()));
    assert!(settle.owes_log(), "a name alone is owed to the log");
    assert!(!settle.log_overdue());
    std::thread::sleep(FRAME_BUDGET);
    assert!(settle.log_overdue(), "the name waited out the budget");
    settle.write_log(Some(&mut store), &mut PerfCounters::default());
    assert!(!settle.owes_log());
    assert_eq!(
        store.session_character(id).expect("the row").as_deref(),
        Some("Orla")
    );
}
