//! Tier 3 native terminal renderer (see docs/native-renderer.md).
//!
//! Platform-agnostic core: the wgpu surface + cell renderer, the shared
//! scroll/split/selection/hover state, and the command-facing API. The
//! platform submodule owns the child window/view plumbing (creating a
//! native view over the webview, moving it, hiding it, clipboard, URL
//! open) and the platform's mouse/cursor handlers:
//!
//! - macOS: `NSView` + `CAMetalLayer` composited over the `WKWebView`,
//!   drawn by wgpu's Metal backend (`macos.rs`).
//! - Windows: a child `HWND` over the `WebView2`, drawn by D3D12
//!   (`windows.rs`).
//! - Linux: a raw X11 child window over the GTK toplevel, drawn by Vulkan;
//!   Wayland falls back to xterm (`linux.rs`).
//!
//! Every window touch happens on the main thread (creation inside
//! `with_webview` / install, updates via `AppHandle::run_on_main_thread`).

#![cfg(native_surface)]
// Platform window plumbing and the wgpu raw-handle surface are unsafe;
// the workspace forbids unsafe by default.
#![allow(unsafe_code)]
// The Linux surface is display-only for now (input propagates through the
// child X window to the webview), so the shared pointer layer sits unused
// there. macOS and Windows still enforce dead-code on it.
#![cfg_attr(target_os = "linux", allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use tauri::{Emitter, Manager};

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

// Live wgpu objects for the terminal surface.
struct GpuState {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    cell_renderer: crate::cell_render::CellRenderer,
}

// The installed surface: the platform's window/view handles plus the GPU
// state. Platform handles are raw pointers and the atlas's font-kit face is
// a platform font object (DirectWrite's is not Send), but every access is
// funnelled through the main thread, so the assertion is sound.
struct SurfaceHandle {
    platform: platform::PlatformSurface,
    gpu: GpuState,
}
unsafe impl Send for SurfaceHandle {}

static SURFACE: OnceLock<Mutex<Option<SurfaceHandle>>> = OnceLock::new();

fn surface_slot() -> &'static Mutex<Option<SurfaceHandle>> {
    SURFACE.get_or_init(|| Mutex::new(None))
}

// App handle for dispatching redraws to the main thread, set at install.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
// True once the frontend has positioned the surface (flag on); keeps
// redraw requests from doing work while the surface is hidden.
static ACTIVE: AtomicBool = AtomicBool::new(false);
// Coalesces redraw requests so a burst of output schedules one repaint.
static REDRAW_PENDING: AtomicBool = AtomicBool::new(false);
// Temporarily hide the opaque surface so a DOM overlay (dropdown, menu,
// modal) that would otherwise be occluded by it shows through. xterm
// renders the same content behind the surface, so the swap is seamless.
static SUPPRESSED: AtomicBool = AtomicBool::new(false);

/// The surface sits BELOW the webview instead of on top (macOS). It spans
/// the whole window and never moves; the grid draws at the pane's offset,
/// the page leaves the pane unpainted so the grid shows through, and DOM
/// overlays composite over live terminal pixels. Pointer input then
/// arrives from the page instead of the view.
pub(crate) const UNDERLAY: bool = cfg!(target_os = "macos");

// The pane rect inside the underlay surface, in device pixels
// [x, y, width, height]. None until the frontend first reports bounds.
static VIEWPORT: Mutex<Option<[u32; 4]>> = Mutex::new(None);

/// The pane rect clamped into a `target_w` x `target_h` render target.
/// Without the underlay (or before the first report) the pane is the
/// whole target.
fn pane_rect(target_w: u32, target_h: u32) -> [u32; 4] {
    let full = [0, 0, target_w.max(1), target_h.max(1)];
    if !UNDERLAY {
        return full;
    }
    let Some([x, y, w, h]) = VIEWPORT.lock().ok().and_then(|v| *v) else {
        return full;
    };
    let x = x.min(target_w.saturating_sub(1));
    let y = y.min(target_h.saturating_sub(1));
    let w = w.min(target_w - x).max(1);
    let h = h.min(target_h - y).max(1);
    [x, y, w, h]
}

/// Hide or show the surface for an overlay. Hiding reveals xterm (same
/// content) so a DOM popover over the terminal is not occluded.
pub(crate) fn set_visible(visible: bool) {
    // Under the underlay the page draws over the grid, so nothing ever
    // needs to hide it.
    if UNDERLAY {
        return;
    }
    SUPPRESSED.store(!visible, Ordering::Release);
    let Some(app) = APP.get() else {
        return;
    };
    let _ = app.run_on_main_thread(move || {
        if let Ok(slot) = surface_slot().lock() {
            if let Some(handle) = slot.as_ref() {
                platform::set_hidden(&handle.platform, !visible);
            }
        }
    });
    if visible {
        request_redraw();
    }
}

/// Request a repaint of the terminal surface. Called from the session loop
/// after feeding the grid. No-ops until the surface is active (and not
/// suppressed); coalesces bursts; dispatches the actual draw to the main
/// thread (Metal requires it).
pub(crate) fn request_redraw() {
    if !ACTIVE.load(Ordering::Acquire) || SUPPRESSED.load(Ordering::Acquire) {
        return;
    }
    let Some(app) = APP.get() else {
        return;
    };
    if REDRAW_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = app.run_on_main_thread(redraw_now);
}

fn redraw_now() {
    REDRAW_PENDING.store(false, Ordering::Release);
    if let Ok(mut slot) = surface_slot().lock() {
        if let Some(handle) = slot.as_mut() {
            // A theme change repaints through here, so the backdrop that
            // shows during a resize follows it. Cached, so cheap.
            #[cfg(target_os = "macos")]
            if UNDERLAY {
                platform::set_backdrop(&handle.platform, crate::cell_render::theme_bg_rgb());
            }
            render(&mut handle.gpu);
            platform::after_redraw(&handle.platform);
        }
    }
}

