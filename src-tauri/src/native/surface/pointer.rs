//! Pointer input the page forwards to the surface. A press, drag or
//! release selects text, drags the scrollbar thumb or the split
//! divider, or opens a link, and the wheel scrolls the history. The
//! surface tells the page which cursor to show, and a selection goes
//! to the clipboard on release.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;

use tauri::Emitter;

use super::{platform, redraw_now, split_drag, APP, PANE};
use crate::app::events::{NATIVE_COPIED, TERMINAL_CLICKED, TERMINAL_CURSOR};
use crate::native::grid;
use crate::sessions::SessionId;

/// The pointer input in flight: the drag a press started, the wheel's
/// remainder, the link under the pointer and the cursor the page shows.
struct Pointer {
    // Text selection in progress.
    selecting: AtomicBool,
    // A divider drag in progress.
    dragging_divider: AtomicBool,
    // A scrollbar thumb drag in progress.
    dragging_scrollbar: AtomicBool,
    // The selection drag began in the history half of an open split, so it
    // stays in the history and autoscrolls past the divider (split_drag.rs).
    from_history: AtomicBool,
    // An autoscroll ticker is running.
    autoscroll_armed: AtomicBool,
    // The last point of a selection drag as (x, y, surface height) in
    // physical pixels, which each autoscroll tick reads again.
    last_drag: Mutex<Option<(f64, f64, f64)>>,
    // Fractional scroll accumulator so precise (trackpad) deltas are not
    // rounded away.
    scroll_accum: Mutex<f64>,
    // The URL under the pointer as (grid_line, start_col, end_col), so the
    // renderer can underline it as a clickable affordance.
    hover_url: Mutex<Option<(i32, usize, usize)>>,
    // The cursor last reported to the page, as a `CursorHint` discriminant.
    cursor: AtomicU8,
}

static POINTER: Pointer = Pointer {
    selecting: AtomicBool::new(false),
    dragging_divider: AtomicBool::new(false),
    dragging_scrollbar: AtomicBool::new(false),
    from_history: AtomicBool::new(false),
    autoscroll_armed: AtomicBool::new(false),
    last_drag: Mutex::new(None),
    scroll_accum: Mutex::new(0.0),
    hover_url: Mutex::new(None),
    cursor: AtomicU8::new(CursorHint::Default as u8),
};

/// The scrollback split's divider, as f32 bits.
struct Split {
    // The divider position a drag set, as a fraction of surface height
    // (0 = no drag yet, so `split_ratio` gives the default).
    ratio: AtomicU32,
    // The exact divider fraction the renderer last drew (0 = no split), so
    // the grab band and the resize cursor the page shows across it line up
    // with the rendered line rather than the raw ratio.
    divider_frac: AtomicU32,
}

static SPLIT: Split = Split {
    ratio: AtomicU32::new(0),
    divider_frac: AtomicU32::new(0),
};

/// The backing scale and atlas cell size as f32 bits (set each frame / on
/// bounds) so the pointer code can map a point to a grid cell without
/// locking the surface.
pub(super) struct Cells {
    pub(super) dpr: AtomicU32,
    pub(super) cell_w: AtomicU32,
    pub(super) cell_h: AtomicU32,
}

pub(super) static CELLS: Cells = Cells {
    dpr: AtomicU32::new(0),
    cell_w: AtomicU32::new(0),
    cell_h: AtomicU32::new(0),
};

/// Divider position as a fraction of the surface height (0.66 default).
pub(crate) fn split_ratio() -> f32 {
    let bits = SPLIT.ratio.load(Ordering::Acquire);
    if bits == 0 {
        0.66
    } else {
        f32::from_bits(bits)
    }
}

fn set_split_ratio(ratio: f32) {
    SPLIT.ratio.store(ratio.to_bits(), Ordering::Release);
}

pub(super) fn set_divider_frac(frac: Option<f32>) {
    SPLIT
        .divider_frac
        .store(frac.map_or(0, f32::to_bits), Ordering::Release);
}

