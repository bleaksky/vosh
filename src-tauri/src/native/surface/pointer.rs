//! Pointer input the page forwards to the surface. A press, drag or
//! release selects text, drags the scrollbar thumb or the split
//! divider, or opens a link, and the wheel scrolls the history. The
//! surface tells the page which cursor to show, and a selection goes
//! to the clipboard on release.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;

use tauri::Emitter;

use super::{platform, redraw_now, split_drag, APP, VIEWPORT};
use crate::app::events::{NATIVE_COPIED, TERMINAL_CLICKED, TERMINAL_CURSOR};

// Fractional scroll accumulator so precise (trackpad) deltas are not
// rounded away; the divider position as a fraction of surface height; and
// whether a divider drag is in progress.
static SCROLL_ACCUM: Mutex<f64> = Mutex::new(0.0);
static SPLIT_RATIO: AtomicU32 = AtomicU32::new(0);
static DRAGGING: AtomicBool = AtomicBool::new(false);

/// Divider position as a fraction of the surface height (0.66 default).
pub(crate) fn split_ratio() -> f32 {
    let bits = SPLIT_RATIO.load(Ordering::Acquire);
    if bits == 0 {
        0.66
    } else {
        f32::from_bits(bits)
    }
}

fn set_split_ratio(ratio: f32) {
    SPLIT_RATIO.store(ratio.to_bits(), Ordering::Release);
}

// The exact divider fraction the renderer last drew (0 = no split), so the
// cursor rect aligns with the rendered line rather than the raw ratio.
static DIVIDER_FRAC: AtomicU32 = AtomicU32::new(0);

pub(super) fn set_divider_frac(frac: Option<f32>) {
    DIVIDER_FRAC.store(frac.map_or(0, f32::to_bits), Ordering::Release);
}

fn divider_frac() -> Option<f32> {
    let bits = DIVIDER_FRAC.load(Ordering::Acquire);
    (bits != 0).then(|| f32::from_bits(bits))
}

// Text selection in progress, plus the backing scale and atlas cell size
// (set each frame / on bounds) so the mouse handler can map a point to a
// grid cell without locking the surface.
static SELECTING: AtomicBool = AtomicBool::new(false);
pub(super) static DPR: AtomicU32 = AtomicU32::new(0);
pub(super) static CELL_W: AtomicU32 = AtomicU32::new(0);
pub(super) static CELL_H: AtomicU32 = AtomicU32::new(0);

// The URL under the pointer as (grid_line, start_col, end_col), so the
// renderer can underline it as a clickable affordance.
static HOVER_URL: Mutex<Option<(i32, usize, usize)>> = Mutex::new(None);

/// The hovered URL's cell range, for the renderer's hover underline.
pub(crate) fn hover_url() -> Option<(i32, usize, usize)> {
    HOVER_URL.lock().ok().and_then(|h| *h)
}

fn set_hover_url(next: Option<(i32, usize, usize)>) {
    let changed = if let Ok(mut h) = HOVER_URL.lock() {
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
        cell_w: f64::from(load_f32(&CELL_W, 0.0)),
        cell_h: f64::from(load_f32(&CELL_H, 0.0)),
        height: height_px,
        dpr: f64::from(load_f32(&DPR, 2.0)),
        divider: divider_frac(),
    }
}

/// Map a physical-pixel point inside the surface to a grid cell
/// `(line, col)`, mirroring the renderer's split mapping: the bottom (live)
/// region of an open split reads at offset 0, the top (history) region at
/// the scroll offset. `height_px` is the surface height in physical pixels.
fn phys_point_to_cell(phys_x: f64, phys_y: f64, height_px: f64) -> Option<(i32, usize)> {
    let offset = crate::native::grid::current_display_offset();
    let view = surface_frame(height_px).view(offset)?;
    Some(view.cell_at(phys_x, phys_y))
}

/// A pointer event in surface-physical pixels, with the surface size and
/// the open-link modifier (Cmd) state.
struct PointerEvent {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub open_modifier: bool,
}

// A scrollbar thumb drag in progress.
static SCROLLBAR_DRAGGING: AtomicBool = AtomicBool::new(false);

// The selection drag began in the history half of an open split, so it
// stays in the history and autoscrolls past the divider (split_drag.rs).
static DRAG_FROM_HISTORY: AtomicBool = AtomicBool::new(false);
// The last point of a selection drag as (x, y, surface height) in
// physical pixels, which each autoscroll tick reads again.
static LAST_DRAG: Mutex<Option<(f64, f64, f64)>> = Mutex::new(None);
// An autoscroll ticker is running.
static AUTOSCROLL_ARMED: AtomicBool = AtomicBool::new(false);