// The rows at the bottom of the pane the pinned prompt band borrows while
// your prompt takes more than one row, from the page's latest bounds
// report. Stored with the bounds, so a frame never pairs new bounds with
// an old count.
static LENT_ROWS: AtomicU32 = AtomicU32::new(0);

// Last (cols << 16 | rows) the grid took, which the page's hidden xterm
// follows, and last the game was told through NAWS. Each goes out only
// when it changes.
static LAST_GRID_SIZE: AtomicU32 = AtomicU32::new(0);
static LAST_GAME_SIZE: AtomicU32 = AtomicU32::new(0);

/// The rows the grid takes and the rows the game is told, for a pane whose
/// surface fits `fit` rows while the pinned prompt band borrows `lent`.
/// Under the underlay the surface keeps the whole pane and the grid gives
/// the rows up from its top, so its newest line sits right above the band.
/// Elsewhere the surface itself stops short of the band, so `fit` already
/// leaves them out. Either way the game is told the rows the pane holds
/// with a one row band. A fight that grows the band by a row only moves
/// the text, so the game hears of no new size and wraps as before.
/// `keptRows` and `gameSize` in src/lib/terminalRows.ts do the same for
/// xterm, and both run fixtures/terminal-rows/cases.json, so keep them in
/// step.
fn grid_and_game_rows(fit: usize, lent: usize, underlay: bool) -> (usize, usize) {
    if underlay {
        (fit.saturating_sub(lent).max(1), fit)
    } else {
        (fit, fit + lent)
    }
}

/// The device pixels of a surface `full` tall that stops `lent` rows of
/// `cell` px short of the pane's bottom, where the band reaches up. At
/// least one.
fn short_of_band(full: u32, lent: u32, cell: u32) -> u32 {
    full.saturating_sub(lent.saturating_mul(cell)).max(1)
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
fn report_sizes(cols: usize, rows: usize, game_rows: usize) {
    let cols = clamp_u16(cols);
    let rows = clamp_u16(rows);
    let game_rows = clamp_u16(game_rows);
    let grid_news = changed(&LAST_GRID_SIZE, cols, rows);
    let game_news = changed(&LAST_GAME_SIZE, cols, game_rows);
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
        let _ = app.emit("vosh://native-grid-size", (cols, rows));
    }
    if !game_news {
        return;
    }
    let state = app.state::<crate::commands::SharedState>();
    if let Ok(mut ws) = state.window_size.lock() {
        *ws = (cols, game_rows);
    }
    tell_session(state.inner(), &LAST_GAME_SIZE);
}

/// Tell the live session the game's size that `newest` holds, packed as
/// `changed` keeps it. The frame runs on the main thread, so it never
/// waits on the session lock. When a command holds the lock, a task waits
/// for it instead and then sends the size that is newest by then. The
/// frames after this one see no new size, so without the task the game
/// would wrap at the old width until the window changed again.
fn tell_session(state: &crate::commands::SharedState, newest: &'static AtomicU32) {
    if let Ok(session) = state.session.try_lock() {
        send_game_size(session.as_ref(), newest);
        return;
    }
    let state = Arc::clone(state);
    tauri::async_runtime::spawn(async move {
        let session = state.session.lock().await;
        send_game_size(session.as_ref(), newest);
    });
}

/// Hand the live session, if there is one, the size `newest` holds.
fn send_game_size(session: Option<&crate::session::SessionHandle>, newest: &AtomicU32) {
    if let Some(handle) = session {
        let packed = newest.load(Ordering::Acquire);
        handle.set_window_size((packed >> 16) as u16, packed as u16);
    }
}

// The scroll state last reported to the page, as a `scroll_report_key`.
// Starts at a value no key reaches, so the first frame reports.
static LAST_SCROLL: AtomicU64 = AtomicU64::new(u64::MAX);

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
/// `vosh://native-scroll` `[offset, max]` on every platform. The page
/// learns from it whether the scrollback split is open, and under the
/// underlay it also draws the scroll depth, since the surface draws no
/// pill there. Only fires when `scroll_report_key` changes, so the live
/// tail reports once as `[0, max]` and then stays quiet.
fn report_scroll_if_changed() {
    let (offset, max) = crate::term_grid::scroll_metrics();
    let key = scroll_report_key(offset, max);
    if LAST_SCROLL.swap(key, Ordering::AcqRel) == key {
        return;
    }
    if let Some(app) = APP.get() {
        let _ = app.emit("vosh://native-scroll", (offset, max));
    }
}

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