fn divider_frac() -> Option<f32> {
    let bits = SPLIT.divider_frac.load(Ordering::Acquire);
    (bits != 0).then(|| f32::from_bits(bits))
}

/// The hovered URL's cell range, for the renderer's hover underline.
pub(super) fn hover_url() -> Option<(i32, usize, usize)> {
    POINTER.hover_url.lock().ok().and_then(|h| *h)
}

fn set_hover_url(next: Option<(i32, usize, usize)>) {
    let changed = if let Ok(mut h) = POINTER.hover_url.lock() {
        let changed = *h != next;
        *h = next;
        changed
    } else {
        false
    };
    if changed {
        redraw_now();
    }
}

pub(super) fn store_f32(slot: &AtomicU32, value: f32) {
    slot.store(value.to_bits(), Ordering::Release);
}

pub(super) fn load_f32(slot: &AtomicU32, default: f32) -> f32 {
    let bits = slot.load(Ordering::Acquire);
    if bits == 0 {
        default
    } else {
        f32::from_bits(bits)
    }
}

/// The surface as the last frame left it, for a surface `height_px`
/// physical pixels tall: the cell size, the backing scale, and the
/// divider the renderer drew.
fn surface_frame(height_px: f64) -> split_drag::Frame {
    split_drag::Frame {
        cell_w: f64::from(load_f32(&CELLS.cell_w, 0.0)),
        cell_h: f64::from(load_f32(&CELLS.cell_h, 0.0)),
        height: height_px,
        dpr: f64::from(load_f32(&CELLS.dpr, 2.0)),
        divider: divider_frac(),
    }
}

/// Map the event's point to a cell `(line, col)` of the grid it acts on,
/// mirroring the renderer's split mapping: the bottom (live) region of an
/// open split reads at offset 0, the top (history) region at the scroll
/// offset.
fn phys_point_to_cell(ev: &PointerEvent) -> Option<(i32, usize)> {
    let offset = grid::current_display_offset(ev.session);
    let view = surface_frame(ev.height).view(offset)?;
    Some(view.cell_at(ev.x, ev.y))
}

/// A pointer event in surface-physical pixels, with the surface size, the
/// open-link modifier (Cmd) state, and the session whose grid showed as
/// it came, which it acts on.
struct PointerEvent {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub open_modifier: bool,
    pub session: SessionId,
}

/// True when the point falls in the scrollbar hit zone (right edge) while
/// scrolled. The zone is wider than the drawn bar for forgiving grabs.
fn in_scrollbar_zone(ev: &PointerEvent) -> bool {
    let (offset, scrollback) = grid::scroll_metrics(ev.session);
    if offset == 0 || scrollback == 0 {
        return false;
    }
    let dpr = f64::from(load_f32(&CELLS.dpr, 2.0));
    ev.width > 0.0 && ev.x >= ev.width - 12.0 * dpr
}

/// Map a scrollbar drag y to an absolute display offset: the thumb center
/// follows the pointer.
// Scrollback lengths are far inside f64's exact-integer range.
#[allow(clippy::cast_precision_loss)]
fn scrollbar_scroll_to(ev: &PointerEvent) {
    let cell_h = f64::from(load_f32(&CELLS.cell_h, 0.0));
    if cell_h <= 0.0 || ev.height <= 0.0 {
        return;
    }
    let (_, scrollback) = grid::scroll_metrics(ev.session);
    if scrollback == 0 {
        return;
    }
    let rows = (ev.height / cell_h).floor().max(1.0);
    let total = scrollback as f64 + rows;
    let scroll_top = ((ev.y / ev.height) * total - rows / 2.0).clamp(0.0, scrollback as f64);
    let target = scrollback as f64 - scroll_top;
    grid::scroll_to_offset(ev.session, target.round().max(0.0) as usize);
    redraw_now();
}

