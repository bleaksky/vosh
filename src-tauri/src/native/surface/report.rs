//! What a frame reports, each only when it changes. The page hears the
//! grid size and the scroll offset, and the game hears its size through
//! the session. `grid_and_game_rows` is the Rust twin of `keptRows` and
//! `gameSize` in src/lib/terminalRows.ts, and both run
//! fixtures/terminal-rows/cases.json.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use tauri::{Emitter, Manager};

use super::APP;
use crate::app::events::{NATIVE_GRID_SIZE, NATIVE_SCROLL};
use crate::sessions::Session;

/// What the frames last reported, so each report goes out only when it
/// changes.
struct Reported {
    // Last (cols << 16 | rows) the grid took, which the page's hidden xterm
    // follows, and last the game was told through NAWS.
    grid_size: AtomicU32,
    game_size: AtomicU32,
    // The scroll state last reported to the page, as a `scroll_report_key`.
    // Starts at a value no key reaches, so the first frame reports.
    scroll: AtomicU64,
}

static REPORTED: Reported = Reported {
    grid_size: AtomicU32::new(0),
    game_size: AtomicU32::new(0),
    scroll: AtomicU64::new(u64::MAX),
};

/// The rows the grid takes and the rows the game is told, for a pane whose
/// surface fits `fit` rows while the pinned prompt band borrows `lent`.
/// The surface keeps the whole pane and the grid gives the rows up from
/// its top, so its newest line sits right above the band. The game is
/// told the rows the pane holds with a one row band. A fight that grows
/// the band by a row only moves the text, so the game hears of no new
/// size and wraps as before. `keptRows` and `gameSize` in
/// src/lib/terminalRows.ts do the same for xterm, and both run
/// fixtures/terminal-rows/cases.json, so keep them in step.
pub(super) fn grid_and_game_rows(fit: usize, lent: usize) -> (usize, usize) {
    (fit.saturating_sub(lent).max(1), fit)
}

fn clamp_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// Whether `cols` by `rows` differs from the size `last` holds, which then
/// holds it. Packed as `cols << 16 | rows`.
fn changed(last: &AtomicU32, cols: u16, rows: u16) -> bool {
    let packed = (u32::from(cols) << 16) | u32::from(rows);
    last.swap(packed, Ordering::AcqRel) != packed
}

/// When the native surface owns the terminal, tell the page the grid size,
/// so its hidden xterm matches the surface, and advertise the size to the
/// MUD so lines wrap to fill the pane (instead of the xterm width). Each
/// fires only when it changes, and the game's size leaves out the rows the
/// pinned band borrows (`grid_and_game_rows`).
pub(super) fn report_sizes(cols: usize, rows: usize, game_rows: usize) {
    let cols = clamp_u16(cols);
    let rows = clamp_u16(rows);
    let game_rows = clamp_u16(game_rows);
    let grid_news = changed(&REPORTED.grid_size, cols, rows);
    let game_news = changed(&REPORTED.game_size, cols, game_rows);
    if !grid_news && !game_news {
        return;
    }
    let Some(app) = APP.get() else {
        return;
    };
    if grid_news {
        // Tell the frontend so it can size hidden xterm to the same grid;
        // when a DOM overlay reveals xterm it then matches the surface
        // exactly.
        let _ = app.emit(NATIVE_GRID_SIZE, (cols, rows));
    }
    if !game_news {
        return;
    }
    let session = app
        .state::<crate::app::state::SharedState>()
        .selected_session();
    if let Ok(mut ws) = session.window_size.lock() {
        *ws = (cols, game_rows);
    }
    tell_session(&session, &REPORTED.game_size);
}

/// Tell the connection `session` runs the game's size that `newest`
/// holds, packed as `changed` keeps it. The frame runs on the main thread,
/// so it never waits on the session slot. When a command holds the slot,
/// a task waits for it instead and then sends the size that is newest by
/// then. The frames after this one see no new size, so without the task
/// the game would wrap at the old width until the window changed again.
fn tell_session(session: &Arc<Session>, newest: &'static AtomicU32) {
    if let Ok(slot) = session.slot.try_lock() {
        send_game_size(slot.as_ref(), newest);
        return;
    }
    let session = Arc::clone(session);
    tauri::async_runtime::spawn(async move {
        let slot = session.slot.lock().await;
        send_game_size(slot.as_ref(), newest);
    });
}

/// Hand the live session, if there is one, the size `newest` holds.
fn send_game_size(session: Option<&crate::session::SessionHandle>, newest: &AtomicU32) {
    if let Some(handle) = session {
        let packed = newest.load(Ordering::Acquire);
        handle.set_window_size((packed >> 16) as u16, packed as u16);
    }
}