/// True when the point falls in the scrollbar hit zone (right edge) while
/// scrolled. The zone is wider than the drawn bar for forgiving grabs.
fn in_scrollbar_zone(ev: &PointerEvent) -> bool {
    let (offset, scrollback) = crate::native::grid::scroll_metrics();
    if offset == 0 || scrollback == 0 {
        return false;
    }
    let dpr = f64::from(load_f32(&DPR, 2.0));
    ev.width > 0.0 && ev.x >= ev.width - 12.0 * dpr
}

/// Map a scrollbar drag y to an absolute display offset: the thumb center
/// follows the pointer.
// Scrollback lengths are far inside f64's exact-integer range.
#[allow(clippy::cast_precision_loss)]
fn scrollbar_scroll_to(ev: &PointerEvent) {
    let cell_h = f64::from(load_f32(&CELL_H, 0.0));
    if cell_h <= 0.0 || ev.height <= 0.0 {
        return;
    }
    let (_, scrollback) = crate::native::grid::scroll_metrics();
    if scrollback == 0 {
        return;
    }
    let rows = (ev.height / cell_h).floor().max(1.0);
    let total = scrollback as f64 + rows;
    let scroll_top = ((ev.y / ev.height) * total - rows / 2.0).clamp(0.0, scrollback as f64);
    let target = scrollback as f64 - scroll_top;
    crate::native::grid::scroll_to_offset(target.round().max(0.0) as usize);
    redraw_now();
}

/// Middle-click toggles the split: scrolled snaps back to the live tail;
/// at the tail it pages up into scrollback to open the split.
fn middle_click() {
    let (offset, _) = crate::native::grid::scroll_metrics();
    if offset > 0 {
        crate::native::grid::scroll_to_bottom();
    } else {
        crate::native::grid::scroll_page(true);
    }
    redraw_now();
    // A middle click never reaches pointer_up, so it sends the event that
    // gives the command line focus here, as a left click does there.
    if let Some(app) = APP.get() {
        let _ = app.emit(TERMINAL_CLICKED, ());
    }
}

/// A wheel delta forwarded from the page. Positive reveals older lines.
/// The grid scrolls by whole lines and the accumulator keeps the rest.
/// Main thread only.
pub(crate) fn forward_wheel(delta_y: f64) {
    let Ok(mut acc) = SCROLL_ACCUM.lock() else {
        return;
    };
    // Positive deltaY pulls content down = reveal older lines = scroll up.
    *acc += delta_y * 0.12;
    let lines = acc.trunc() as i32;
    *acc -= f64::from(lines);
    drop(acc);
    if lines != 0 {
        crate::native::grid::scroll(lines);
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
    SCROLLBAR_DRAGGING.store(false, Ordering::Release);
    DRAGGING.store(false, Ordering::Release);
    SELECTING.store(false, Ordering::Release);
    DRAG_FROM_HISTORY.store(false, Ordering::Release);
    let cell = phys_point_to_cell(ev.x, ev.y, ev.height);
    if ev.open_modifier {
        if let Some((line, col)) = cell {
            if let Some((url, _, _)) = crate::native::grid::links::url_at(line, col) {
                platform::open_url(&url);
                return;
            }
        }
    }
    if in_scrollbar_zone(ev) {
        SCROLLBAR_DRAGGING.store(true, Ordering::Release);
        scrollbar_scroll_to(ev);
        return;
    }
    // Grab the divider where it is DRAWN (divider_frac), not at the raw
    // ratio, in the same band that shows the page's resize cursor.
    if near_divider(ev) {
        DRAGGING.store(true, Ordering::Release);
        return;
    }
    crate::native::grid::clear_selection();
    if let Some((line, col)) = cell {
        crate::native::grid::start_selection(line, col);
        SELECTING.store(true, Ordering::Release);
        let offset = crate::native::grid::current_display_offset();
        let from_history = surface_frame(ev.height)
            .view(offset)
            .is_some_and(|view| view.in_history(ev.y));
        DRAG_FROM_HISTORY.store(from_history, Ordering::Release);
        redraw_now();
    }
}

/// Move the scrollbar thumb or the divider, or extend the selection,
/// while dragging.
fn pointer_dragged(ev: &PointerEvent) {
    if SCROLLBAR_DRAGGING.load(Ordering::Acquire) {
        scrollbar_scroll_to(ev);
        return;
    }
    if DRAGGING.load(Ordering::Acquire) {
        if ev.height > 0.0 {
            let frac = ev.y / ev.height;
            set_split_ratio((frac.clamp(0.15, 0.85)) as f32);
            redraw_now();
        }
        return;
    }
    if SELECTING.load(Ordering::Acquire) {
        if DRAG_FROM_HISTORY.load(Ordering::Acquire) {
            history_drag(ev);
            return;
        }
        if let Some((line, col)) = phys_point_to_cell(ev.x, ev.y, ev.height) {
            crate::native::grid::update_selection(line, col);
            redraw_now();
        }
    }
}

/// Extend a selection drag that began in the history half. Past the
/// divider it stops at the last history line, and a ticker scrolls the
/// history toward the tail while the pointer stays there.
fn history_drag(ev: &PointerEvent) {
    if let Ok(mut last) = LAST_DRAG.lock() {
        *last = Some((ev.x, ev.y, ev.height));
    }
    let frame = surface_frame(ev.height);
    let scroll =
        crate::native::grid::with_grid_mut(|grid| split_drag::drag(grid, &frame, ev.x, ev.y))
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
    if AUTOSCROLL_ARMED.swap(true, Ordering::AcqRel) {
        return;
    }
    let Some(app) = APP.get().cloned() else {
        AUTOSCROLL_ARMED.store(false, Ordering::Release);
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
                    AUTOSCROLL_ARMED.store(false, Ordering::Release);
                    break;
                }
            }
        }
    });
}