fn set_divider_frac(frac: Option<f32>) {
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
static DPR: AtomicU32 = AtomicU32::new(0);
static CELL_W: AtomicU32 = AtomicU32::new(0);
static CELL_H: AtomicU32 = AtomicU32::new(0);
// The surface's origin in the webview's CSS coordinate space (set with
// the bounds), so a right-click can be reported at its viewport position.
static ORIGIN_X: AtomicU32 = AtomicU32::new(0);
static ORIGIN_Y: AtomicU32 = AtomicU32::new(0);
// xterm's reported device cell size. When set, the atlas uses it instead of
// deriving from font metrics, so the surface matches xterm's density exactly
// (0 = unset, fall back to the font's metrics).
static XTERM_CELL_W: AtomicU32 = AtomicU32::new(0);
static XTERM_CELL_H: AtomicU32 = AtomicU32::new(0);
// xterm's device glyph box height, reported with the cell. The cell is the
// box times the line height, and xterm centers the box in it, so the atlas
// needs both to put its baseline where xterm's is (0 = unset).
static XTERM_CHAR_H: AtomicU32 = AtomicU32::new(0);

/// xterm's reported device cell size, if the frontend has sent it. The glyph
/// atlas sizes its cells to this so spacing matches the webview.
pub(crate) fn reported_cell() -> Option<(u32, u32)> {
    let w = XTERM_CELL_W.load(Ordering::Acquire);
    let h = XTERM_CELL_H.load(Ordering::Acquire);
    if w > 0 && h > 0 {
        Some((w, h))
    } else {
        None
    }
}

/// xterm's reported device glyph box height, if the frontend sent one with
/// the cell size.
pub(crate) fn reported_char_height() -> Option<u32> {
    let h = XTERM_CHAR_H.load(Ordering::Acquire);
    (h > 0).then_some(h)
}

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

fn store_f32(slot: &AtomicU32, value: f32) {
    slot.store(value.to_bits(), Ordering::Release);
}

fn load_f32(slot: &AtomicU32, default: f32) -> f32 {
    let bits = slot.load(Ordering::Acquire);
    if bits == 0 {
        default
    } else {
        f32::from_bits(bits)
    }
}

/// Map a physical-pixel point inside the surface to a grid cell
/// `(line, col)`, mirroring the renderer's split mapping: the bottom (live)
/// region of an open split reads at offset 0, the top (history) region at
/// the scroll offset. `height_px` is the surface height in physical pixels.
fn phys_point_to_cell(phys_x: f64, phys_y: f64, height_px: f64) -> Option<(i32, usize)> {
    let cell_w = f64::from(load_f32(&CELL_W, 0.0));
    let cell_h = f64::from(load_f32(&CELL_H, 0.0));
    if cell_w <= 0.0 || cell_h <= 0.0 {
        return None;
    }
    let col = (phys_x / cell_w).floor().max(0.0) as usize;
    let rows = (height_px / cell_h).floor() as i32;
    let offset = crate::term_grid::current_display_offset() as i32;
    // Mirror the renderer's pixel-smooth split: above the drawn divider is
    // history at the scroll offset; below it are live rows at their
    // absolute top-aligned positions (identical to the non-split view).
    let split = offset > 0 && rows >= 6;
    let row = (phys_y / cell_h).floor().max(0.0) as i32;
    if split {
        if let Some(frac) = divider_frac() {
            let divider_px = f64::from(frac) * height_px;
            if phys_y >= divider_px {
                return Some((row, col));
            }
        }
    }
    Some((row - offset, col))
}

/// A pointer event in surface-physical pixels, with the surface size and
/// the platform's open-link modifier (Cmd / Ctrl) state.
pub(super) struct PointerEvent {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub open_modifier: bool,
}

// A scrollbar thumb drag in progress.
static SCROLLBAR_DRAGGING: AtomicBool = AtomicBool::new(false);

/// True when the point falls in the scrollbar hit zone (right edge) while
/// scrolled. The zone is wider than the drawn bar for forgiving grabs.
fn in_scrollbar_zone(ev: &PointerEvent) -> bool {
    let (offset, scrollback) = crate::term_grid::scroll_metrics();
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
    let (_, scrollback) = crate::term_grid::scroll_metrics();
    if scrollback == 0 {
        return;
    }
    let rows = (ev.height / cell_h).floor().max(1.0);
    let total = scrollback as f64 + rows;
    let scroll_top = ((ev.y / ev.height) * total - rows / 2.0).clamp(0.0, scrollback as f64);
    let target = scrollback as f64 - scroll_top;
    crate::term_grid::scroll_to_offset(target.round().max(0.0) as usize);
    redraw_now();
}

/// Middle-click toggles the split: scrolled snaps back to the live tail;
/// at the tail it pages up into scrollback to open the split.
fn middle_click() {
    let (offset, _) = crate::term_grid::scroll_metrics();
    if offset > 0 {
        crate::term_grid::scroll_to_bottom();
    } else {
        crate::term_grid::scroll_page(true);
    }
    redraw_now();
    // A middle-click still pulls key focus off the command input like
    // any click on the surface, but it arrives as otherMouseDown /
    // WM_MBUTTONDOWN and never reaches pointer_up, so the refocus
    // event has to fire here too. Without it, Enter stops resending
    // the highlighted command and macros go dead after closing the
    // split.
    if let Some(app) = APP.get() {
        let _ = app.emit("vosh://terminal-clicked", ());
    }
}

/// Accumulate a wheel delta (positive = reveal older lines) and scroll the
/// grid by whole lines. Shared by every platform's wheel handler.
fn wheel_scroll(delta_y: f64) {
    let Ok(mut acc) = SCROLL_ACCUM.lock() else {
        return;
    };
    // Positive deltaY pulls content down = reveal older lines = scroll up.
    *acc += delta_y * 0.12;
    let lines = acc.trunc() as i32;
    *acc -= f64::from(lines);
    drop(acc);
    if lines != 0 {
        crate::term_grid::scroll(lines);
        redraw_now();
    }
}

/// Cmd/Ctrl+click on a URL opens it; a press in the scrollbar zone starts
/// a thumb drag; a press on the divider starts a divider drag; anything
/// else starts a selection.
fn pointer_down(ev: &PointerEvent) {
    // A press always starts fresh. Forwarded input cannot promise that
    // every press got its release, and a stale flag would turn the next
    // selection into a divider or scrollbar drag.
    SCROLLBAR_DRAGGING.store(false, Ordering::Release);
    DRAGGING.store(false, Ordering::Release);
    SELECTING.store(false, Ordering::Release);
    let cell = phys_point_to_cell(ev.x, ev.y, ev.height);
    if ev.open_modifier {
        if let Some((line, col)) = cell {
            if let Some((url, _, _)) = crate::term_grid::url_at(line, col) {
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
    crate::term_grid::clear_selection();
    if let Some((line, col)) = cell {
        crate::term_grid::start_selection(line, col);
        SELECTING.store(true, Ordering::Release);
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
        if let Some((line, col)) = phys_point_to_cell(ev.x, ev.y, ev.height) {
            crate::term_grid::update_selection(line, col);
            redraw_now();
        }
    }
}

fn pointer_up() {
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
    // other part of the window. The opaque surface eats the DOM mouseup
    // that used to do this, so the frontend listens for the event instead.
    if let Some(app) = APP.get() {
        let _ = app.emit("vosh://terminal-clicked", ());
    }
}

/// Right-click. The opaque surface eats the DOM contextmenu event, so the
/// pointer's position is forwarded in webview CSS coordinates (surface
/// origin + the event's surface-local point) and the frontend opens its
/// terminal context menu there.
fn context_click(ev: &PointerEvent) {
    let dpr = f64::from(load_f32(&DPR, 2.0));
    if dpr <= 0.0 {
        return;
    }
    let x = f64::from(load_f32(&ORIGIN_X, 0.0)) + ev.x / dpr;
    let y = f64::from(load_f32(&ORIGIN_Y, 0.0)) + ev.y / dpr;
    if let Some(app) = APP.get() {
        let _ = app.emit("vosh://terminal-context-menu", (x, y));
    }
}

/// Track the URL under the pointer so the renderer can underline it. Only
/// repaints when the hovered range actually changes.
fn pointer_moved(ev: Option<&PointerEvent>) {
    let next = ev
        .and_then(|e| phys_point_to_cell(e.x, e.y, e.height))
        .and_then(|(line, col)| crate::term_grid::url_at(line, col).map(|(_, s, e)| (line, s, e)));
    set_hover_url(next);
}

/// A pointer event forwarded from the page under the underlay, where the
/// webview sits on top and receives every click. `x` and `y` are CSS px
/// relative to the pane's top-left corner. `kind` is "down", "drag",
/// "up", "move", "leave", or "middle". Must run on the main thread.
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
/// it starts a divider drag, and under the underlay the page shows the
/// resize cursor across the same band.
const DIVIDER_GRAB_PT: f64 = 8.0;

/// True when the point sits on the divider as drawn, within the grab band.
fn near_divider(ev: &PointerEvent) -> bool {
    let Some(drawn) = divider_frac() else {
        return false;
    };
    let dpr = f64::from(load_f32(&DPR, 2.0));
    ev.height > 0.0 && (ev.y - f64::from(drawn) * ev.height).abs() <= DIVIDER_GRAB_PT * dpr
}

/// The pointer cursor the page should show over the pane. Under the
/// underlay the page owns the cursor, so the surface reports what the
/// pointer is over and the page sets it.
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
        let _ = app.emit("vosh://terminal-cursor", hint.css());
    }
}

/// True once the surface installed and its GPU came up. The page checks
/// this before it leaves the terminal pane transparent, so a failed
/// install falls back to xterm instead of a see-through hole.
pub(crate) fn is_ready() -> bool {
    surface_slot().lock().is_ok_and(|s| s.is_some())
}

/// A wheel delta forwarded from the page under the underlay. Positive
/// reveals older lines, matching the platform handlers. Main thread only.
pub(crate) fn forward_wheel(delta_y: f64) {
    wheel_scroll(delta_y);
}

// The transient "copied N chars" toast: text plus the moment it was set.
// Cleared by a delayed task; the renderer reads it via `copy_notice`.
static COPY_NOTICE: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);
static COPY_NOTICE_GEN: AtomicU32 = AtomicU32::new(0);
const COPY_NOTICE_MS: u64 = 1600;

/// The active copy toast text, if one is showing. Read by the renderer,
/// which draws it as a pill in the bottom-right of the surface.
pub(crate) fn copy_notice() -> Option<String> {
    let guard = COPY_NOTICE.lock().ok()?;
    let (text, at) = guard.as_ref()?;
    (at.elapsed().as_millis() < u128::from(COPY_NOTICE_MS)).then(|| text.clone())
}

/// Copy the current selection to the clipboard (no-op when empty) and show
/// the "copied N chars" toast for a moment so the copy is visibly
/// confirmed. Under the underlay the page shows the toast, so the count
/// goes out as `vosh://native-copied` instead.
fn copy_selection() {
    let Some(text) = crate::term_grid::selection_text() else {
        return;
    };
    if text.is_empty() {
        return;
    }
    platform::set_clipboard(&text);
    let chars = text.chars().count();
    if UNDERLAY {
        if let Some(app) = APP.get() {
            let _ = app.emit("vosh://native-copied", chars);
        }
        return;
    }
    let plural = if chars == 1 { "" } else { "s" };
    if let Ok(mut guard) = COPY_NOTICE.lock() {
        *guard = Some((
            format!("copied {chars} char{plural}"),
            std::time::Instant::now(),
        ));
    }
    let gen = COPY_NOTICE_GEN.fetch_add(1, Ordering::AcqRel) + 1;
    redraw_now();
    // Clear the toast after it expires (unless a newer copy replaced it)
    // and repaint so it actually disappears without waiting for output.
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(COPY_NOTICE_MS)).await;
        if COPY_NOTICE_GEN.load(Ordering::Acquire) == gen {
            if let Ok(mut guard) = COPY_NOTICE.lock() {
                *guard = None;
            }
            request_redraw();
        }
    });
}

