//! Quitting with writes still waiting in a window.
//!
//! Settings holds a change back for a moment: the autosave waits out a
//! short pause, a pane width waits out a drag, and a number or color
//! field saves when you leave it. Quitting used to write the profile at
//! once, so the change you made last never reached the disk. Now an
//! exit request first asks every open window to send what it holds
//! (`vosh://flush-pending-writes`), waits a short, bounded time for each
//! to answer through [`pending_writes_flushed`], and exits again. That
//! second request writes the profile once, as before.
//!
//! [`ExitFlow`] decides what each exit event does, so the profile write
//! runs exactly once however the events arrive. macOS can end the app
//! with `Exit` alone (a quit from the Dock or at log out), and then
//! there is no time to ask the windows, so the write runs at once.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tauri::AppHandle;
use tokio::sync::oneshot;
use tracing::{info, warn};

use crate::app::events::{broadcast, FLUSH_REQUEST_EVENT};

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

static ANSWERS: WindowAnswers = WindowAnswers::new();

/// Wait for `rx` up to `wait`. True when every window answered in time.
pub(crate) async fn wait_for_answers(rx: oneshot::Receiver<()>, wait: Duration) -> bool {
    matches!(tokio::time::timeout(wait, rx).await, Ok(Ok(())))
}

/// Ask every open window to send the writes it holds, and wait for the
/// answers up to [`WINDOW_FLUSH_WAIT`]. Then mark the flow answered, so
/// the next exit request writes the profile.
pub(crate) async fn ask_windows_to_flush<R: tauri::Runtime>(app: &AppHandle<R>) {
    use tauri::Manager;
    static ROUND: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = ROUND.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
    let labels: Vec<String> = app
        .webview_windows()
        .into_keys()
        .filter(|label| holds_writes(label))
        .collect();
    let rx = ANSWERS.start(id, labels.iter().cloned());
    broadcast(app, FLUSH_REQUEST_EVENT, &id);
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

/// A window answers the quit request once it has sent what it held.
#[tauri::command]
pub(crate) fn pending_writes_flushed<R: tauri::Runtime>(window: tauri::WebviewWindow<R>) {
    ANSWERS.answer(window.label());
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
