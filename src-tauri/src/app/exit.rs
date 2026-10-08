//! Quitting with writes still waiting in a window.
//!
//! Settings holds a change back for a moment: the autosave waits out a
//! short pause, a pane width waits out a drag, and a number or color
//! field saves when you leave it. Quitting used to write the profile at
//! once, so the change you made last never reached the disk. Now an
//! exit request first asks every open window to send what it holds
//! (`vosh://flush-pending-writes`), waits a short, bounded time for each
//! to answer through the `pending_writes_flushed` command, and exits
//! again. That second request writes the profile once, as before.
//!
//! [`ExitFlow`] decides what each exit event does, so the profile write
//! runs exactly once however the events arrive. macOS can end the app
//! with `Exit` alone (a quit from the Dock or at log out), and then
//! there is no time to ask the windows, so the write runs at once.
//!
//! [`on_run_event`] takes both exit events from the app's run loop and
//! does what the flow says.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;
use tracing::{info, warn};

use crate::app::events::{broadcast, FLUSH_PENDING_WRITES};
use crate::app::state::SharedState;

/// How long quit waits for the windows. A window gives up on its own
/// writes a little sooner (`FLUSH_TIMEOUT_MS` in pendingWrites.ts), so
/// a slow write still gets its answer in.
pub(crate) const WINDOW_FLUSH_WAIT: Duration = Duration::from_millis(1200);

/// Whether quit asks the window `label` for the writes it holds. Help
/// only reads, so it holds none, and quit never waits on it.
pub(crate) fn holds_writes(label: &str) -> bool {
    label != "help"
}

/// What an exit event asks the run loop to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitStep {
    /// Hold the exit, ask the open windows for their writes, and exit
    /// again once they answer or the wait runs out.
    AskWindows,
    /// The windows are being asked. Hold this exit too, since the ask
    /// exits again when it ends.
    Hold,
    /// Write the profile now and let the app exit.
    Flush,
    /// The profile was written already. Let the app exit.
    Done,
}

/// Where quitting stands. One per process.
#[derive(Debug, Default)]
pub(crate) struct ExitFlow {
    asked: bool,
    asking: bool,
    flushed: bool,
}

impl ExitFlow {
    pub(crate) const fn new() -> Self {
        Self {
            asked: false,
            asking: false,
            flushed: false,
        }
    }

    /// An exit request. `can_hold` is false for a restart, which Tauri
    /// does not let the app hold. `windows` counts the open windows.
    pub(crate) fn exit_requested(&mut self, can_hold: bool, windows: usize) -> ExitStep {
        if self.flushed {
            return ExitStep::Done;
        }
        if can_hold {
            if self.asking {
                return ExitStep::Hold;
            }
            if !self.asked && windows > 0 {
                self.asked = true;
                self.asking = true;
                return ExitStep::AskWindows;
            }
        }
        self.flushed = true;
        ExitStep::Flush
    }

    /// The windows answered, or the wait ran out.
    pub(crate) fn windows_answered(&mut self) {
        self.asking = false;
    }

    /// The run loop is ending.
    pub(crate) fn exit(&mut self) -> ExitStep {
        if self.flushed {
            return ExitStep::Done;
        }
        self.flushed = true;
        ExitStep::Flush
    }
}

static EXIT_FLOW: Mutex<ExitFlow> = Mutex::new(ExitFlow::new());

/// [`ExitFlow::exit_requested`] on the process's flow.
pub(crate) fn exit_requested(can_hold: bool, windows: usize) -> ExitStep {
    EXIT_FLOW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .exit_requested(can_hold, windows)
}

/// [`ExitFlow::exit`] on the process's flow.
pub(crate) fn exit() -> ExitStep {
    EXIT_FLOW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .exit()
}

fn windows_answered() {
    EXIT_FLOW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .windows_answered();
}

/// The windows asked in one round that have not answered yet.
#[derive(Debug, Default)]
pub(crate) struct WindowAnswers {
    round: Mutex<Option<Round>>,
}

#[derive(Debug)]
struct Round {
    id: u64,
    waiting: BTreeSet<String>,
    done: Option<oneshot::Sender<()>>,
}

impl WindowAnswers {
    pub(crate) const fn new() -> Self {
        Self {
            round: Mutex::new(None),
        }
    }

