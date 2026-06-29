//! Tier 3 native terminal renderer (see docs/native-renderer.md).
//!
//! M1 proved a wgpu Metal surface on a child `NSView` composites over the
//! `WKWebView` inside the Tauri window (teal triangle on dark blue). M2a
//! makes that surface track the real terminal pane: the frontend reports
//! the pane's bounds + device-pixel-ratio over IPC, and `set_bounds`
//! repositions/resizes the view, reconfigures the wgpu surface, and
//! redraws. Still drawing the test triangle, so the dark-blue rectangle
//! should now cover exactly the terminal pane.
//!
//! macOS only. The view/layer plumbing is raw objc2 message-sends, same
//! style as `enable_macos_spellcheck`. Every NSView/Metal touch happens
//! on the main thread (creation in `with_webview`, updates via
//! `AppHandle::run_on_main_thread`).

#![cfg(target_os = "macos")]
// objc2 message-sends, the CoreGraphics Encode impls, and the wgpu raw
// window handle are all unsafe; the workspace forbids unsafe by default.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use objc2::declare::ClassBuilder;
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2::{class, msg_send, sel, Encode, Encoding};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use tauri::Manager;

// Self-describing CoreGraphics geometry so frame messages can be
// message-sent without pulling objc2-foundation.
#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[Encoding::Double, Encoding::Double]);
}
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[Encoding::Double, Encoding::Double]);
}
unsafe impl Encode for CGRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

// Live wgpu objects for the terminal surface.
struct GpuState {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    cell_renderer: crate::cell_render::CellRenderer,
}

// The installed surface: its NSView, the CAMetalLayer it hosts, and the
// GPU state. Raw pointers are not Send, but every access is funnelled
// through the main thread (creation in `with_webview`, updates via
// `run_on_main_thread`), so the marker is sound.
struct SurfaceHandle {
    view: *mut AnyObject,
    metal_layer: *mut AnyObject,
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
                // SAFETY: main thread; the view is live.
                unsafe {
                    let _: () = msg_send![handle.view, setHidden: !visible];
                }
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
            // Refresh the divider cursor rect for the current split state,
            // except mid-drag (AppKit holds the cursor through the drag).
            if !DRAGGING.load(Ordering::Acquire) {
                // SAFETY: main thread; the view and its window are live.
                unsafe {
                    let window: *mut AnyObject = msg_send![handle.view, window];
                    if !window.is_null() {
                        let _: () = msg_send![window, invalidateCursorRectsForView: handle.view];
                    }
                }
            }
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

/// Map a mouse event to a grid cell `(line, col)`, mirroring the renderer's
/// split mapping: the bottom (live) region of an open split reads at offset
/// 0, the top (history) region at the scroll offset.
fn point_to_cell(this: *mut AnyObject, event: *mut AnyObject) -> Option<(i32, usize)> {
    if this.is_null() || event.is_null() {
        return None;
    }
    let dpr = f64::from(load_f32(&DPR, 2.0));
    let cell_w = f64::from(load_f32(&CELL_W, 0.0));
    let cell_h = f64::from(load_f32(&CELL_H, 0.0));
    if cell_w <= 0.0 || cell_h <= 0.0 {
        return None;
    }
    // SAFETY: AppKit hands us a live NSView (`this`) and NSEvent.
    unsafe {
        let win_pt: CGPoint = msg_send![event, locationInWindow];
        let view_pt: CGPoint =
            msg_send![this, convertPoint: win_pt, fromView: std::ptr::null_mut::<AnyObject>()];
        let bounds: CGRect = msg_send![this, bounds];
        // NSView is bottom-left; flip to top-down, then scale to pixels.
        let phys_x = view_pt.x * dpr;
        let phys_y = (bounds.size.height - view_pt.y) * dpr;
        let col = (phys_x / cell_w).floor().max(0.0) as usize;
        let row = (phys_y / cell_h).floor().max(0.0) as i32;
        let rows = (bounds.size.height * dpr / cell_h).floor() as i32;
        let offset = crate::term_grid::current_display_offset() as i32;
        // Match the renderer's split: top region is history at the offset,
        // bottom region is the live tail at offset 0.
        let split = offset > 0 && rows >= 6;
        let top_rows = if split {
            ((f64::from(rows) * f64::from(split_ratio())) as i32).clamp(1, rows - 1)
        } else {
            rows
        };
        let line = if split && row >= top_rows {
            row
        } else {
            row - offset
        };
        Some((line, col))
    }
}

/// Put `text` on the general pasteboard (UTF-8 plain text).
fn set_clipboard(text: &str) {
    let Ok(text_c) = std::ffi::CString::new(text) else {
        return;
    };
    let Ok(type_c) = std::ffi::CString::new("public.utf8-plain-text") else {
        return;
    };
    // SAFETY: standard NSPasteboard string write on the main thread.
    unsafe {
        let pasteboard: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pasteboard.is_null() {
            return;
        }
        let _: () = msg_send![pasteboard, clearContents];
        let ns_text: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: text_c.as_ptr()];
        let ns_type: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: type_c.as_ptr()];
        let _: bool = msg_send![pasteboard, setString: ns_text, forType: ns_type];
    }
}