/// Copy the native selection, dispatched to the main thread. Called by the
/// Cmd+C / Ctrl+C path from the frontend.
pub(crate) fn request_copy() {
    let Some(app) = APP.get() else {
        return;
    };
    let _ = app.run_on_main_thread(copy_selection);
}

/// Swap in a renderer built from `fonts` and repaint. Runs on the main
/// thread. It keeps the surface and device. The fonts arrive loaded from
/// the blocking pool, so all that is left here is rasterizing the ASCII
/// glyphs and uploading the atlas.
fn swap_font(fonts: crate::cell_render::AtlasFonts, font_px: f32) {
    if let Ok(mut slot) = surface_slot().lock() {
        if let Some(handle) = slot.as_mut() {
            handle.gpu.cell_renderer = crate::cell_render::CellRenderer::with_fonts(
                &handle.gpu.device,
                &handle.gpu.queue,
                handle.gpu.config.format,
                fonts,
                font_px,
            );
            render(&mut handle.gpu);
        }
    }
}

/// Tickets for font loads. Each request takes the next ticket, and a
/// load swaps its fonts in only while its ticket is still the newest, so
/// when requests overlap the last one wins, whatever order their loads
/// finish in.
struct LatestTicket(AtomicU64);

impl LatestTicket {
    const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    fn take(&self) -> u64 {
        self.0.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn is_newest(&self, ticket: u64) -> bool {
        self.0.load(Ordering::Acquire) == ticket
    }
}

static FONT_TICKETS: LatestTicket = LatestTicket::new();

/// Run `load` on the blocking pool, then hand its result to `apply`
/// through `on_main`, which runs the swap on the main thread. A request
/// that a newer one overtakes stops at the next check, before the load,
/// after it, or on the main thread just before `apply`. A load that
/// returns None applies nothing.
fn load_latest<T: Send + 'static>(
    tickets: &'static LatestTicket,
    load: impl FnOnce() -> Option<T> + Send + 'static,
    on_main: impl FnOnce(Box<dyn FnOnce() + Send>) + Send + 'static,
    apply: impl FnOnce(T) + Send + 'static,
) {
    let ticket = tickets.take();
    tauri::async_runtime::spawn_blocking(move || {
        if !tickets.is_newest(ticket) {
            return;
        }
        let Some(value) = load() else {
            return;
        };
        if !tickets.is_newest(ticket) {
            return;
        }
        on_main(Box::new(move || {
            if tickets.is_newest(ticket) {
                apply(value);
            }
        }));
    });
}