/// Middle-click toggles the split of the grid of `session`: scrolled
/// snaps back to the live tail; at the tail it pages up into scrollback
/// to open the split.
fn middle_click(session: SessionId) {
    let (offset, _) = grid::scroll_metrics(session);
    if offset > 0 {
        grid::scroll_to_bottom(session);
    } else {
        grid::scroll_page(session, true, split_ratio());
    }
    redraw_now();
    // A middle click never reaches pointer_up, so it sends the event that
    // gives the command line focus here, as a left click does there.
    if let Some(app) = APP.get() {
        let _ = app.emit(TERMINAL_CLICKED, ());
    }
}

/// A wheel delta forwarded from the page. Positive reveals older lines.
/// The shown grid scrolls by whole lines and the accumulator keeps the
/// rest. Main thread only.
pub(crate) fn forward_wheel(delta_y: f64) {
    let Ok(mut acc) = POINTER.scroll_accum.lock() else {
        return;
    };
    // Positive deltaY pulls content down = reveal older lines = scroll up.
    *acc += delta_y * 0.12;
    let lines = acc.trunc() as i32;
    *acc -= f64::from(lines);
    drop(acc);
    if lines != 0 {
        grid::scroll(grid::shown(), lines);
        redraw_now();
    }
}

/// Cmd+click on a URL opens it; a press in the scrollbar zone starts
/// a thumb drag; a press on the divider starts a divider drag; anything
/// else starts a selection.
fn pointer_down(ev: &PointerEvent) {
    // A press always starts fresh. Forwarded input cannot promise that
    // every press got its release, and a stale flag would turn the next
    // selection into a divider or scrollbar drag.
    POINTER.dragging_scrollbar.store(false, Ordering::Release);
    POINTER.dragging_divider.store(false, Ordering::Release);
    POINTER.selecting.store(false, Ordering::Release);
    POINTER.from_history.store(false, Ordering::Release);
    let cell = phys_point_to_cell(ev);
    if ev.open_modifier {
        if let Some((line, col)) = cell {
            if let Some((url, _, _)) = grid::links::url_at(ev.session, line, col) {
                platform::open_url(&url);
                return;
            }
        }
    }
    if in_scrollbar_zone(ev) {
        POINTER.dragging_scrollbar.store(true, Ordering::Release);
        scrollbar_scroll_to(ev);
        return;
    }
    // Grab the divider where it is DRAWN (divider_frac), not at the raw
    // ratio, in the same band that shows the page's resize cursor.
    if near_divider(ev) {
        POINTER.dragging_divider.store(true, Ordering::Release);
        return;
    }
    grid::clear_selection(ev.session);
    if let Some((line, col)) = cell {
        grid::start_selection(ev.session, line, col);
        POINTER.selecting.store(true, Ordering::Release);
        let offset = grid::current_display_offset(ev.session);
        let from_history = surface_frame(ev.height)
            .view(offset)
            .is_some_and(|view| view.in_history(ev.y));
        POINTER.from_history.store(from_history, Ordering::Release);
        redraw_now();
    }
}

/// Move the scrollbar thumb or the divider, or extend the selection,
/// while dragging.
fn pointer_dragged(ev: &PointerEvent) {
    if POINTER.dragging_scrollbar.load(Ordering::Acquire) {
        scrollbar_scroll_to(ev);
        return;
    }
    if POINTER.dragging_divider.load(Ordering::Acquire) {
        if ev.height > 0.0 {
            let frac = ev.y / ev.height;
            set_split_ratio((frac.clamp(0.15, 0.85)) as f32);
            redraw_now();
        }
        return;
    }
    if POINTER.selecting.load(Ordering::Acquire) {
        if POINTER.from_history.load(Ordering::Acquire) {
            history_drag(ev);
            return;
        }
        if let Some((line, col)) = phys_point_to_cell(ev) {
            grid::update_selection(ev.session, line, col);
            redraw_now();
        }
    }
}

