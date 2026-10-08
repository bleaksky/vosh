//! Keep logs for (D34). Once a day, and when you change it, Vosh deletes
//! each log that ended longer ago than you keep logs, whole, and gives
//! the file's space back a little at a time, so the session's appends
//! wait at most one short step. A file an older build wrote needs one
//! full rebuild first, which holds the session's appends until it ends.
//! A change made while a pass runs waits for that pass, then runs with
//! the span you picked last, so two passes never overlap.

use std::future::Future;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tracing::{info, warn};

use crate::app::state::SharedState;
use crate::logs::SharedLogStore;

/// Free pages one step gives back, 1 MB of 4 KB pages.
const STEP_PAGES: u32 = 256;

/// The pause between two steps, in which the session appends.
const STEP_PAUSE: Duration = Duration::from_millis(50);

/// How often a running Vosh looks for logs past the span again.
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// The one pass a running Vosh makes at a time.
static TIDYING: Tidying = Tidying::new();

/// Whether a pass runs, and whether a call came in meanwhile.
#[derive(Clone, Copy)]
enum Pass {
    Idle,
    Running,
    Again,
}

/// Passes in turn. A call that finds a pass running asks it to go again
/// and returns, so the running pass reads the span once more after it.
struct Tidying(Mutex<Pass>);

impl Tidying {
    const fn new() -> Self {
        Self(Mutex::new(Pass::Idle))
    }

    /// Take the turn, or ask the pass that holds it to go again.
    fn claim(&self) -> bool {
        let mut pass = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let idle = matches!(*pass, Pass::Idle);
        *pass = if idle { Pass::Running } else { Pass::Again };
        idle
    }

    /// After a pass, keep the turn when a call came in meanwhile.
    fn again(&self) -> bool {
        let mut pass = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let again = matches!(*pass, Pass::Again);
        *pass = if again { Pass::Running } else { Pass::Idle };
        again
    }
}

/// What one pass did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Tidied {
    pub deleted: usize,
    pub rebuilt: bool,
}

/// Look for logs past Keep logs for now and once a day after, for as
/// long as Vosh runs.
pub(crate) fn start(state: &SharedState) {
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            run(&state).await;
            tokio::time::sleep(EVERY).await;
        }
    });
}

/// Delete the logs past the span profiles.toml keeps. Does nothing while
/// you keep logs forever. A call during a pass makes that pass run once
/// more after it, with the span read again.
pub(crate) async fn run(state: &SharedState) {
    take_turns(&TIDYING, &state.logs, &state.log_reader, || async {
        state
            .loaded_profile_set()
            .await
            .ok()
            .and_then(|set| set.keep_logs_days())
    })
    .await;
}

