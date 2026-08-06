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

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

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

/// Hide or show the surface for an overlay. Hiding reveals xterm (same
/// content) so a DOM popover over the terminal is not occluded.
pub(crate) fn set_visible(visible: bool) {
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
            render(&mut handle.gpu);
            platform::after_redraw(&handle.platform);
        }
    }
}

// Last (cols << 16 | rows) advertised to the MUD, so NAWS is pushed only
// when the native grid size actually changes (i.e. on window resize).
static LAST_GRID_SIZE: AtomicU32 = AtomicU32::new(0);

/// When the native surface owns the terminal, advertise its grid size to
/// the MUD so lines wrap to fill the pane (instead of the xterm width).
/// Only fires when the size changes.
fn push_native_size_if_changed(cols: usize, rows: usize) {
    let cols = cols.min(usize::from(u16::MAX)) as u16;
    let rows = rows.min(usize::from(u16::MAX)) as u16;
    let packed = (u32::from(cols) << 16) | u32::from(rows);
    if LAST_GRID_SIZE.swap(packed, Ordering::AcqRel) == packed {
        return;
    }
    let Some(app) = APP.get() else {
        return;
    };
    let state = app.state::<crate::commands::SharedState>();
    if let Ok(mut ws) = state.window_size.lock() {
        *ws = (cols, rows);
    }
    // Tell the frontend so it can size hidden xterm to the same grid; when a
    // DOM overlay reveals xterm it then matches the surface exactly.
    let _ = app.emit("vosh://native-grid-size", (cols, rows));
    // Bound to a named local (not a temporary) so the guard drops before
    // `state` at end of scope.
    let session_guard = state.session.try_lock();
    if let Ok(session) = session_guard {
        if let Some(handle) = session.as_ref() {
            handle.set_window_size(cols, rows);
        }
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
// xterm's reported device cell size. When set, the atlas uses it instead of
// deriving from font metrics, so the surface matches xterm's density exactly
// (0 = unset, fall back to the font's metrics).
static XTERM_CELL_W: AtomicU32 = AtomicU32::new(0);
static XTERM_CELL_H: AtomicU32 = AtomicU32::new(0);

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
    // ratio. The band is a bit wider than the cursor rect for forgiving
    // grabs.
    if let Some(drawn) = divider_frac() {
        let dpr = f64::from(load_f32(&DPR, 2.0));
        if ev.height > 0.0 && (ev.y - f64::from(drawn) * ev.height).abs() <= 8.0 * dpr {
            DRAGGING.store(true, Ordering::Release);
            return;
        }
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

/// Track the URL under the pointer so the renderer can underline it. Only
/// repaints when the hovered range actually changes.
fn pointer_moved(ev: Option<&PointerEvent>) {
    let next = ev
        .and_then(|e| phys_point_to_cell(e.x, e.y, e.height))
        .and_then(|(line, col)| crate::term_grid::url_at(line, col).map(|(_, s, e)| (line, s, e)));
    set_hover_url(next);
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
/// confirmed.
fn copy_selection() {
    let Some(text) = crate::term_grid::selection_text() else {
        return;
    };
    if text.is_empty() {
        return;
    }
    platform::set_clipboard(&text);
    let chars = text.chars().count();
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

/// Rebuild the renderer's glyph atlas with a new font and repaint. Runs on
/// the main thread; keeps the surface/device, swaps the cell renderer.
fn rebuild_font(family: &str, font_px: f32) {
    if let Ok(mut slot) = surface_slot().lock() {
        if let Some(handle) = slot.as_mut() {
            if let Some(renderer) = crate::cell_render::CellRenderer::new(
                &handle.gpu.device,
                &handle.gpu.queue,
                handle.gpu.config.format,
                family,
                font_px,
            ) {
                handle.gpu.cell_renderer = renderer;
                render(&mut handle.gpu);
            }
        }
    }
}

/// Re-create the atlas at the configured font/size (CSS px * scale),
/// dispatched to the main thread. Called when the font setting changes.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn request_set_font(family: String, font_size: u32) {
    let Some(app) = APP.get() else {
        return;
    };
    let font_px = (font_size as f32 * load_f32(&DPR, 2.0)).max(6.0);
    let _ = app.run_on_main_thread(move || rebuild_font(&family, font_px));
}

/// Record xterm's device cell size and rebuild the atlas to it, so the
/// surface matches the webview's spacing exactly. No-op if unchanged.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn set_cell_metrics(width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }
    // Swap both unconditionally; a short-circuiting && would skip the second
    // store and leave the height unset.
    let prev_w = XTERM_CELL_W.swap(width, Ordering::AcqRel);
    let prev_h = XTERM_CELL_H.swap(height, Ordering::AcqRel);
    if prev_w == width && prev_h == height {
        return;
    }
    tracing::debug!(width, height, "native-surface: xterm reported cell metrics");
    let Some(app) = APP.get() else {
        return;
    };
    let scale = f64::from(load_f32(&DPR, 2.0));
    let (family, font_px) = font_atlas_params(scale);
    let _ = app.run_on_main_thread(move || rebuild_font(&family, font_px));
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
/// device pixel ratio. Must run on the main thread.
pub(crate) fn set_bounds(x: f64, y: f64, width: f64, height: f64, dpr: f64) {
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
    store_f32(&DPR, dpr as f32);
    platform::set_frame(&handle.platform, x, y, width, height, dpr);
    // Respect an active overlay suppression so a resize does not pop the
    // surface back over an open dropdown.
    platform::set_hidden(&handle.platform, SUPPRESSED.load(Ordering::Acquire));

    let px_w = (width * dpr).max(1.0) as u32;
    let px_h = (height * dpr).max(1.0) as u32;
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

    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
            .map_err(|e| tracing::warn!(error = %e, "native-surface: request_device failed"))
            .ok()?;

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

    // Size the grid to the surface so the terminal fills the pane instead
    // of a fixed 80x24 corner.
    let (cols, rows) = state
        .cell_renderer
        .grid_size_for(state.config.width, state.config.height);
    crate::term_grid::resize_grid(cols, rows);
    push_native_size_if_changed(cols, rows);
    // Publish the cell size so the mouse handler can map points to cells.
    let (cw, ch) = state.cell_renderer.cell_size_px();
    store_f32(&CELL_W, cw);
    store_f32(&CELL_H, ch);

    // Disjoint borrows of GpuState fields so the grid-reading closure can
    // hold the renderer mutably and the device/queue immutably.
    let device = &state.device;
    let queue = &state.queue;
    let width = state.config.width;
    let height = state.config.height;
    let cell_renderer = &mut state.cell_renderer;
    let drew = crate::term_grid::with_grid(|grid| {
        if let Some(grid) = grid {
            let frac = cell_renderer.draw(
                device,
                queue,
                &mut encoder,
                &view,
                grid,
                width,
                height,
                split_ratio(),
            );
            set_divider_frac(frac);
            true
        } else {
            false
        }
    });
    if !drew {
        // No grid yet: clear to the default background. The pass records
        // its clear when dropped at the end of this block.
        let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("term-surface-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.04,
                        g: 0.05,
                        b: 0.16,
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
}