/// Extend a selection drag that began in the history half. Past the
/// divider it stops at the last history line, and a ticker scrolls the
/// history toward the tail while the pointer stays there.
fn history_drag(ev: &PointerEvent) {
    if let Ok(mut last) = POINTER.last_drag.lock() {
        *last = Some((ev.x, ev.y, ev.height));
    }
    let frame = surface_frame(ev.height);
    let scroll = grid::with_grid_mut(ev.session, |grid| {
        split_drag::drag(grid, &frame, ev.x, ev.y)
    })
    .unwrap_or(0);
    redraw_now();
    if scroll > 0 {
        arm_autoscroll();
    }
}

/// Start the autoscroll ticker unless one runs. Each tick goes to the
/// main thread, and the ticker stops on the first tick with nothing to
/// scroll.
fn arm_autoscroll() {
    if POINTER.autoscroll_armed.swap(true, Ordering::AcqRel) {
        return;
    }
    let Some(app) = APP.get().cloned() else {
        POINTER.autoscroll_armed.store(false, Ordering::Release);
        return;
    };
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(split_drag::AUTOSCROLL_TICK).await;
            let (tx, rx) = tokio::sync::oneshot::channel();
            let ran = match app.run_on_main_thread(move || {
                let _ = tx.send(autoscroll_tick());
            }) {
                Ok(()) => rx.await.ok(),
                Err(_) => None,
            };
            match ran {
                Some(true) => {}
                // The tick that ended it cleared the flag on the main
                // thread, where a new drag may have set it again since.
                Some(false) => break,
                // No tick ran, so none cleared it, and none could start
                // another ticker while it stayed set.
                None => {
                    POINTER.autoscroll_armed.store(false, Ordering::Release);
                    break;
                }
            }
        }
    });
}

/// One autoscroll tick of a drag from the history half of the shown
/// grid. Runs on the main thread. False, with the ticker's flag cleared,
/// once the drag ended, the pointer came back above the divider, or the
/// split closed at the tail.
fn autoscroll_tick() -> bool {
    let point = POINTER.last_drag.lock().ok().and_then(|last| *last);
    let more = match point {
        Some((x, y, height))
            if POINTER.selecting.load(Ordering::Acquire)
                && POINTER.from_history.load(Ordering::Acquire) =>
        {
            let frame = surface_frame(height);
            let more =
                grid::with_grid_mut(grid::shown(), |grid| split_drag::tick(grid, &frame, x, y))
                    .unwrap_or(false);
            redraw_now();
            more
        }
        _ => false,
    };
    if !more {
        POINTER.autoscroll_armed.store(false, Ordering::Release);
    }
    more
}

fn pointer_up(session: SessionId) {
    POINTER.from_history.store(false, Ordering::Release);
    let was_scrollbar = POINTER.dragging_scrollbar.swap(false, Ordering::AcqRel);
    let was_divider = POINTER.dragging_divider.swap(false, Ordering::AcqRel);
    if !was_scrollbar && !was_divider && POINTER.selecting.swap(false, Ordering::AcqRel) {
        // Copy the selection to the clipboard on release.
        copy_selection(session);
    }
    // Clicking the terminal focuses the command input, like clicking any
    // other part of the window. The page cancels the press it forwards,
    // so no DOM mouseup follows to do this, and the page listens for the
    // event instead.
    if let Some(app) = APP.get() {
        let _ = app.emit(TERMINAL_CLICKED, ());
    }
}

/// The main window lost focus. A drag whose release may never come then,
/// after a system gesture or a grab another window took, ends here, so no
/// autoscroll tick runs on to the tail and closes the split behind your
/// back. The selection stays for a copy, and nothing goes to the
/// clipboard. Leaving the window ends nothing, as the release still comes.
pub(crate) fn window_blurred() {
    POINTER.selecting.store(false, Ordering::Release);
    POINTER.from_history.store(false, Ordering::Release);
    POINTER.dragging_scrollbar.store(false, Ordering::Release);
    POINTER.dragging_divider.store(false, Ordering::Release);
    if let Ok(mut last) = POINTER.last_drag.lock() {
        *last = None;
    }
}