/// One autoscroll tick of a drag from the history half. Runs on the main
/// thread. False, with the ticker's flag cleared, once the drag ended,
/// the pointer came back above the divider, or the split closed at the
/// tail.
fn autoscroll_tick() -> bool {
    let point = LAST_DRAG.lock().ok().and_then(|last| *last);
    let more = match point {
        Some((x, y, height))
            if SELECTING.load(Ordering::Acquire) && DRAG_FROM_HISTORY.load(Ordering::Acquire) =>
        {
            let frame = surface_frame(height);
            let more =
                crate::native::grid::with_grid_mut(|grid| split_drag::tick(grid, &frame, x, y))
                    .unwrap_or(false);
            redraw_now();
            more
        }
        _ => false,
    };
    if !more {
        AUTOSCROLL_ARMED.store(false, Ordering::Release);
    }
    more
}

fn pointer_up() {
    DRAG_FROM_HISTORY.store(false, Ordering::Release);
    let was_scrollbar = SCROLLBAR_DRAGGING.swap(false, Ordering::AcqRel);
    let was_divider = DRAGGING.swap(false, Ordering::AcqRel);
    if was_divider {
        // Divider drag over; redraw so the cursor rect refreshes.
        redraw_now();
    }
    if !was_scrollbar && !was_divider && SELECTING.swap(false, Ordering::AcqRel) {
        // Copy the selection to the clipboard on release.
        copy_selection();
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
    SELECTING.store(false, Ordering::Release);
    DRAG_FROM_HISTORY.store(false, Ordering::Release);
    SCROLLBAR_DRAGGING.store(false, Ordering::Release);
    DRAGGING.store(false, Ordering::Release);
    if let Ok(mut last) = LAST_DRAG.lock() {
        *last = None;
    }
}

/// Track the URL under the pointer so the renderer can underline it. Only
/// repaints when the hovered range actually changes.
fn pointer_moved(ev: Option<&PointerEvent>) {
    let next = ev
        .and_then(|e| phys_point_to_cell(e.x, e.y, e.height))
        .and_then(|(line, col)| {
            crate::native::grid::links::url_at(line, col).map(|(_, s, e)| (line, s, e))
        });
    set_hover_url(next);
}

/// A pointer event forwarded from the page, which sits on top and
/// receives every click. `x` and `y` are CSS px relative to the pane's
/// top-left corner. `kind` is "down", "drag", "up", "move", "leave", or
/// "middle". Must run on the main thread.
pub(crate) fn forward_pointer(kind: &str, x: f64, y: f64, open_modifier: bool) {
    let dpr = f64::from(load_f32(&DPR, 2.0));
    let (width, height) = VIEWPORT
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
    };
    match kind {
        "down" => pointer_down(&ev),
        "drag" => pointer_dragged(&ev),
        "up" => pointer_up(),
        "move" => pointer_moved(Some(&ev)),
        "leave" => pointer_moved(None),
        "middle" => middle_click(),
        _ => {}
    }
    let hint = if kind == "leave" {
        CursorHint::Default
    } else {
        cursor_hint(
            DRAGGING.load(Ordering::Acquire),
            SELECTING.load(Ordering::Acquire) || SCROLLBAR_DRAGGING.load(Ordering::Acquire),
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
    let dpr = f64::from(load_f32(&DPR, 2.0));
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

// The cursor last reported to the page, as a `CursorHint` discriminant.
static CURSOR_HINT: AtomicU8 = AtomicU8::new(CursorHint::Default as u8);

/// Send `vosh://terminal-cursor` with the CSS cursor name, only when it
/// changes.
fn report_cursor(hint: CursorHint) {
    if CURSOR_HINT.swap(hint as u8, Ordering::AcqRel) == hint as u8 {
        return;
    }
    if let Some(app) = APP.get() {
        let _ = app.emit(TERMINAL_CURSOR, hint.css());
    }
}

/// Copy the current selection to the clipboard (no-op when empty) and send
/// the count of characters copied as `vosh://native-copied`, so the page
/// shows the copy toast.
fn copy_selection() {
    let Some(text) = crate::native::grid::selection_text() else {
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

/// Copy the native selection, dispatched to the main thread. Called by the
/// Cmd+C / Ctrl+C path from the frontend.
pub(crate) fn request_copy() {
    let Some(app) = APP.get() else {
        return;
    };
    let _ = app.run_on_main_thread(copy_selection);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pointer event on a 160 by 120 px surface at scale 1.
    fn at(x: f64, y: f64) -> PointerEvent {
        PointerEvent {
            x,
            y,
            width: 160.0,
            height: 120.0,
            open_modifier: false,
        }
    }

    /// Lay the shared grid out as a split at offset `offset`: 10 px rows
    /// on 120 px, L00 to L59 with L48 to L59 on the screen, and the
    /// divider the renderer draws at 79 px. Call with the shared grid
    /// lock held, and `reset_pointer` after.
    fn split_surface(offset: i32) {
        crate::native::grid::blank_shared_grid_for_test(20, 12);
        let text: Vec<String> = (0..60).map(|n| format!("L{n:02}")).collect();
        crate::native::grid::with_grid_mut(|grid| {
            grid.feed(text.join("\r\n").as_bytes());
            grid.scroll(offset);
        });
        store_f32(&CELL_W, 8.0);
        store_f32(&CELL_H, 10.0);
        store_f32(&DPR, 1.0);
        set_divider_frac(Some(79.0 / 120.0));
    }

    fn reset_pointer() {
        SELECTING.store(false, Ordering::Release);
        DRAG_FROM_HISTORY.store(false, Ordering::Release);
        AUTOSCROLL_ARMED.store(false, Ordering::Release);
        *LAST_DRAG.lock().unwrap() = None;
        CELL_W.store(0, Ordering::Release);
        CELL_H.store(0, Ordering::Release);
        DPR.store(0, Ordering::Release);
        set_divider_frac(None);
        crate::native::grid::clear_selection();
    }

    #[test]
    fn a_history_drag_autoscrolls_through_the_divider_into_one_selection() {
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        split_surface(8);
        pointer_down(&at(0.0, 15.0));
        assert!(DRAG_FROM_HISTORY.load(Ordering::Acquire));
        // Past the divider the selection ends on the last history line,
        // L47 at offset 8, and nothing of the live half.
        pointer_dragged(&at(40.0, 100.0));
        let text = crate::native::grid::selection_text().expect("a selection");
        assert_eq!(text.lines().last(), Some("L47"));
        assert_eq!(crate::native::grid::current_display_offset(), 8);
        // Each tick scrolls 7 lines toward the tail, and the second
        // reaches it, which closes the split. The selection runs on to
        // the pointer's cell in the full view.
        assert!(autoscroll_tick());
        assert_eq!(crate::native::grid::current_display_offset(), 1);
        let text = crate::native::grid::selection_text().expect("a selection");
        assert_eq!(text.lines().last(), Some("L54"));
        assert!(!autoscroll_tick());
        assert_eq!(crate::native::grid::current_display_offset(), 0);
        let text = crate::native::grid::selection_text().expect("a selection");
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
        assert!(SELECTING.load(Ordering::Acquire));
        window_blurred();
        assert!(!SELECTING.load(Ordering::Acquire));
        assert!(!DRAG_FROM_HISTORY.load(Ordering::Acquire));
        // The next tick scrolls nothing and stops the ticker, and the
        // split stays open with the selection as it was.
        AUTOSCROLL_ARMED.store(true, Ordering::Release);
        assert!(!autoscroll_tick());
        assert!(!AUTOSCROLL_ARMED.load(Ordering::Acquire));
        assert_eq!(crate::native::grid::current_display_offset(), 8);
        let text = crate::native::grid::selection_text().expect("a selection");
        assert_eq!(text.lines().last(), Some("L47"));
        // A drag event that still comes moves nothing.
        pointer_dragged(&at(40.0, 110.0));
        assert_eq!(crate::native::grid::current_display_offset(), 8);
        let after = crate::native::grid::selection_text().expect("a selection");
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
        assert!(!DRAG_FROM_HISTORY.load(Ordering::Acquire));
        pointer_dragged(&at(0.0, 25.0));
        let text = crate::native::grid::selection_text().expect("a selection");
        assert_eq!(text.lines().next(), Some("L42"));
        assert_eq!(text.lines().last(), Some("L57"));
        assert!(!autoscroll_tick());
        assert_eq!(crate::native::grid::current_display_offset(), 8);
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