/// Pass with the days `span` gives, `None` for forever, for as long as
/// calls keep coming in meanwhile.
async fn take_turns<F, Fut>(
    tidying: &Tidying,
    logs: &SharedLogStore,
    reader: &SharedLogStore,
    mut span: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<u32>>,
{
    if !tidying.claim() {
        return;
    }
    loop {
        if let Some(days) = span().await {
            let cutoff = crate::session::now_ms() - i64::from(days) * 86_400_000;
            match tidy(logs, reader, cutoff).await {
                Ok(t) if t.deleted > 0 || t.rebuilt => {
                    info!(
                        deleted = t.deleted,
                        rebuilt = t.rebuilt,
                        days,
                        "kept logs for the span"
                    );
                }
                Ok(_) => {}
                Err(e) => warn!(error = %e, "could not delete the logs past Keep logs for"),
            }
        }
        if !tidying.again() {
            break;
        }
    }
}

/// Delete each log that ended before `cutoff`, in Unix ms, and give the
/// space back in steps, turning the steps on first when the file has
/// none. Each step takes the writer the way every log write does, so the
/// session's next append waits for it instead of racing it.
pub(crate) async fn tidy(
    logs: &SharedLogStore,
    reader: &SharedLogStore,
    cutoff: i64,
) -> vosh_log::Result<Tidied> {
    let mut done = Tidied::default();
    let expired = {
        let guard = logs.lock().await;
        let Some(store) = guard.as_ref() else {
            return Ok(done);
        };
        store.logs_ended_before(cutoff)?
    };
    if expired.is_empty() {
        return Ok(done);
    }
    {
        let mut writer = logs.lock().await;
        // A rebuild truncates the write ahead log, so no search may hold
        // an old snapshot open meanwhile.
        let _searches_wait = reader.lock().await;
        if let Some(store) = writer.as_mut() {
            if !store.compacts_in_steps()? {
                blocking(|| store.turn_on_compaction())?;
                done.rebuilt = true;
            }
        }
    }
    for id in expired {
        let mut writer = logs.lock().await;
        if let Some(store) = writer.as_mut() {
            blocking(|| store.delete_log(id))?;
            done.deleted += 1;
        }
    }
    loop {
        let more = {
            let mut writer = logs.lock().await;
            match writer.as_mut() {
                Some(store) => blocking(|| store.compact_step(STEP_PAGES))?,
                None => false,
            }
        };
        if !more {
            break;
        }
        tokio::time::sleep(STEP_PAUSE).await;
    }
    Ok(done)
}

/// Run `work` where blocking is allowed: on the multi thread runtime the
/// tasks that share the thread move elsewhere meanwhile.
fn blocking<T>(work: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current().map(|h| h.runtime_flavor()) {
        Ok(tokio::runtime::RuntimeFlavor::MultiThread) => tokio::task::block_in_place(work),
        _ => work(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_pass_deletes_the_logs_that_ended_before_the_cutoff() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let path = dir.path().join("logs.sqlite");
        let logs = SharedLogStore::default();
        let reader = SharedLogStore::default();
        let mut store = vosh_log::LogStore::open(&path).expect("the log");
        let old = store.start_session("h", 1, 0).expect("a log");
        for n in 0..2_000 {
            store
                .append(old, n, "Orla says, 'Meet me at the crossroads.'", None)
                .unwrap();
        }
        store.end_session(old, 2_000).unwrap();
        let recent = store.start_session("h", 1, 10_000).expect("a log");
        store.end_session(recent, 20_000).unwrap();
        let open = store.start_session("h", 1, 1_000).expect("a log");
        *logs.lock().await = Some(store);
        *reader.lock().await = Some(vosh_log::LogStore::open(&path).expect("a reader"));

        // A new file gives space back in steps from the start, so the
        // pass never rebuilds it. vosh-log tests the rebuild.
        let done = tidy(&logs, &reader, 5_000).await.expect("a pass");
        assert_eq!(
            done,
            Tidied {
                deleted: 1,
                rebuilt: false
            }
        );
        let guard = logs.lock().await;
        let left: Vec<i64> = guard
            .as_ref()
            .unwrap()
            .list_sessions(0, &vosh_log::Scope::default())
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(left, vec![recent, open]);
        drop(guard);
        assert_eq!(
            tidy(&logs, &reader, 5_000).await.unwrap(),
            Tidied::default()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_span_picked_during_a_pass_runs_after_it() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let path = dir.path().join("logs.sqlite");
        let logs = SharedLogStore::default();
        let reader = SharedLogStore::default();
        let mut store = vosh_log::LogStore::open(&path).expect("the log");
        let day = 86_400_000;
        let now = crate::session::now_ms();
        let old = store.start_session("h", 1, now - 61 * day).expect("a log");
        store.end_session(old, now - 60 * day).unwrap();
        *logs.lock().await = Some(store);
        *reader.lock().await = Some(vosh_log::LogStore::open(&path).expect("a reader"));

        // The first read of the span gives a year. While that pass holds
        // the turn, you pick 30 days, and that call returns at once.
        let tidying = Tidying::new();
        let picked = std::sync::atomic::AtomicU32::new(365);
        let mut reads = 0;
        take_turns(&tidying, &logs, &reader, || {
            reads += 1;
            let first = reads == 1;
            let (tidying, logs, reader, picked) = (&tidying, &logs, &reader, &picked);
            async move {
                let days = picked.load(std::sync::atomic::Ordering::SeqCst);
                if first {
                    picked.store(30, std::sync::atomic::Ordering::SeqCst);
                    take_turns(tidying, logs, reader, || async {
                        unreachable!("a second pass never overlaps the first")
                    })
                    .await;
                }
                Some(days)
            }
        })
        .await;

        assert_eq!(reads, 2);
        let guard = logs.lock().await;
        let left = guard
            .as_ref()
            .unwrap()
            .list_sessions(0, &vosh_log::Scope::default())
            .unwrap();
        assert!(left.is_empty());
        drop(guard);
        assert!(tidying.claim(), "the turn is free once the passes end");
    }
}