/// Another session's grid shows. What the pointer held on the grid that
/// showed before ends here, as a blur ends it: a drag with its autoscroll,
/// the wheel's remainder, the hovered link and the divider the last frame
/// drew. The selection stays in that grid. Asks for no frame itself, so
/// it takes no lock but the pointer's state.
pub(super) fn let_go() {
    window_blurred();
    if let Ok(mut acc) = POINTER.scroll_accum.lock() {
        *acc = 0.0;
    }
    if let Ok(mut hover) = POINTER.hover_url.lock() {
        *hover = None;
    }
    set_divider_frac(None);
}

/// Track the URL under the pointer so the renderer can underline it. Only
/// repaints when the hovered range actually changes.
fn pointer_moved(ev: Option<&PointerEvent>) {
    let next = ev.and_then(|e| {
        let (line, col) = phys_point_to_cell(e)?;
        grid::links::url_at(e.session, line, col).map(|(_, s, end)| (line, s, end))
    });
    set_hover_url(next);
}

/// A pointer event forwarded from the page, which sits on top and
/// receives every click. `x` and `y` are CSS px relative to the pane's
/// top-left corner. `kind` is "down", "drag", "up", "move", "leave", or
/// "middle". It acts on the shown grid. Must run on the main thread.
pub(crate) fn forward_pointer(kind: &str, x: f64, y: f64, open_modifier: bool) {
    let dpr = f64::from(load_f32(&CELLS.dpr, 2.0));
    let (width, height) = PANE
        .viewport
        .lock()
        .ok()
        .and_then(|v| *v)
        .map_or((0.0, 0.0), |[_, _, w, h]| (f64::from(w), f64::from(h)));
    let ev = PointerEvent {
        x: x * dpr,
        y: y * dpr,
        width,
        height,
        open_modifier,
        session: grid::shown(),
    };
    match kind {
        "down" => pointer_down(&ev),
        "drag" => pointer_dragged(&ev),
        "up" => pointer_up(ev.session),
        "move" => pointer_moved(Some(&ev)),
        "leave" => pointer_moved(None),
        "middle" => middle_click(ev.session),
        _ => {}
    }
    let hint = if kind == "leave" {
        CursorHint::Default
    } else {
        cursor_hint(
            POINTER.dragging_divider.load(Ordering::Acquire),
            POINTER.selecting.load(Ordering::Acquire)
                || POINTER.dragging_scrollbar.load(Ordering::Acquire),
            // A press in the scrollbar zone grabs the thumb first.
            near_divider(&ev) && !in_scrollbar_zone(&ev),
            ev.open_modifier && hover_url().is_some(),
        )
    };
    report_cursor(hint);
}

/// Half the height of the divider's grab band, in points. A press inside
/// it starts a divider drag, and the page shows the resize cursor across
/// the same band.
const DIVIDER_GRAB_PT: f64 = 8.0;

/// True when the point sits on the divider as drawn, within the grab band.
fn near_divider(ev: &PointerEvent) -> bool {
    let Some(drawn) = divider_frac() else {
        return false;
    };
    let dpr = f64::from(load_f32(&CELLS.dpr, 2.0));
    ev.height > 0.0 && (ev.y - f64::from(drawn) * ev.height).abs() <= DIVIDER_GRAB_PT * dpr
}

/// The pointer cursor the page should show over the pane. The page owns
/// the cursor, so the surface reports what the pointer is over and the
/// page sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorHint {
    Default = 0,
    RowResize = 1,
    Pointer = 2,
}

impl CursorHint {
    fn css(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::RowResize => "row-resize",
            Self::Pointer => "pointer",
        }
    }
}

/// Pick the cursor. A divider drag holds the resize cursor wherever the
/// pointer goes, and a selection or scrollbar drag holds the arrow even
/// across the divider. Otherwise the divider band shows the resize cursor
/// and a link under the open modifier shows the hand.
fn cursor_hint(
    dragging_divider: bool,
    dragging_other: bool,
    on_divider: bool,
    link_armed: bool,
) -> CursorHint {
    if dragging_divider {
        CursorHint::RowResize
    } else if dragging_other {
        CursorHint::Default
    } else if on_divider {
        CursorHint::RowResize
    } else if link_armed {
        CursorHint::Pointer
    } else {
        CursorHint::Default
    }
}