    /// Start round `id` over the windows `labels`. The receiver resolves
    /// once every one of them has answered, at once for no windows.
    pub(crate) fn start(
        &self,
        id: u64,
        labels: impl IntoIterator<Item = String>,
    ) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let waiting: BTreeSet<String> = labels.into_iter().collect();
        let mut round = self.round.lock().unwrap_or_else(PoisonError::into_inner);
        if waiting.is_empty() {
            let _ = tx.send(());
            *round = None;
        } else {
            *round = Some(Round {
                id,
                waiting,
                done: Some(tx),
            });
        }
        rx
    }

    /// The window `label` sent what it held. An answer from a window
    /// the round did not ask, or after the round ended, changes nothing.
    pub(crate) fn answer(&self, label: &str) {
        let mut round = self.round.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(current) = round.as_mut() else {
            return;
        };
        current.waiting.remove(label);
        if current.waiting.is_empty() {
            if let Some(tx) = current.done.take() {
                let _ = tx.send(());
            }
            *round = None;
        }
    }

    /// End round `id`, answered or not.
    pub(crate) fn finish(&self, id: u64) {
        let mut round = self.round.lock().unwrap_or_else(PoisonError::into_inner);
        if round.as_ref().is_some_and(|r| r.id == id) {
            *round = None;
        }
    }
}

pub(crate) static ANSWERS: WindowAnswers = WindowAnswers::new();

/// Wait for `rx` up to `wait`. True when every window answered in time.
pub(crate) async fn wait_for_answers(rx: oneshot::Receiver<()>, wait: Duration) -> bool {
    matches!(tokio::time::timeout(wait, rx).await, Ok(Ok(())))
}

/// Ask every open window to send the writes it holds, and wait for the
/// answers up to [`WINDOW_FLUSH_WAIT`]. Then mark the flow answered, so
/// the next exit request writes the profile.
pub(crate) async fn ask_windows_to_flush<R: tauri::Runtime>(app: &AppHandle<R>) {
    static ROUND: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = ROUND.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
    let labels: Vec<String> = app
        .webview_windows()
        .into_keys()
        .filter(|label| holds_writes(label))
        .collect();
    let rx = ANSWERS.start(id, labels.iter().cloned());
    broadcast(app, FLUSH_PENDING_WRITES, &id);
    if wait_for_answers(rx, WINDOW_FLUSH_WAIT).await {
        info!(
            windows = labels.len(),
            "quit: every window sent its pending writes"
        );
    } else {
        warn!(
            wait_ms = WINDOW_FLUSH_WAIT.as_millis(),
            "quit: a window did not answer in time, writing the profile anyway"
        );
    }
    ANSWERS.finish(id);
    windows_answered();
}

/// The app's run loop hands every event here. Only the two exit events
/// do anything.
pub(crate) fn on_run_event(app_handle: &AppHandle, event: tauri::RunEvent) {
    // Backstop flush: slash-command and Lua edits ride a debounced
    // persist that may not have fired when the user quits (Cmd+Q,
    // window close). Write the profile out before the process
    // ends so nothing authored this session is lost. Before that
    // write, an exit request asks the open windows for the edits
    // they hold back (the Settings autosave, a pane width, the
    // field you are typing in) and waits a short, bounded time
    // for them (ask_windows_to_flush). Matches both exit events because
    // macOS quit paths that go through NSApplication terminate
    // can deliver Exit without a preceding ExitRequested. The
    // exit flow keeps the write to exactly once when both arrive.
    match event {
        tauri::RunEvent::ExitRequested { code, api, .. } => {
            // Tauri does not let a restart be held.
            let can_hold = code != Some(tauri::RESTART_EXIT_CODE);
            let windows = app_handle
                .webview_windows()
                .into_keys()
                .filter(|label| holds_writes(label))
                .count();
            match exit_requested(can_hold, windows) {
                ExitStep::AskWindows => {
                    api.prevent_exit();
                    let app = app_handle.clone();
                    let code = code.unwrap_or(0);
                    tauri::async_runtime::spawn(async move {
                        ask_windows_to_flush(&app).await;
                        app.exit(code);
                    });
                }
                ExitStep::Hold => api.prevent_exit(),
                ExitStep::Flush => flush_profile_on_exit(app_handle),
                ExitStep::Done => {}
            }
        }
        tauri::RunEvent::Exit => {
            let step = exit();
            if step == ExitStep::Flush {
                flush_profile_on_exit(app_handle);
            }
        }
        _ => {}
    }
}

