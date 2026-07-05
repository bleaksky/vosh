//! macOS window glue for the native surface: a `CAMetalLayer`-backed
//! `NSView` subclass composited over the `WKWebView`, with `AppKit` mouse,
//! cursor-rect, clipboard, and URL-open plumbing. All raw objc2
//! message-sends, same style as `enable_macos_spellcheck`; every view
//! touch happens on the main thread.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::OnceLock;

use super::{
    divider_frac, load_f32, pointer_down, pointer_dragged, pointer_moved, pointer_up, render,
    surface_slot, wheel_scroll, SurfaceHandle, DPR, DRAGGING,
};
use objc2::declare::ClassBuilder;
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2::{class, msg_send, sel, Encode, Encoding};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
};

/// wgpu backend for this platform.
const BACKENDS: wgpu::Backends = wgpu::Backends::METAL;

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

/// The platform's window handles: the surface `NSView` and the
/// `CAMetalLayer` it hosts. Raw pointers are not Send, but every access is
/// funnelled through the main thread, so the marker is sound.
pub(super) struct PlatformSurface {
    view: *mut AnyObject,
    metal_layer: *mut AnyObject,
}
unsafe impl Send for PlatformSurface {}

/// Hide or show the surface view. Main thread only.
pub(super) fn set_hidden(platform: &PlatformSurface, hidden: bool) {
    // SAFETY: main thread; the view is live.
    unsafe {
        let _: () = msg_send![platform.view, setHidden: hidden];
    }
}

/// Move the view to the pane rect (CSS px, top-left origin) and keep the
/// Metal layer's scale in sync. Main thread only.
pub(super) fn set_frame(
    platform: &PlatformSurface,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    dpr: f64,
) {
    // SAFETY: main thread; the view and layer are live.
    unsafe {
        // The view's superview is the content view; AppKit frames use a
        // bottom-left origin, so flip the top-left y the webview reports.
        let superview: *mut AnyObject = msg_send![platform.view, superview];
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
        let _: () = msg_send![platform.view, setFrame: frame];
        let _: () = msg_send![platform.metal_layer, setContentsScale: dpr];
    }
}

/// Post-redraw hook: refresh the divider cursor rect for the current split
/// state, except mid-drag (`AppKit` holds the cursor through the drag).
pub(super) fn after_redraw(platform: &PlatformSurface) {
    if DRAGGING.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }
    // SAFETY: main thread; the view and its window are live.
    unsafe {
        let window: *mut AnyObject = msg_send![platform.view, window];
        if !window.is_null() {
            let _: () = msg_send![window, invalidateCursorRectsForView: platform.view];
        }
    }
}

/// Put `text` on the general pasteboard (UTF-8 plain text).
pub(super) fn set_clipboard(text: &str) {
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

/// Open a URL in the default browser via `NSWorkspace`.
pub(super) fn open_url(url: &str) {
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

/// Map a mouse event to a grid cell via the shared physical-pixel mapping.
fn point_to_cell(this: *mut AnyObject, event: *mut AnyObject) -> Option<(i32, usize)> {
    if this.is_null() || event.is_null() {
        return None;
    }
    let dpr = f64::from(load_f32(&DPR, 2.0));
    // SAFETY: AppKit hands us a live NSView (`this`) and NSEvent.
    unsafe {
        let win_pt: CGPoint = msg_send![event, locationInWindow];
        let view_pt: CGPoint =
            msg_send![this, convertPoint: win_pt, fromView: std::ptr::null_mut::<AnyObject>()];
        let bounds: CGRect = msg_send![this, bounds];
        // NSView is bottom-left; flip to top-down, then scale to pixels.
        let phys_x = view_pt.x * dpr;
        let phys_y = (bounds.size.height - view_pt.y) * dpr;
        super::phys_point_to_cell(phys_x, phys_y, bounds.size.height * dpr)
    }
}

/// The view's height in points, for the divider grab band.
fn view_height(this: *mut AnyObject) -> f64 {
    if this.is_null() {
        return 0.0;
    }
    // SAFETY: AppKit hands us a live NSView.
    unsafe {
        let bounds: CGRect = msg_send![this, bounds];
        bounds.size.height
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

/// Mouse-wheel handler. `AppKit` calls this on the main thread with a live
/// `NSEvent`; the shared accumulator scrolls the grid by whole lines.
extern "C" fn scroll_wheel(_this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    if event.is_null() {
        return;
    }
    // SAFETY: AppKit hands us a valid NSEvent for the scrollWheel: selector.
    let delta_y: f64 = unsafe { msg_send![event, scrollingDeltaY] };
    wheel_scroll(delta_y);
}

/// Grab the divider if the press lands on it, otherwise begin a selection.
/// Cmd+click opens a URL under the pointer.
extern "C" fn mouse_down(this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    pointer_down(
        point_to_cell(this, event),
        event_fraction(this, event),
        view_height(this),
        event_has_command(event),
    );
}

/// Move the divider, or extend the selection, while dragging.
extern "C" fn mouse_dragged(this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    pointer_dragged(point_to_cell(this, event), event_fraction(this, event));
}

extern "C" fn mouse_up(_this: *mut AnyObject, _cmd: Sel, _event: *mut AnyObject) {
    pointer_up();
}

/// Track the URL under the pointer so the renderer can underline it.
extern "C" fn mouse_moved(this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    pointer_moved(point_to_cell(this, event));
}

extern "C" fn mouse_exited(_this: *mut AnyObject, _cmd: Sel, _event: *mut AnyObject) {
    pointer_moved(None);
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

/// Create the surface view over the main window's content view and stand up
/// the GPU. Best-effort: logs and returns on any missing handle. Runs on
/// the main thread (the `with_webview` callback).
pub(super) fn install(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
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
            let (font_stack, font_px) = super::font_atlas_params(scale);
            let window_handle = NonNull::new(view.cast::<c_void>())
                .map(|nn| RawWindowHandle::AppKit(AppKitWindowHandle::new(nn)));
            let Some(window_handle) = window_handle else {
                tracing::warn!("native-surface: view pointer was null for wgpu");
                return;
            };
            let display_handle = RawDisplayHandle::AppKit(AppKitDisplayHandle::new());
            match super::init_gpu(
                window_handle,
                display_handle,
                BACKENDS,
                px_w,
                px_h,
                &font_stack,
                font_px,
            ) {
                Some(gpu) => {
                    let mut handle = SurfaceHandle {
                        platform: PlatformSurface { view, metal_layer },
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