/// Copy the current selection to the clipboard (no-op when empty).
fn copy_selection() {
    if let Some(text) = crate::term_grid::selection_text() {
        if !text.is_empty() {
            set_clipboard(&text);
        }
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

/// Mouse-wheel handler. `AppKit` calls this on the main thread with a live
/// `NSEvent`; accumulate fractional lines and scroll the grid.
extern "C" fn scroll_wheel(_this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    if event.is_null() {
        return;
    }
    // SAFETY: AppKit hands us a valid NSEvent for the scrollWheel: selector.
    let delta_y: f64 = unsafe { msg_send![event, scrollingDeltaY] };
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

/// The event's y location as a fraction of the view height (0 = top).
fn event_fraction(this: *mut AnyObject, event: *mut AnyObject) -> Option<f64> {
    if this.is_null() || event.is_null() {
        return None;
    }
    // SAFETY: AppKit hands us a live NSView (`this`) and NSEvent.
    unsafe {
        let win_pt: CGPoint = msg_send![event, locationInWindow];
        let view_pt: CGPoint =
            msg_send![this, convertPoint: win_pt, fromView: std::ptr::null_mut::<AnyObject>()];
        let bounds: CGRect = msg_send![this, bounds];
        if bounds.size.height <= 0.0 {
            return None;
        }
        // NSView uses a bottom-left origin; flip so 0 = top, matching rows.
        Some(1.0 - view_pt.y / bounds.size.height)
    }
}

/// True when the event has the Command modifier held.
fn event_has_command(event: *mut AnyObject) -> bool {
    if event.is_null() {
        return false;
    }
    // SAFETY: AppKit hands us a live NSEvent.
    let flags: usize = unsafe { msg_send![event, modifierFlags] };
    flags & (1 << 20) != 0 // NSEventModifierFlagCommand
}

/// Open a URL in the default browser via `NSWorkspace`.
fn open_url(url: &str) {
    let Ok(cstr) = std::ffi::CString::new(url) else {
        return;
    };
    // SAFETY: standard NSWorkspace openURL on the main thread.
    unsafe {
        let ns_str: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: cstr.as_ptr()];
        let ns_url: *mut AnyObject = msg_send![class!(NSURL), URLWithString: ns_str];
        if ns_url.is_null() {
            return;
        }
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let _: bool = msg_send![workspace, openURL: ns_url];
    }
}

/// Grab the divider if the press lands on it, otherwise begin a selection.
/// Cmd+click opens a URL under the pointer.
extern "C" fn mouse_down(this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    if event_has_command(event) {
        if let Some((line, col)) = point_to_cell(this, event) {
            if let Some((url, _, _)) = crate::term_grid::url_at(line, col) {
                open_url(&url);
                return;
            }
        }
    }
    if crate::term_grid::current_display_offset() > 0 {
        if let Some(frac) = event_fraction(this, event) {
            if (frac - f64::from(split_ratio())).abs() < 0.03 {
                DRAGGING.store(true, Ordering::Release);
                return;
            }
        }
    }
    crate::term_grid::clear_selection();
    if let Some((line, col)) = point_to_cell(this, event) {
        crate::term_grid::start_selection(line, col);
        SELECTING.store(true, Ordering::Release);
        redraw_now();
    }
}

/// Move the divider, or extend the selection, while dragging.
extern "C" fn mouse_dragged(this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    if DRAGGING.load(Ordering::Acquire) {
        if let Some(frac) = event_fraction(this, event) {
            set_split_ratio((frac.clamp(0.15, 0.85)) as f32);
            redraw_now();
        }
        return;
    }
    if SELECTING.load(Ordering::Acquire) {
        if let Some((line, col)) = point_to_cell(this, event) {
            crate::term_grid::update_selection(line, col);
            redraw_now();
        }
    }
}

extern "C" fn mouse_up(_this: *mut AnyObject, _cmd: Sel, _event: *mut AnyObject) {
    if DRAGGING.swap(false, Ordering::AcqRel) {
        // Divider drag over; redraw so the cursor rect refreshes.
        redraw_now();
        return;
    }
    if SELECTING.swap(false, Ordering::AcqRel) {
        // Copy the selection to the clipboard on release.
        copy_selection();
    }
}

/// Track the URL under the pointer so the renderer can underline it. Only
/// repaints when the hovered range actually changes.
extern "C" fn mouse_moved(this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    let next = point_to_cell(this, event)
        .and_then(|(line, col)| crate::term_grid::url_at(line, col).map(|(_, s, e)| (line, s, e)));
    set_hover_url(next);
}

extern "C" fn mouse_exited(_this: *mut AnyObject, _cmd: Sel, _event: *mut AnyObject) {
    set_hover_url(None);
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

/// Cursor rects: an arrow over the surface (so the webview's text cursor
/// does not bleed through) and a vertical-resize cursor over the divider
/// band while the split is open. `AppKit` holds the rect's cursor through a
/// drag started inside it, so the divider drag shows the resize cursor.
extern "C" fn reset_cursor_rects(this: *mut AnyObject, _cmd: Sel) {
    if this.is_null() {
        return;
    }
    // SAFETY: AppKit calls this with a live NSView.
    unsafe {
        let bounds: CGRect = msg_send![this, bounds];
        let arrow: *mut AnyObject = msg_send![class!(NSCursor), arrowCursor];
        let _: () = msg_send![this, addCursorRect: bounds, cursor: arrow];

        if let Some(frac) = divider_frac() {
            let height = bounds.size.height;
            let band = 12.0;
            // Divider is `frac` down from the top; NSView is bottom-left.
            let bottom_y = height * (1.0 - f64::from(frac)) - band / 2.0;
            let rect = CGRect {
                origin: CGPoint {
                    x: 0.0,
                    y: bottom_y,
                },
                size: CGSize {
                    width: bounds.size.width,
                    height: band,
                },
            };
            let resize: *mut AnyObject = msg_send![class!(NSCursor), resizeUpDownCursor];
            let _: () = msg_send![this, addCursorRect: rect, cursor: resize];
        }
    }
}

/// A minimal `NSView` subclass that forwards mouse-wheel events to the grid.
/// Registered once; the surface view is an instance of it.
fn surface_view_class() -> &'static AnyClass {
    static CLASS: OnceLock<usize> = OnceLock::new();
    let ptr = *CLASS.get_or_init(|| {
        let mut builder = ClassBuilder::new("VoshSurfaceView", class!(NSView))
            .expect("VoshSurfaceView already registered");
        // SAFETY: the signatures match the overridden NSView/NSResponder
        // methods.
        unsafe {
            builder.add_method(
                sel!(scrollWheel:),
                scroll_wheel as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            builder.add_method(
                sel!(mouseDown:),
                mouse_down as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            builder.add_method(
                sel!(mouseDragged:),
                mouse_dragged as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            builder.add_method(
                sel!(mouseUp:),
                mouse_up as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            builder.add_method(
                sel!(resetCursorRects),
                reset_cursor_rects as extern "C" fn(*mut AnyObject, Sel),
            );
            builder.add_method(
                sel!(mouseMoved:),
                mouse_moved as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            builder.add_method(
                sel!(mouseExited:),
                mouse_exited as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
        }
        let cls: &'static AnyClass = builder.register();
        std::ptr::from_ref(cls) as usize
    });
    // SAFETY: the pointer comes from a registered, process-lifetime class.
    unsafe { &*(ptr as *const AnyClass) }
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
/// thread (the `with_webview` callback). The surface starts at a
/// placeholder frame; the frontend's first `set_bounds` call snaps it to
/// the terminal pane.
pub(crate) fn install_probe(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    let _ = APP.set(window.app_handle().clone());
    window.with_webview(|webview| {
        let wk = webview.inner().cast::<AnyObject>();
        if wk.is_null() {
            tracing::warn!("native-surface: webview.inner() was null");
            return;
        }
        unsafe {
            let ns_window: *mut AnyObject = msg_send![wk, window];
            if ns_window.is_null() {
                tracing::warn!("native-surface: WKWebView has no window yet");
                return;
            }
            let content_view: *mut AnyObject = msg_send![ns_window, contentView];
            if content_view.is_null() {
                tracing::warn!("native-surface: window has no contentView");
                return;
            }

            let frame = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: 320.0,
                    height: 200.0,
                },
            };
            let scale: f64 = msg_send![ns_window, backingScaleFactor];

            let view: *mut AnyObject = msg_send![surface_view_class(), alloc];
            let view: *mut AnyObject = msg_send![view, initWithFrame: frame];
            if view.is_null() {
                tracing::warn!("native-surface: NSView alloc/init failed");
                return;
            }
            let metal_layer: *mut AnyObject = msg_send![class!(CAMetalLayer), layer];
            if metal_layer.is_null() {
                tracing::warn!("native-surface: CAMetalLayer creation failed");
                return;
            }
            let _: () = msg_send![metal_layer, setContentsScale: scale];
            let _: () = msg_send![view, setLayer: metal_layer];
            let _: () = msg_send![view, setWantsLayer: true];
            let _: () = msg_send![content_view, addSubview: view];
            // Tracking area for URL-hover: MouseMoved | MouseEnteredAndExited
            // | ActiveInKeyWindow | InVisibleRect. InVisibleRect keeps it
            // sized to the view automatically, so no manual resize tracking.
            let opts: usize = 0x02 | 0x01 | 0x20 | 0x200;
            let area: *mut AnyObject = msg_send![class!(NSTrackingArea), alloc];
            let zero = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: 0.0,
                    height: 0.0,
                },
            };
            let area: *mut AnyObject = msg_send![area, initWithRect: zero, options: opts, owner: view, userInfo: std::ptr::null_mut::<AnyObject>()];
            if !area.is_null() {
                let _: () = msg_send![view, addTrackingArea: area];
            }
            // Start hidden. The surface is opaque and would occlude xterm,
            // so it stays invisible until the frontend opts in (flag) and
            // reports pane bounds, which reveals and positions it.
            let _: () = msg_send![view, setHidden: true];

            let px_w = (frame.size.width * scale).max(1.0) as u32;
            let px_h = (frame.size.height * scale).max(1.0) as u32;
            let (font_stack, font_px) = font_atlas_params(scale);
            match init_gpu(view.cast::<c_void>(), px_w, px_h, &font_stack, font_px) {
                Some(gpu) => {
                    let mut handle = SurfaceHandle {
                        view,
                        metal_layer,
                        gpu,
                    };
                    render(&mut handle.gpu);
                    if let Ok(mut slot) = surface_slot().lock() {
                        *slot = Some(handle);
                    }
                    tracing::info!("native-surface: M2a surface installed");
                }
                None => tracing::warn!("native-surface: wgpu init failed; native view is blank"),
            }
        }
    })
}

/// Reposition and resize the surface to the terminal pane. `x`/`y`/`w`/`h`
/// are CSS pixels in the webview's top-left coordinate space (which maps
/// 1:1 to the content view's points); `dpr` is the device pixel ratio.
/// Must run on the main thread.
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
    unsafe {
        // The view's superview is the content view; AppKit frames use a
        // bottom-left origin, so flip the top-left y the webview reports.
        let superview: *mut AnyObject = msg_send![handle.view, superview];
        let content_h = if superview.is_null() {
            y + height
        } else {
            let cv_bounds: CGRect = msg_send![superview, bounds];
            cv_bounds.size.height
        };
        let frame = CGRect {
            origin: CGPoint {
                x,
                y: content_h - (y + height),
            },
            size: CGSize { width, height },
        };
        let _: () = msg_send![handle.view, setFrame: frame];
        // Respect an active overlay suppression so a resize does not pop the
        // surface back over an open dropdown.
        let _: () = msg_send![handle.view, setHidden: SUPPRESSED.load(Ordering::Acquire)];
        let _: () = msg_send![handle.metal_layer, setContentsScale: dpr];

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
}

unsafe fn init_gpu(
    ns_view: *mut c_void,
    width: u32,
    height: u32,
    font_stack: &str,
    font_px: f32,
) -> Option<GpuState> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });

    let window_handle = RawWindowHandle::AppKit(AppKitWindowHandle::new(NonNull::new(ns_view)?));
    let display_handle = RawDisplayHandle::AppKit(AppKitDisplayHandle::new());
    let surface = instance
        .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: display_handle,
            raw_window_handle: window_handle,
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