/// Write every open profile once on the way out. [`ExitFlow`] decides
/// when, so this runs exactly once.
fn flush_profile_on_exit(app_handle: &AppHandle) {
    let state: SharedState = app_handle.state::<SharedState>().inner().clone();
    // The affect fulls are a cache of their own, written whatever
    // becomes of the profiles.
    for session in state.all_sessions() {
        session.affect_full.flush(&state.affect_file);
    }
    // The open logs and the scrollback, bounded as the profiles are.
    let quit = tauri::async_runtime::block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            crate::logs::on_quit(&state),
        )
        .await
    });
    if quit.is_err() {
        tracing::warn!("exit: the logs and scrollback timed out after 3s");
    }
    // Honor a #profile reset/load: a profile it left deliberately
    // diverged from disk is not written back.
    let (held, saved): (Vec<_>, Vec<_>) = state
        .open_profiles()
        .into_iter()
        .partition(|open| open.held());
    if !held.is_empty() {
        info!(
            held = held.len(),
            "exit flush: skipped the profiles a profile reset or load holds"
        );
    }
    info!(profiles = saved.len(), "exit flush: persisting profiles");
    // Bounded: a wedged Lua trigger holding a profile lock must
    // not turn quit into a hang. The timeout cuts the lock waits
    // for every profile together; the file writes themselves are
    // sync and small.
    let flush = async {
        for open in &saved {
            crate::disk::save::persist_profile(&state, open).await;
        }
    };
    let outcome = tauri::async_runtime::block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(3), flush).await
    });
    match outcome {
        Ok(()) => info!("exit flush: done"),
        Err(_) => {
            tracing::warn!("exit flush: timed out after 3s, exiting without it");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quit_asks_the_windows_first_and_writes_the_profile_once() {
        let mut flow = ExitFlow::new();
        // Quit from the menu, with the main window and Settings open.
        assert_eq!(flow.exit_requested(true, 2), ExitStep::AskWindows);
        // A second quit while the windows answer waits for the first.
        assert_eq!(flow.exit_requested(true, 2), ExitStep::Hold);
        flow.windows_answered();
        // The ask exits again, and that request writes the profile.
        assert_eq!(flow.exit_requested(true, 2), ExitStep::Flush);
        // The run loop then ends without a second write.
        assert_eq!(flow.exit(), ExitStep::Done);
        assert_eq!(flow.exit_requested(true, 2), ExitStep::Done);
    }

    #[test]
    fn exit_alone_writes_the_profile_once() {
        // macOS ends the app from the Dock with Exit and no request.
        let mut flow = ExitFlow::new();
        assert_eq!(flow.exit(), ExitStep::Flush);
        assert_eq!(flow.exit(), ExitStep::Done);
    }

    #[test]
    fn exit_while_the_windows_answer_writes_the_profile_at_once() {
        let mut flow = ExitFlow::new();
        assert_eq!(flow.exit_requested(true, 1), ExitStep::AskWindows);
        assert_eq!(flow.exit(), ExitStep::Flush);
        flow.windows_answered();
        assert_eq!(flow.exit_requested(true, 1), ExitStep::Done);
    }

    #[test]
    fn nothing_to_ask_writes_the_profile_at_once() {
        // The last window closed, so no window is left to ask.
        let mut flow = ExitFlow::new();
        assert_eq!(flow.exit_requested(true, 0), ExitStep::Flush);
        assert_eq!(flow.exit(), ExitStep::Done);
        // A restart cannot be held, so it never waits on the windows.
        let mut flow = ExitFlow::new();
        assert_eq!(flow.exit_requested(false, 2), ExitStep::Flush);
    }

    #[tokio::test]
    async fn the_wait_ends_when_every_window_answers() {
        let answers = WindowAnswers::new();
        let rx = answers.start(1, ["main".to_string(), "settings".to_string()]);
        answers.answer("settings");
        // A window the round did not ask changes nothing.
        answers.answer("help");
        answers.answer("settings");
        answers.answer("main");
        assert!(wait_for_answers(rx, Duration::from_secs(5)).await);
    }

    #[tokio::test]
    async fn the_wait_is_bounded_when_a_window_never_answers() {
        let answers = WindowAnswers::new();
        let rx = answers.start(1, ["main".to_string(), "settings".to_string()]);
        answers.answer("main");
        assert!(!wait_for_answers(rx, Duration::from_millis(20)).await);
        answers.finish(1);
        // A late answer after the round ended is ignored.
        answers.answer("settings");
    }

    #[test]
    fn quit_asks_every_window_but_help() {
        assert!(holds_writes("main"));
        assert!(holds_writes("settings"));
        assert!(!holds_writes("help"));
    }

    #[tokio::test]
    async fn no_windows_needs_no_wait() {
        let answers = WindowAnswers::new();
        let rx = answers.start(1, Vec::<String>::new());
        assert!(wait_for_answers(rx, Duration::from_millis(1)).await);
    }
}