/// Rebuild the atlas at `family` and `font_px`. font-kit resolves the
/// family and loads its faces on the blocking pool, which took 3 to
/// 27 ms for a typical family and 300 to 665 ms for a large CJK family,
/// and only the atlas swap runs on the main thread. The newest request
/// wins.
fn request_font_rebuild(family: String, font_px: f32) {
    let Some(app) = APP.get() else {
        return;
    };
    let app = app.clone();
    load_latest(
        &FONT_TICKETS,
        move || crate::cell_render::FontsInTransit::load(&family),
        move |swap| {
            let _ = app.run_on_main_thread(swap);
        },
        move |fonts| {
            if let Some(fonts) = fonts.arrive() {
                swap_font(fonts, font_px);
            }
        },
    );
}

/// Re-create the atlas at the configured font/size (CSS px * scale).
/// Called when the font setting changes.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn request_set_font(family: String, font_size: u32) {
    let font_px = (font_size as f32 * load_f32(&DPR, 2.0)).max(6.0);
    request_font_rebuild(family, font_px);
}

/// Record xterm's device cell size and glyph box height and rebuild the
/// atlas to them, so the surface matches the webview's spacing and
/// baseline exactly. A `char_height` of 0 means the page did not send
/// one. No-op if unchanged.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn set_cell_metrics(width: u32, height: u32, char_height: u32) {
    if width == 0 || height == 0 {
        return;
    }
    // Swap all three unconditionally; a short-circuiting && would skip a
    // later store and leave that value unset.
    let prev_w = XTERM_CELL_W.swap(width, Ordering::AcqRel);
    let prev_h = XTERM_CELL_H.swap(height, Ordering::AcqRel);
    let prev_char = XTERM_CHAR_H.swap(char_height, Ordering::AcqRel);
    if prev_w == width && prev_h == height && prev_char == char_height {
        return;
    }
    tracing::debug!(
        width,
        height,
        char_height,
        "native-surface: xterm reported cell metrics"
    );
    let scale = f64::from(load_f32(&DPR, 2.0));
    let (family, font_px) = font_atlas_params(scale);
    request_font_rebuild(family, font_px);
}

/// The configured terminal font family stack and the atlas pixel size.
/// Falls back to the system monospace at 14 CSS px. `scale` is the backing
/// scale factor, so the returned px is physical pixels (crisp at retina)
/// and the glyphs match the xterm font size.
#[allow(clippy::cast_precision_loss)]
fn font_atlas_params(scale: f64) -> (String, f32) {
    let size_for = |css: f32| (css * scale as f32).max(6.0);
    let Some(app) = APP.get() else {
        return ("monospace".to_string(), size_for(14.0));
    };
    let state = app.state::<crate::commands::SharedState>();
    let guard = state.profile.try_lock();
    if let Ok(p) = guard {
        (p.ui.font_family.clone(), size_for(p.ui.font_size as f32))
    } else {
        ("monospace".to_string(), size_for(14.0))
    }
}

/// Install the native surface over the main window's content view.
/// Best-effort: logs and returns on any missing handle. Runs on the main
/// thread. The surface starts at a placeholder frame; the frontend's first
/// `set_bounds` call snaps it to the terminal pane.
pub(crate) fn install_probe(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    let _ = APP.set(window.app_handle().clone());
    platform::install(window)
}