/// The key that decides whether a scroll report is news: the display
/// offset and the history length packed together, with the length zeroed
/// at the live tail. The page hides the depth there, and the length grows
/// with every line of output. Each half stops one short of `u32::MAX`, so
/// no key equals the unset marker.
fn scroll_report_key(offset: usize, max: usize) -> u64 {
    let cap = u64::from(u32::MAX - 1);
    let clamp = |n: usize| u64::try_from(n).map_or(cap, |n| n.min(cap));
    let max = if offset == 0 { 0 } else { clamp(max) };
    (clamp(offset) << 32) | max
}

/// Send the page the display offset and the history length as
/// `vosh://native-scroll` `[offset, max]`. The page learns from it
/// whether the scrollback split is open, and draws the scroll depth from
/// it. Only fires when `scroll_report_key` changes, so the live
/// tail reports once as `[0, max]` and then stays quiet.
pub(super) fn report_scroll_if_changed() {
    let (offset, max) = crate::native::grid::scroll_metrics();
    let key = scroll_report_key(offset, max);
    if REPORTED.scroll.swap(key, Ordering::AcqRel) == key {
        return;
    }
    if let Some(app) = APP.get() {
        let _ = app.emit(NATIVE_SCROLL, (offset, max));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn scroll_key_ignores_history_growth_at_the_live_tail() {
        assert_eq!(scroll_report_key(0, 100), scroll_report_key(0, 250));
        assert_eq!(scroll_report_key(0, 0), 0);
    }

    #[test]
    fn scroll_key_changes_with_offset_or_history_when_scrolled() {
        assert_ne!(scroll_report_key(0, 100), scroll_report_key(1, 100));
        assert_ne!(scroll_report_key(5, 100), scroll_report_key(6, 100));
        assert_ne!(scroll_report_key(5, 100), scroll_report_key(5, 101));
        assert_eq!(scroll_report_key(5, 100), (5 << 32) + 100);
    }

    #[test]
    fn scroll_key_never_reaches_the_unset_marker() {
        assert_ne!(scroll_report_key(usize::MAX, usize::MAX - 1), u64::MAX);
        assert_ne!(scroll_report_key(1, usize::MAX), u64::MAX);
    }

    /// The grid rows and the rows the game hears of, frame by frame, for
    /// a pane that fits `fit` rows while the band borrows `lent`.
    fn reported(frames: &[(usize, usize)]) -> (Vec<u16>, Vec<u16>) {
        let grid = AtomicU32::new(0);
        let game = AtomicU32::new(0);
        let (mut sized, mut told) = (Vec::new(), Vec::new());
        for &(fit, lent) in frames {
            let (rows, game_rows) = grid_and_game_rows(fit, lent);
            let (rows, game_rows) = (clamp_u16(rows), clamp_u16(game_rows));
            if changed(&grid, 120, rows) {
                sized.push(rows);
            }
            if changed(&game, 120, game_rows) {
                told.push(game_rows);
            }
        }
        (sized, told)
    }

    #[test]
    fn a_row_the_pinned_band_borrows_never_reaches_the_game() {
        // A fight starts and ends three times in a second, then the
        // window grows by two rows and a fight starts in it.
        let frames = [
            (40, 0),
            (40, 1),
            (40, 0),
            (40, 1),
            (40, 0),
            (40, 1),
            (40, 0),
            (42, 0),
            (42, 1),
        ];
        let (sized, told) = reported(&frames);
        // The grid gives up its top row to the band and takes it back
        // each time, so the page's hidden xterm follows it.
        assert_eq!(sized, [40, 39, 40, 39, 40, 39, 40, 42, 41]);
        // The game hears the rows the pane holds with a one row band,
        // once, and again only when the window itself changes.
        assert_eq!(told, [40, 42]);
    }

    #[test]
    fn the_grid_keeps_a_row_whatever_the_band_borrows() {
        assert_eq!(grid_and_game_rows(3, 5), (1, 3));
        assert_eq!(grid_and_game_rows(40, 0), (40, 40));
    }

    /// fixtures/terminal-rows/cases.json, which `keptRows`, `gameSize` and
    /// `GameSizeReport` in src/lib/terminalRows.ts run too.
    #[derive(serde::Deserialize)]
    struct RowCases {
        split: Vec<SplitCase>,
        reports: Vec<ReportCase>,
    }

    #[derive(serde::Deserialize)]
    struct SplitCase {
        name: String,
        fit: usize,
        lent: usize,
        grid: usize,
        game: Option<usize>,
        game_underlay: Option<usize>,
    }

    impl SplitCase {
        /// The rows the case says the game is told under the page. A pane
        /// no taller than what the band borrows names them `game_underlay`,
        /// since xterm tells the game other rows there (`game_short`, which
        /// only xterm reads).
        fn game(&self) -> usize {
            match (self.game, self.game_underlay) {
                (Some(game), None) | (None, Some(game)) => game,
                _ => panic!("{} needs game or game_underlay", self.name),
            }
        }
    }

    #[derive(serde::Deserialize)]
    struct ReportCase {
        name: String,
        frames: Vec<(u16, usize, usize)>,
        grid: Vec<usize>,
        told: Vec<(u16, u16)>,
    }

    fn row_cases() -> RowCases {
        let text = include_str!("../../../../fixtures/terminal-rows/cases.json");
        serde_json::from_str(text).expect("the row cases parse")
    }

    #[test]
    fn rows_split_as_the_cases_xterm_runs() {
        let cases = row_cases();
        assert!(!cases.split.is_empty());
        for case in &cases.split {
            let (grid, game) = grid_and_game_rows(case.fit, case.lent);
            assert_eq!(grid, case.grid, "{}", case.name);
            assert_eq!(game, case.game(), "{}", case.name);
        }
    }

    /// Runs `changed` on a slot of its own, the way `report_sizes` does
    /// for the game, and never runs `report_sizes` itself, which keeps its
    /// slots in `REPORTED` and reaches the app. A change to the rows
    /// `report_sizes` hands `changed` passes here unseen.
    #[test]
    fn sizes_reach_the_game_as_the_cases_xterm_runs() {
        let cases = row_cases();
        assert!(!cases.reports.is_empty());
        for case in &cases.reports {
            let last = AtomicU32::new(0);
            let (mut grid, mut told) = (Vec::new(), Vec::new());
            for &(cols, fit, lent) in &case.frames {
                let (rows, game_rows) = grid_and_game_rows(fit, lent);
                grid.push(rows);
                let game_rows = clamp_u16(game_rows);
                if changed(&last, cols, game_rows) {
                    told.push((cols, game_rows));
                }
            }
            assert_eq!(grid, case.grid, "{}", case.name);
            assert_eq!(told, case.told, "{}", case.name);
        }
    }

    /// Whether the game reads `want` from the client within `WAIT`.
    /// `heard` keeps what it read so far across calls.
    async fn game_hears(
        game: &mut tokio::net::TcpStream,
        heard: &mut Vec<u8>,
        want: &[u8],
    ) -> bool {
        use tokio::io::AsyncReadExt;
        let deadline = tokio::time::Instant::now() + WAIT;
        let mut buf = [0u8; 4096];
        loop {
            if heard.windows(want.len()).any(|w| w == want) {
                return true;
            }
            match tokio::time::timeout_at(deadline, game.read(&mut buf)).await {
                Ok(Ok(n)) if n > 0 => heard.extend_from_slice(&buf[..n]),
                _ => return false,
            }
        }
    }

    /// Run `f` on a plain thread and wait for it. The app runs each frame
    /// on the main thread, which has no tokio runtime, so a frame that
    /// starts a task there must hand it to one that lives on its own.
    fn on_a_plain_thread(f: impl FnOnce() + Send) {
        std::thread::scope(|s| {
            s.spawn(f).join().expect("the frame");
        });
    }

    /// A live session against a local game that asked for your window
    /// size and heard the one the session started with, 100 by 40.
    struct SizedGame {
        state: crate::app::state::SharedState,
        /// The game's end of the socket.
        game: tokio::net::TcpStream,
        /// What the game read so far.
        heard: Vec<u8>,
        /// The mock app the session runs in.
        app: tauri::App<tauri::test::MockRuntime>,
    }

    /// What the game reads when the client says it is `cols` by 40.
    fn naws(cols: u8) -> [u8; 9] {
        use vosh_protocol::telnet::codes::{option::NAWS, IAC, SB, SE};
        [IAC, SB, NAWS, 0, cols, 0, 40, IAC, SE]
    }

    async fn sized_game() -> SizedGame {
        use tauri::test::{mock_builder, mock_context, noop_assets};
        use tokio::io::AsyncWriteExt;
        use vosh_protocol::telnet::codes::{option::NAWS, DO, IAC};

        use crate::app::state::{AppState, SharedState};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a local port");
        let port = listener.local_addr().expect("an address").port();
        let state: SharedState = Arc::new(AppState::default());
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("a mock app");
        app.manage::<SharedState>(state.clone());
        let accept = tokio::spawn(async move { listener.accept().await.expect("a client").0 });
        let handle = crate::session::spawn(
            app.handle().clone(),
            &state,
            &state.selected_session(),
            "127.0.0.1".into(),
            port,
            false,
            false,
            None,
            (100, 40),
        )
        .await
        .expect("the game answers");
        *state.selected_session().slot.lock().await = Some(handle);
        let mut game = accept.await.expect("the accept task");
        let mut heard = Vec::new();

        // The game asks for your window size and hears the one the
        // session started with.
        game.write_all(&[IAC, DO, NAWS]).await.expect("the ask");
        assert!(game_hears(&mut game, &mut heard, &naws(100)).await);
        SizedGame {
            state,
            game,
            heard,
            app,
        }
    }

    /// End the session `state` holds.
    async fn end_session(state: &crate::app::state::SharedState) {
        let handle = state.selected_session().slot.lock().await.take();
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
    }

    /// Bug 12. You widen the window while a command holds the session,
    /// so the frame that sizes the grid finds the session busy. The
    /// frames after it see no new size, yet the game still hears the new
    /// width instead of wrapping at the old one.
    #[tokio::test]
    async fn a_resize_while_the_session_is_busy_still_reaches_the_game() {
        // The game's size the frames last reported, as
        // `REPORTED.game_size` holds it in the app.
        static LAST: AtomicU32 = AtomicU32::new(0);
        let SizedGame {
            state,
            mut game,
            mut heard,
            app: _app,
        } = sized_game().await;

        // Frames report the game's size the way `report_sizes` does, on
        // a thread outside any runtime, as the main thread runs them.
        let frame = |cols: u16| {
            on_a_plain_thread(|| {
                if changed(&LAST, cols, 40) {
                    tell_session(&state.selected_session(), &LAST);
                }
            });
        };
        frame(100);
        // You widen the window while a command holds the session.
        {
            let session = state.selected_session();
            let _busy = session.slot.lock().await;
            frame(120);
        }
        // Frames go on at the new size, which none of them reports.
        frame(120);
        frame(120);
        assert!(
            game_hears(&mut game, &mut heard, &naws(120)).await,
            "the game kept wrapping at 100 columns"
        );

        end_session(&state).await;
    }

    /// A size that waited on the session never undoes a newer one. The
    /// frame that finds the session busy leaves a task waiting, and a
    /// later frame takes the session first and sends a wider size. The
    /// task then sends the newest size, not the one its frame saw, so
    /// the game keeps the wider one.
    #[tokio::test]
    async fn a_size_that_waited_never_undoes_a_newer_one() {
        use vosh_protocol::telnet::codes::{option::NAWS, IAC, SB};

        static LAST: AtomicU32 = AtomicU32::new(0);
        let SizedGame {
            state,
            mut game,
            mut heard,
            app: _app,
        } = sized_game().await;
        assert!(changed(&LAST, 100, 40));

        // The waiting task holds a clone of the session until it has
        // sent, so the count falls back to this once it is done.
        let session = state.selected_session();
        let idle = Arc::strong_count(&session);
        {
            let busy = session.slot.lock().await;
            // You widen the window while a command holds the session.
            assert!(changed(&LAST, 120, 40));
            on_a_plain_thread(|| tell_session(&session, &LAST));
            // You widen it again, and that frame takes the session before
            // the waiting task does.
            assert!(changed(&LAST, 130, 40));
            send_game_size(busy.as_ref(), &LAST);
        }
        let deadline = tokio::time::Instant::now() + WAIT;
        while Arc::strong_count(&session) > idle {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the waiting task never sent"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        // What the session sends next lands behind every size it sent
        // before, so the last size ahead of it is the one the game keeps.
        let marker = b"after the resize";
        let sent = session
            .slot
            .lock()
            .await
            .as_ref()
            .expect("the session")
            .send(marker.to_vec());
        assert!(sent);
        assert!(game_hears(&mut game, &mut heard, marker).await);
        let at = heard
            .windows(marker.len())
            .position(|w| w == marker)
            .expect("the marker");
        let ahead = &heard[..at];
        let last = ahead
            .windows(3)
            .rposition(|w| w == [IAC, SB, NAWS])
            .expect("a size");
        assert_eq!(
            ahead.get(last..last + 9),
            Some(&naws(130)[..]),
            "an older size went out last"
        );

        end_session(&state).await;
    }
}