/// Send `vosh://terminal-cursor` with the CSS cursor name, only when it
/// changes.
fn report_cursor(hint: CursorHint) {
    if POINTER.cursor.swap(hint as u8, Ordering::AcqRel) == hint as u8 {
        return;
    }
    if let Some(app) = APP.get() {
        let _ = app.emit(TERMINAL_CURSOR, hint.css());
    }
}

/// Copy the selection in the grid of `session` to the clipboard (no-op
/// when empty) and send the count of characters copied as
/// `vosh://native-copied`, so the page shows the copy toast.
fn copy_selection(session: SessionId) {
    let Some(text) = grid::selection_text(session) else {
        return;
    };
    if text.is_empty() {
        return;
    }
    platform::set_clipboard(&text);
    if let Some(app) = APP.get() {
        let _ = app.emit(NATIVE_COPIED, text.chars().count());
    }
}

/// Copy the selection in the grid of `session`, dispatched to the main
/// thread. Called by the Cmd+C / Ctrl+C path from the frontend.
pub(crate) fn request_copy(session: SessionId) {
    let Some(app) = APP.get() else {
        return;
    };
    let _ = app.run_on_main_thread(move || copy_selection(session));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The session whose grid shows in these tests.
    const ONE: SessionId = SessionId::FIRST;

    /// A pointer event on a 160 by 120 px surface at scale 1.
    fn at(x: f64, y: f64) -> PointerEvent {
        PointerEvent {
            x,
            y,
            width: 160.0,
            height: 120.0,
            open_modifier: false,
            session: ONE,
        }
    }

    /// Lay the shown grid out as a split at offset `offset`: 10 px rows
    /// on 120 px, L00 to L59 with L48 to L59 on the screen, and the
    /// divider the renderer draws at 79 px. Call with the grids' test
    /// lock held, and `reset_pointer` after.
    fn split_surface(offset: i32) {
        crate::native::grid::blank_shared_grid_for_test(20, 12);
        let text: Vec<String> = (0..60).map(|n| format!("L{n:02}")).collect();
        crate::native::grid::with_grid_mut(ONE, |grid| {
            grid.feed(text.join("\r\n").as_bytes());
            grid.scroll(offset);
        });
        store_f32(&CELLS.cell_w, 8.0);
        store_f32(&CELLS.cell_h, 10.0);
        store_f32(&CELLS.dpr, 1.0);
        set_divider_frac(Some(79.0 / 120.0));
    }

    fn reset_pointer() {
        POINTER.selecting.store(false, Ordering::Release);
        POINTER.from_history.store(false, Ordering::Release);
        POINTER.autoscroll_armed.store(false, Ordering::Release);
        *POINTER.last_drag.lock().unwrap() = None;
        CELLS.cell_w.store(0, Ordering::Release);
        CELLS.cell_h.store(0, Ordering::Release);
        CELLS.dpr.store(0, Ordering::Release);
        set_divider_frac(None);
        crate::native::grid::clear_selection(ONE);
    }

    #[test]
    fn a_history_drag_autoscrolls_through_the_divider_into_one_selection() {
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        split_surface(8);
        pointer_down(&at(0.0, 15.0));
        assert!(POINTER.from_history.load(Ordering::Acquire));
        // Past the divider the selection ends on the last history line,
        // L47 at offset 8, and nothing of the live half.
        pointer_dragged(&at(40.0, 100.0));
        let text = crate::native::grid::selection_text(ONE).expect("a selection");
        assert_eq!(text.lines().last(), Some("L47"));
        assert_eq!(crate::native::grid::current_display_offset(ONE), 8);
        // Each tick scrolls 7 lines toward the tail, and the second
        // reaches it, which closes the split. The selection runs on to
        // the pointer's cell in the full view.
        assert!(autoscroll_tick());
        assert_eq!(crate::native::grid::current_display_offset(ONE), 1);
        let text = crate::native::grid::selection_text(ONE).expect("a selection");
        assert_eq!(text.lines().last(), Some("L54"));
        assert!(!autoscroll_tick());
        assert_eq!(crate::native::grid::current_display_offset(ONE), 0);
        let text = crate::native::grid::selection_text(ONE).expect("a selection");
        let lines: Vec<&str> = text.lines().collect();
        let want: Vec<String> = (41..=58).map(|n| format!("L{n:02}")).collect();
        assert_eq!(lines, want);
        reset_pointer();
    }

    #[test]
    fn a_window_blur_ends_a_history_drag_and_its_autoscroll() {
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        split_surface(8);
        pointer_down(&at(0.0, 15.0));
        pointer_dragged(&at(40.0, 100.0));
        assert!(POINTER.selecting.load(Ordering::Acquire));
        window_blurred();
        assert!(!POINTER.selecting.load(Ordering::Acquire));
        assert!(!POINTER.from_history.load(Ordering::Acquire));
        // The next tick scrolls nothing and stops the ticker, and the
        // split stays open with the selection as it was.
        POINTER.autoscroll_armed.store(true, Ordering::Release);
        assert!(!autoscroll_tick());
        assert!(!POINTER.autoscroll_armed.load(Ordering::Acquire));
        assert_eq!(crate::native::grid::current_display_offset(ONE), 8);
        let text = crate::native::grid::selection_text(ONE).expect("a selection");
        assert_eq!(text.lines().last(), Some("L47"));
        // A drag event that still comes moves nothing.
        pointer_dragged(&at(40.0, 110.0));
        assert_eq!(crate::native::grid::current_display_offset(ONE), 8);
        let after = crate::native::grid::selection_text(ONE).expect("a selection");
        assert_eq!(after, text);
        reset_pointer();
    }

    #[test]
    fn a_drag_from_the_live_half_maps_the_pointer_as_before() {
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        split_surface(8);
        // Live row 10 is L58. Up across the divider the pointer reads the
        // history at the offset, row 2 there being L42.
        pointer_down(&at(0.0, 105.0));
        assert!(!POINTER.from_history.load(Ordering::Acquire));
        pointer_dragged(&at(0.0, 25.0));
        let text = crate::native::grid::selection_text(ONE).expect("a selection");
        assert_eq!(text.lines().next(), Some("L42"));
        assert_eq!(text.lines().last(), Some("L57"));
        assert!(!autoscroll_tick());
        assert_eq!(crate::native::grid::current_display_offset(ONE), 8);
        reset_pointer();
    }

    #[test]
    fn cursor_rests_as_the_arrow() {
        assert_eq!(cursor_hint(false, false, false, false), CursorHint::Default);
    }

    #[test]
    fn cursor_shows_resize_on_the_divider_and_through_its_drag() {
        assert_eq!(
            cursor_hint(false, false, true, false),
            CursorHint::RowResize
        );
        assert_eq!(
            cursor_hint(true, false, false, false),
            CursorHint::RowResize
        );
        assert_eq!(cursor_hint(true, false, false, true), CursorHint::RowResize);
    }

    #[test]
    fn cursor_keeps_the_arrow_through_a_selection_drag() {
        assert_eq!(cursor_hint(false, true, true, false), CursorHint::Default);
        assert_eq!(cursor_hint(false, true, false, true), CursorHint::Default);
    }

    #[test]
    fn cursor_shows_the_hand_on_an_armed_link() {
        assert_eq!(cursor_hint(false, false, false, true), CursorHint::Pointer);
        // The divider band wins over a link under it.
        assert_eq!(cursor_hint(false, false, true, true), CursorHint::RowResize);
    }

    #[test]
    fn cursor_hints_name_css_cursors() {
        assert_eq!(CursorHint::Default.css(), "default");
        assert_eq!(CursorHint::RowResize.css(), "row-resize");
        assert_eq!(CursorHint::Pointer.css(), "pointer");
    }
}