/// Reposition and resize the surface to the terminal pane. `x`/`y`/`w`/`h`
/// are CSS pixels in the webview's top-left coordinate space; `dpr` is the
/// device pixel ratio. `lent` is the rows at the pane's bottom the pinned
/// prompt band borrows (`grid_and_game_rows`). Must run on the main thread.
pub(crate) fn set_bounds(x: f64, y: f64, width: f64, height: f64, dpr: f64, lent: u32) {
    let Ok(mut slot) = surface_slot().lock() else {
        return;
    };
    let Some(handle) = slot.as_mut() else {
        return;
    };
    if width < 1.0 || height < 1.0 {
        return;
    }
    // The surface is now positioned and visible, so live output should
    // trigger repaints.
    ACTIVE.store(true, Ordering::Release);
    LENT_ROWS.store(lent, Ordering::Release);
    store_f32(&DPR, dpr as f32);
    store_f32(&ORIGIN_X, x as f32);
    store_f32(&ORIGIN_Y, y as f32);
    // The underlay helpers exist only in the macOS glue.
    #[cfg(target_os = "macos")]
    if UNDERLAY {
        // The view already spans the window (AppKit resizes it with the
        // window), so the report only moves the grid inside it. Snap to
        // whole device pixels so glyphs land on the pixel grid.
        let snap = |v: f64| (v * dpr).round().max(0.0) as u32;
        if let Ok(mut vp) = VIEWPORT.lock() {
            *vp = Some([snap(x), snap(y), snap(width).max(1), snap(height).max(1)]);
        }
        let (px_w, px_h) = platform::view_size_px(&handle.platform, dpr);
        let (px_w, px_h) = clamp_to_device(&handle.gpu.device, px_w, px_h);
        platform::set_scale(&handle.platform, dpr);
        platform::set_hidden(&handle.platform, false);
        platform::set_backdrop(&handle.platform, crate::cell_render::theme_bg_rgb());
        // The window reports its real radius once it is on screen, and a
        // fullscreen switch also resizes the pane, so re-check here.
        platform::sync_corner_radius(&handle.platform);
        if px_w != handle.gpu.config.width || px_h != handle.gpu.config.height {
            handle.gpu.config.width = px_w;
            handle.gpu.config.height = px_h;
            handle
                .gpu
                .surface
                .configure(&handle.gpu.device, &handle.gpu.config);
        }
        render(&mut handle.gpu);
        return;
    }
    // The surface sits over the page here, so it stops short of the rows
    // the band borrows, or it would hide them.
    let cell_px = handle.gpu.cell_renderer.cell_size_px().1.round() as u32;
    let px_w = (width * dpr).max(1.0) as u32;
    let px_h = short_of_band((height * dpr).max(1.0) as u32, lent, cell_px);
    let height = f64::from(px_h) / dpr;
    platform::set_frame(&handle.platform, x, y, width, height, dpr);
    // Respect an active overlay suppression so a resize does not pop the
    // surface back over an open dropdown.
    platform::set_hidden(&handle.platform, SUPPRESSED.load(Ordering::Acquire));

    let (px_w, px_h) = clamp_to_device(&handle.gpu.device, px_w, px_h);
    if px_w != handle.gpu.config.width || px_h != handle.gpu.config.height {
        handle.gpu.config.width = px_w;
        handle.gpu.config.height = px_h;
        handle
            .gpu
            .surface
            .configure(&handle.gpu.device, &handle.gpu.config);
    }
    render(&mut handle.gpu);
}

/// Clamp a drawable size to the device's texture limit. `configure` panics
/// on anything larger, and that panic would abort the app from inside an
/// `AppKit` callback.
fn clamp_to_device(device: &wgpu::Device, width: u32, height: u32) -> (u32, u32) {
    let max = device.limits().max_texture_dimension_2d.max(1);
    (width.clamp(1, max), height.clamp(1, max))
}

/// Build the wgpu surface + device + cell renderer over the platform's raw
/// window handle. `backends` is the platform's preferred wgpu backend.
unsafe fn init_gpu(
    raw_window_handle: RawWindowHandle,
    raw_display_handle: RawDisplayHandle,
    backends: wgpu::Backends,
    width: u32,
    height: u32,
    font_stack: &str,
    font_px: f32,
) -> Option<GpuState> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });

    let surface = instance
        .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle,
            raw_window_handle,
        })
        .map_err(|e| tracing::warn!(error = %e, "native-surface: create_surface failed"))
        .ok()?;

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))?;

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            // The underlay drawable spans the window, which can pass the
            // default 8192 px cap on a window stretched across displays.
            // Ask for what the GPU actually supports (16384 on Apple).
            required_limits: adapter.limits(),
            ..Default::default()
        },
        None,
    ))
    .map_err(|e| tracing::warn!(error = %e, "native-surface: request_device failed"))
    .ok()?;

    let (width, height) = clamp_to_device(&device, width, height);
    let mut config = surface.get_default_config(&adapter, width, height)?;
    // Use the non-sRGB view of the format. The cell renderer writes
    // sRGB-encoded values and relies on hardware alpha blending compositing
    // in that (gamma) space so glyph antialiasing matches the webview; an
    // sRGB surface would blend in linear space and make text look heavy.
    config.format = config.format.remove_srgb_suffix();
    surface.configure(&device, &config);

    let cell_renderer =
        crate::cell_render::CellRenderer::new(&device, &queue, config.format, font_stack, font_px)?;

    Some(GpuState {
        _instance: instance,
        surface,
        device,
        queue,
        config,
        cell_renderer,
    })
}

fn render(state: &mut GpuState) {
    let frame = match state.surface.get_current_texture() {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "native-surface: get_current_texture failed");
            return;
        }
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = state
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // Size the grid to the pane so the terminal fills it instead of a fixed
    // 80x24 corner. Under the underlay the pane is a rect inside the
    // window-sized target; otherwise it is the whole target.
    let [pane_x, pane_y, pane_w, pane_h] = pane_rect(state.config.width, state.config.height);
    let (cols, fit) = state.cell_renderer.grid_size_for(pane_w, pane_h);
    let lent = LENT_ROWS.load(Ordering::Acquire) as usize;
    let (rows, game_rows) = grid_and_game_rows(fit, lent, UNDERLAY);
    crate::term_grid::resize_grid(cols, rows);
    report_sizes(cols, rows, game_rows);
    // Publish the cell size so the mouse handler can map points to cells.
    let (cw, ch) = state.cell_renderer.cell_size_px();
    store_f32(&CELL_W, cw);
    store_f32(&CELL_H, ch);

    // Disjoint borrows of GpuState fields so the grid-reading closure can
    // hold the renderer mutably and the device/queue immutably.
    let device = &state.device;
    let queue = &state.queue;
    let placement = crate::cell_render::Placement {
        x: pane_x,
        y: pane_y,
        vignette: !UNDERLAY,
        indicators: !UNDERLAY,
        scale: load_f32(&DPR, 2.0),
        target: [state.config.width, state.config.height],
    };
    let cell_renderer = &mut state.cell_renderer;
    let drew = crate::term_grid::with_grid(|grid| {
        if let Some(grid) = grid {
            let frac = cell_renderer.draw(
                device,
                queue,
                &mut encoder,
                &view,
                grid,
                pane_w,
                pane_h,
                split_ratio(),
                placement,
            );
            set_divider_frac(frac);
            true
        } else {
            false
        }
    });
    if !drew {
        // No grid yet: clear to the terminal background, since under the
        // underlay this fills the whole window behind the page. The pass
        // records its clear when dropped at the end of this block.
        let (bg_r, bg_g, bg_b) = crate::cell_render::theme_bg_rgb();
        let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("term-surface-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(bg_r) / 255.0,
                        g: f64::from(bg_g) / 255.0,
                        b: f64::from(bg_b) / 255.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    state.queue.submit(Some(encoder.finish()));
    frame.present();
    // Every scroll path repaints through here, and the grid lock is free
    // again, so this is where the page hears about the new offset.
    report_scroll_if_changed();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::Duration;

    /// Tickets of its own for each test, so tests that run at once never
    /// overtake each other.
    fn tickets() -> &'static LatestTicket {
        Box::leak(Box::new(LatestTicket::new()))
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn a_slow_font_load_never_replaces_a_newer_font() {
        let tickets = tickets();
        let (applied_tx, applied) = mpsc::channel();
        let (started_tx, started) = mpsc::channel();
        let (release_tx, release) = mpsc::channel::<()>();
        let slow = applied_tx.clone();
        load_latest(
            tickets,
            move || {
                started_tx.send(()).ok();
                release.recv().ok();
                Some("slow")
            },
            |swap| swap(),
            move |fonts| slow.send(fonts).unwrap(),
        );
        started.recv_timeout(WAIT).expect("the slow load starts");
        load_latest(
            tickets,
            || Some("fast"),
            |swap| swap(),
            move |fonts| applied_tx.send(fonts).unwrap(),
        );
        assert_eq!(applied.recv_timeout(WAIT), Ok("fast"));
        release_tx.send(()).unwrap();
        // Every sender went away with its request, and the slow one
        // applied nothing on the way out.
        assert_eq!(
            applied.recv_timeout(WAIT),
            Err(RecvTimeoutError::Disconnected)
        );
    }

    #[test]
    fn a_swap_queued_for_the_main_thread_drops_when_a_newer_font_arrives() {
        let tickets = tickets();
        let (applied_tx, applied) = mpsc::channel();
        let (queued_tx, queued) = mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let first = applied_tx.clone();
        load_latest(
            tickets,
            || Some("first"),
            move |swap| queued_tx.send(swap).unwrap(),
            move |fonts| first.send(fonts).unwrap(),
        );
        let late_swap = queued.recv_timeout(WAIT).expect("the first swap queues");
        load_latest(
            tickets,
            || Some("second"),
            |swap| swap(),
            move |fonts| applied_tx.send(fonts).unwrap(),
        );
        assert_eq!(applied.recv_timeout(WAIT), Ok("second"));
        late_swap();
        assert_eq!(
            applied.recv_timeout(WAIT),
            Err(RecvTimeoutError::Disconnected)
        );
    }

    #[test]
    fn the_only_font_request_swaps_in_and_a_failed_load_swaps_nothing() {
        let tickets = tickets();
        let (applied_tx, applied) = mpsc::channel();
        load_latest(
            tickets,
            || Some("only"),
            |swap| swap(),
            move |fonts| applied_tx.send(fonts).unwrap(),
        );
        assert_eq!(applied.recv_timeout(WAIT), Ok("only"));
        let (failed_tx, failed) = mpsc::channel::<&str>();
        load_latest(
            tickets,
            || None,
            |swap| swap(),
            move |fonts| failed_tx.send(fonts).unwrap(),
        );
        assert_eq!(
            failed.recv_timeout(WAIT),
            Err(RecvTimeoutError::Disconnected)
        );
    }

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

    #[test]
    fn scroll_key_never_reaches_the_unset_marker() {
        assert_ne!(scroll_report_key(usize::MAX, usize::MAX - 1), u64::MAX);
        assert_ne!(scroll_report_key(1, usize::MAX), u64::MAX);
    }

    /// The grid rows and the rows the game hears of, frame by frame, for
    /// a pane that fits `fit` rows while the band borrows `lent`.
    fn reported(frames: &[(usize, usize)], underlay: bool) -> (Vec<u16>, Vec<u16>) {
        let grid = AtomicU32::new(0);
        let game = AtomicU32::new(0);
        let (mut sized, mut told) = (Vec::new(), Vec::new());
        for &(fit, lent) in frames {
            let (rows, game_rows) = grid_and_game_rows(fit, lent, underlay);
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
        let (sized, told) = reported(&frames, true);
        // The grid gives up its top row to the band and takes it back
        // each time, so the page's hidden xterm follows it.
        assert_eq!(sized, [40, 39, 40, 39, 40, 39, 40, 42, 41]);
        // The game hears the rows the pane holds with a one row band,
        // once, and again only when the window itself changes.
        assert_eq!(told, [40, 42]);
    }

    #[test]
    fn a_surface_that_stops_short_of_the_band_tells_the_game_the_same_rows() {
        // Off the underlay the surface ends above the band, so the rows
        // it fits already leave the borrowed ones out.
        let frames = [(40, 0), (39, 1), (40, 0), (38, 2)];
        let (sized, told) = reported(&frames, false);
        assert_eq!(sized, [40, 39, 40, 38]);
        assert_eq!(told, [40]);
        // A pane 1415 device px tall fits 40 rows of 35 px. Cut short by
        // one borrowed row it fits 39, never 38.
        assert_eq!(short_of_band(1415, 1, 35) / 35, 39);
        assert_eq!(short_of_band(1400, 1, 35) / 35, 39);
        assert_eq!(short_of_band(30, 3, 35), 1);
    }

    #[test]
    fn the_grid_keeps_a_row_whatever_the_band_borrows() {
        assert_eq!(grid_and_game_rows(3, 5, true), (1, 3));
        assert_eq!(grid_and_game_rows(40, 0, true), (40, 40));
        assert_eq!(grid_and_game_rows(40, 0, false), (40, 40));
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
        game_short: Option<usize>,
    }

    impl SplitCase {
        /// The rows the case says the game is told, under the underlay or
        /// short of the band. A pane no taller than what the band borrows
        /// names each place apart, since the two tell the game different
        /// rows there today.
        fn game(&self, underlay: bool) -> usize {
            match (self.game, self.game_underlay, self.game_short) {
                (Some(game), None, None) => game,
                (None, Some(under), Some(short)) => {
                    if underlay {
                        under
                    } else {
                        short
                    }
                }
                _ => panic!("{} needs game, or game_underlay with game_short", self.name),
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
        let text = include_str!("../../../fixtures/terminal-rows/cases.json");
        serde_json::from_str(text).expect("the row cases parse")
    }

    /// The device px a row takes in the cases.
    const CASE_CELL: u32 = 35;

    /// The grid rows and the game rows for a pane that fits `fit` rows
    /// with `spare` px left over while the band borrows `lent`. Under the
    /// underlay the grid gives the rows up itself. Elsewhere the surface
    /// stops short of the band and the grid fits what is left, at least
    /// one row, as `grid_size_for` counts it.
    fn split(fit: usize, lent: usize, underlay: bool, spare: u32) -> (usize, usize) {
        if underlay {
            return grid_and_game_rows(fit, lent, true);
        }
        let full = u32::try_from(fit).unwrap() * CASE_CELL + spare;
        let short = short_of_band(full, u32::try_from(lent).unwrap(), CASE_CELL);
        let fits = usize::try_from(short / CASE_CELL).unwrap().max(1);
        grid_and_game_rows(fits, lent, false)
    }

    /// Each way the surface can sit: under the page, and over it short of
    /// the band with no px, some px, or almost a row left over.
    const PLACES: [(bool, u32); 4] = [(true, 0), (false, 0), (false, 17), (false, CASE_CELL - 1)];

    #[test]
    fn rows_split_as_the_cases_xterm_runs() {
        let cases = row_cases();
        assert!(!cases.split.is_empty());
        for case in &cases.split {
            for (underlay, spare) in PLACES {
                let (grid, game) = split(case.fit, case.lent, underlay, spare);
                let at = format!("{}, underlay {underlay}, {spare} px spare", case.name);
                assert_eq!(grid, case.grid, "{at}");
                assert_eq!(game, case.game(underlay), "{at}");
            }
        }
    }

    /// Runs `changed` on a slot of its own, the way `report_sizes` does
    /// for the game, and never runs `report_sizes` itself, which keeps its
    /// slots in statics and reaches the app. A change to the rows
    /// `report_sizes` hands `changed` passes here unseen.
    #[test]
    fn sizes_reach_the_game_as_the_cases_xterm_runs() {
        let cases = row_cases();
        assert!(!cases.reports.is_empty());
        for case in &cases.reports {
            for (underlay, spare) in PLACES {
                let last = AtomicU32::new(0);
                let (mut grid, mut told) = (Vec::new(), Vec::new());
                for &(cols, fit, lent) in &case.frames {
                    let (rows, game_rows) = split(fit, lent, underlay, spare);
                    grid.push(rows);
                    let game_rows = clamp_u16(game_rows);
                    if changed(&last, cols, game_rows) {
                        told.push((cols, game_rows));
                    }
                }
                let at = format!("{}, underlay {underlay}, {spare} px spare", case.name);
                assert_eq!(grid, case.grid, "{at}");
                assert_eq!(told, case.told, "{at}");
            }
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
        state: crate::commands::SharedState,
        /// The game's end of the socket.
        game: tokio::net::TcpStream,
        /// What the game read so far.
        heard: Vec<u8>,
        /// The mock app the session runs in.
        app: tauri::App<tauri::test::MockRuntime>,
    }

    /// What the game reads when the client says it is `cols` by 40.
    fn naws(cols: u8) -> [u8; 9] {
        use vosh_telnet::codes::{option::NAWS, IAC, SB, SE};
        [IAC, SB, NAWS, 0, cols, 0, 40, IAC, SE]
    }

    async fn sized_game() -> SizedGame {
        use tauri::test::{mock_builder, mock_context, noop_assets};
        use tokio::io::AsyncWriteExt;
        use vosh_telnet::codes::{option::NAWS, DO, IAC};

        use crate::commands::{AppState, SharedState};

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
            "127.0.0.1".into(),
            port,
            false,
            state.profile.clone(),
            state.map.clone(),
            state.script_timers.clone(),
            state.logs.clone(),
            state.scrollback.clone(),
            None,
            (100, 40),
        )
        .await
        .expect("the game answers");
        *state.session.lock().await = Some(handle);
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
    async fn end_session(state: &crate::commands::SharedState) {
        let handle = state.session.lock().await.take();
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
        // The game's size the frames last reported, as `LAST_GAME_SIZE`
        // holds it in the app.
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
                    tell_session(&state, &LAST);
                }
            });
        };
        frame(100);
        // You widen the window while a command holds the session.
        {
            let _busy = state.session.lock().await;
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
        use vosh_telnet::codes::{option::NAWS, IAC, SB};

        static LAST: AtomicU32 = AtomicU32::new(0);
        let SizedGame {
            state,
            mut game,
            mut heard,
            app: _app,
        } = sized_game().await;
        assert!(changed(&LAST, 100, 40));

        // The waiting task holds a clone of the state until it has sent,
        // so the count falls back to this once it is done.
        let idle = Arc::strong_count(&state);
        {
            let busy = state.session.lock().await;
            // You widen the window while a command holds the session.
            assert!(changed(&LAST, 120, 40));
            on_a_plain_thread(|| tell_session(&state, &LAST));
            // You widen it again, and that frame takes the session before
            // the waiting task does.
            assert!(changed(&LAST, 130, 40));
            send_game_size(busy.as_ref(), &LAST);
        }
        let deadline = tokio::time::Instant::now() + WAIT;
        while Arc::strong_count(&state) > idle {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the waiting task never sent"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        // What the session sends next lands behind every size it sent
        // before, so the last size ahead of it is the one the game keeps.
        let marker = b"after the resize";
        let sent = state
            .session
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
