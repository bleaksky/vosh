//! macOS window glue for the native surface: a `CAMetalLayer`-backed
//! `NSView` subclass under the `WKWebView`, the placement of the grid in
//! the pane, and clipboard and URL-open plumbing. The page above it takes
//! every pointer event. All raw objc2 message-sends, same style as
//! `enable_macos_spellcheck`; every view touch happens on the main
//! thread.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;

use super::device::clamp_to_device;
use super::{render, surface_slot, SurfaceHandle, PANE};
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

// Opaque CoreGraphics object pointers, typed so msg_send's debug encoding
// check sees the `^{CGColorSpace=}` and `^{CGColor=}` the layer expects.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct CGColorSpaceRef(*mut c_void);
unsafe impl Encode for CGColorSpaceRef {
    const ENCODING: Encoding = Encoding::Pointer(&Encoding::Struct("CGColorSpace", &[]));
}
#[repr(transparent)]
#[derive(Clone, Copy)]
struct CGColorRef(*mut c_void);
unsafe impl Encode for CGColorRef {
    const ENCODING: Encoding = Encoding::Pointer(&Encoding::Struct("CGColor", &[]));
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    static kCGColorSpaceSRGB: *const c_void;
    fn CGColorSpaceCreateWithName(name: *const c_void) -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGColorCreate(space: *mut c_void, components: *const f64) -> *mut c_void;
    fn CGColorRelease(color: *mut c_void);
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {
    static NSWindowDidEnterFullScreenNotification: *mut AnyObject;
    static NSWindowDidExitFullScreenNotification: *mut AnyObject;
}

/// Corner radius for the underlay layer when the window cannot report its
/// own. The underlay spans the window, so it clips to the window's curve or
/// its square corners would paint past the rounded window edge.
const FALLBACK_CORNER_RADIUS: f64 = 10.0;

/// `NSWindowStyleMaskFullScreen`. A fullscreen window has square corners.
const STYLE_MASK_FULL_SCREEN: usize = 1 << 14;

// The radius last set on the layer, as f64 bits. Starts at a NaN pattern
// so the first real radius always applies.
static CORNER_RADIUS: AtomicU64 = AtomicU64::new(u64::MAX);

// The underlay layer, for the fullscreen observer, which has no other way
// to reach it. The layer lives as long as the app.
static UNDERLAY_LAYER: AtomicPtr<AnyObject> = AtomicPtr::new(std::ptr::null_mut());

/// The radius the underlay should clip to. None in fullscreen, otherwise
/// the window's own radius when it reports a sane one, else the fallback.
fn corner_radius_for(fullscreen: bool, reported: Option<f64>) -> f64 {
    if fullscreen {
        return 0.0;
    }
    match reported {
        Some(radius) if radius.is_finite() && radius > 0.0 && radius <= 64.0 => radius,
        _ => FALLBACK_CORNER_RADIUS,
    }
}

/// The window's corner radius from the private `_cornerRadius` getter
/// (16 on macOS 26 and later). Guarded twice: the method must exist
/// and return a double, since a debug build's `msg_send` check panics on a
/// mismatched return type and that panic cannot unwind out of `AppKit`.
/// Main thread only.
unsafe fn window_corner_radius(ns_window: *mut AnyObject) -> Option<f64> {
    let selector = sel!(_cornerRadius);
    let method = (*ns_window).class().instance_method(selector)?;
    if &*method.return_type() != "d" {
        return None;
    }
    let radius: f64 = msg_send![ns_window, _cornerRadius];
    Some(radius)
}

/// Clip the underlay layer to the window's current corner radius. A no-op
/// when the radius has not changed. Main thread only.
unsafe fn apply_corner_radius(ns_window: *mut AnyObject, metal_layer: *mut AnyObject) {
    if ns_window.is_null() || metal_layer.is_null() {
        return;
    }
    let style_mask: usize = msg_send![ns_window, styleMask];
    let radius = corner_radius_for(
        style_mask & STYLE_MASK_FULL_SCREEN != 0,
        window_corner_radius(ns_window),
    );
    if CORNER_RADIUS.swap(radius.to_bits(), Ordering::AcqRel) == radius.to_bits() {
        return;
    }
    // Set it without the implicit animation.
    let _: () = msg_send![class!(CATransaction), begin];
    let _: () = msg_send![class!(CATransaction), setDisableActions: true];
    let _: () = msg_send![metal_layer, setCornerRadius: radius];
    let _: () = msg_send![class!(CATransaction), commit];
}

/// Re-read the window's corner radius and fullscreen state and clip the
/// underlay to match. Main thread only.
fn sync_corner_radius(platform: &PlatformSurface) {
    // SAFETY: main thread; the view and layer are live.
    unsafe {
        let window: *mut AnyObject = msg_send![platform.view, window];
        apply_corner_radius(window, platform.metal_layer);
    }
}

/// Fullscreen entered or exited. `AppKit` posts both notifications on the
/// main thread with the window as the object.
extern "C" fn full_screen_changed(_this: *mut AnyObject, _cmd: Sel, note: *mut AnyObject) {
    if note.is_null() {
        return;
    }
    // SAFETY: AppKit hands us a live NSNotification on the main thread,
    // and the layer lives as long as the app.
    unsafe {
        let window: *mut AnyObject = msg_send![note, object];
        apply_corner_radius(window, UNDERLAY_LAYER.load(Ordering::Acquire));
    }
}

/// An `NSObject` subclass that receives the window's fullscreen
/// notifications. Registered once.
fn window_observer_class() -> &'static AnyClass {
    static CLASS: OnceLock<usize> = OnceLock::new();
    let ptr = *CLASS.get_or_init(|| {
        let mut builder = ClassBuilder::new("VoshWindowObserver", class!(NSObject))
            .expect("VoshWindowObserver already registered");
        // SAFETY: the signature matches a notification observer method.
        unsafe {
            builder.add_method(
                sel!(fullScreenChanged:),
                full_screen_changed as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
        }
        let cls: &'static AnyClass = builder.register();
        std::ptr::from_ref(cls) as usize
    });
    // SAFETY: the pointer comes from a registered, process-lifetime class.
    unsafe { &*(ptr as *const AnyClass) }
}

/// Follow the window in and out of fullscreen so the underlay's corners go
/// square and back. The observer is never released, since the window lives
/// as long as the app. Main thread only.
unsafe fn observe_full_screen(ns_window: *mut AnyObject) {
    let observer: *mut AnyObject = msg_send![window_observer_class(), new];
    if observer.is_null() {
        return;
    }
    let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
    for name in [
        NSWindowDidEnterFullScreenNotification,
        NSWindowDidExitFullScreenNotification,
    ] {
        let _: () = msg_send![
            center,
            addObserver: observer,
            selector: sel!(fullScreenChanged:),
            name: name,
            object: ns_window
        ];
    }
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
fn set_hidden(platform: &PlatformSurface, hidden: bool) {
    // SAFETY: main thread; the view is live.
    unsafe {
        let _: () = msg_send![platform.view, setHidden: hidden];
    }
}

/// The view's size in device pixels at `dpr`. The view spans the window
/// and `AppKit` resizes it, so the surface reads its size here instead of
/// from the frontend. Main thread only.
fn view_size_px(platform: &PlatformSurface, dpr: f64) -> (u32, u32) {
    // SAFETY: main thread; the view is live.
    let bounds: CGRect = unsafe { msg_send![platform.view, bounds] };
    (
        (bounds.size.width * dpr).round().max(1.0) as u32,
        (bounds.size.height * dpr).round().max(1.0) as u32,
    )
}

/// Keep the layer's backing scale in step with the display the window is
/// on, so text stays crisp after a move between a 2x and a 1x screen.
/// Main thread only.
fn set_scale(platform: &PlatformSurface, dpr: f64) {
    // SAFETY: main thread; the layer is live.
    unsafe {
        let current: f64 = msg_send![platform.metal_layer, contentsScale];
        if (current - dpr).abs() > f64::EPSILON {
            let _: () = msg_send![platform.metal_layer, setContentsScale: dpr];
        }
    }
}

// Last backdrop color set on the layer, packed 0x00RRGGBB, plus one so the
// initial zero means unset.
static BACKDROP: AtomicU32 = AtomicU32::new(0);

/// Paint the layer's own background in the terminal color. During a live
/// resize the drawable lags the view by a frame, and the uncovered strip
/// shows this color instead of the desktop. Main thread only.
pub(super) fn set_backdrop(platform: &PlatformSurface, rgb: (u8, u8, u8)) {
    let packed = (u32::from(rgb.0) << 16 | u32::from(rgb.1) << 8 | u32::from(rgb.2)) + 1;
    if BACKDROP.swap(packed, Ordering::AcqRel) == packed {
        return;
    }
    // SAFETY: main thread; the layer is live. The CGColor is retained by
    // the layer and released here after the set.
    unsafe {
        // CGColorCreate with an sRGB space, not CGColorCreateSRGB, which
        // is 10.15+ and would fail to load on the 10.13 Intel floor.
        let space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        if space.is_null() {
            return;
        }
        let components = [
            f64::from(rgb.0) / 255.0,
            f64::from(rgb.1) / 255.0,
            f64::from(rgb.2) / 255.0,
            1.0,
        ];
        let color = CGColorCreate(space, components.as_ptr());
        CGColorSpaceRelease(space);
        if color.is_null() {
            return;
        }
        // Layer property changes animate by default. Set it without the
        // implicit fade.
        let _: () = msg_send![class!(CATransaction), begin];
        let _: () = msg_send![class!(CATransaction), setDisableActions: true];
        let _: () = msg_send![platform.metal_layer, setBackgroundColor: CGColorRef(color)];
        let _: () = msg_send![class!(CATransaction), commit];
        CGColorRelease(color);
    }
}

/// Place the grid in the pane the page reports and show the surface.
/// `x`/`y`/`width`/`height` are CSS pixels in the webview's top-left
/// coordinate space at `dpr`. The layer follows the display's scale, the
/// theme's backdrop and the window's corners, and the drawable follows the
/// view's size. Main thread only.
pub(super) fn place(handle: &mut SurfaceHandle, x: f64, y: f64, width: f64, height: f64, dpr: f64) {
    // The view already spans the window (AppKit resizes it with the
    // window), so the report only moves the grid inside it. Snap to whole
    // device pixels so glyphs land on the pixel grid.
    let snap = |v: f64| (v * dpr).round().max(0.0) as u32;
    if let Ok(mut vp) = PANE.viewport.lock() {
        *vp = Some([snap(x), snap(y), snap(width).max(1), snap(height).max(1)]);
    }
    let (px_w, px_h) = view_size_px(&handle.platform, dpr);
    let (px_w, px_h) = clamp_to_device(&handle.gpu.device, px_w, px_h);
    set_scale(&handle.platform, dpr);
    set_hidden(&handle.platform, false);
    set_backdrop(&handle.platform, crate::native::gpu::style::theme_bg_rgb());
    // The window reports its real radius once it is on screen, and a
    // fullscreen switch also resizes the pane, so re-check here.
    sync_corner_radius(&handle.platform);
    if px_w != handle.gpu.config.width || px_h != handle.gpu.config.height {
        handle.gpu.config.width = px_w;
        handle.gpu.config.height = px_h;
        handle
            .gpu
            .surface
            .configure(&handle.gpu.device, &handle.gpu.config);
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
        // clearContents returns NSInteger (the new change count), not
        // void. Binding it as () makes objc2 encode the call as
        // returning 'v' while the selector is 'q'; a debug build's
        // msg_send verification then panics. Copy runs on the main
        // thread inside AppKit's event loop, which a panic cannot unwind
        // out of, so it aborts the whole app. Bind the real return so
        // copy is safe.
        let _: isize = msg_send![pasteboard, clearContents];
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

/// The view never takes a click. The webview sits above it and forwards
/// pointer input over IPC, so hit testing skips it even if something
/// reorders the views.
extern "C" fn hit_test(_this: *mut AnyObject, _cmd: Sel, _point: CGPoint) -> *mut AnyObject {
    std::ptr::null_mut()
}

/// A minimal `NSView` subclass that hit testing always passes over.
/// Registered once; the surface view is an instance of it.
fn surface_view_class() -> &'static AnyClass {
    static CLASS: OnceLock<usize> = OnceLock::new();
    let ptr = *CLASS.get_or_init(|| {
        let mut builder = ClassBuilder::new("VoshSurfaceView", class!(NSView))
            .expect("VoshSurfaceView already registered");
        // SAFETY: the signature matches the overridden NSView method.
        unsafe {
            builder.add_method(
                sel!(hitTest:),
                hit_test as extern "C" fn(*mut AnyObject, Sel, CGPoint) -> *mut AnyObject,
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

            // The view spans the webview's parent and sits below the
            // webview in it.
            let parent: *mut AnyObject = msg_send![wk, superview];
            let parent = if parent.is_null() {
                content_view
            } else {
                parent
            };
            let frame: CGRect = msg_send![parent, bounds];
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
            // Tag the drawable as sRGB so the compositor color-manages the
            // terminal the same way WebKit manages the chrome around it.
            // Left unset, the values go to a wide-gamut panel raw.
            let srgb = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
            if !srgb.is_null() {
                let _: () = msg_send![metal_layer, setColorspace: CGColorSpaceRef(srgb)];
                CGColorSpaceRelease(srgb);
            }
            let _: () = msg_send![view, setLayer: metal_layer];
            let _: () = msg_send![view, setWantsLayer: true];
            // Pin the drawn frame to the top-left so a resize that outpaces
            // the next render exposes backdrop at the right and bottom
            // instead of stretching the text. Clip to the window's curve,
            // read at runtime and square in fullscreen.
            let top_left: *mut AnyObject = msg_send![
                class!(NSString),
                stringWithUTF8String: c"topLeft".as_ptr()
            ];
            let _: () = msg_send![metal_layer, setContentsGravity: top_left];
            apply_corner_radius(ns_window, metal_layer);
            let _: () = msg_send![metal_layer, setMasksToBounds: true];
            UNDERLAY_LAYER.store(metal_layer, Ordering::Release);
            observe_full_screen(ns_window);
            // NSViewWidthSizable | NSViewHeightSizable: AppKit resizes the
            // view with the window in the same layout pass.
            let _: () = msg_send![view, setAutoresizingMask: 18_usize];
            // NSWindowBelow (-1): under the webview in its parent.
            let _: () = msg_send![parent, addSubview: view, positioned: -1_isize, relativeTo: wk];
            // Start hidden until the frontend opts in (flag) and reports
            // pane bounds, which reveals the surface and places the grid.
            let _: () = msg_send![view, setHidden: true];

            // init_gpu clamps these to the device's texture limit.
            let px_w = (frame.size.width * scale).max(1.0) as u32;
            let px_h = (frame.size.height * scale).max(1.0) as u32;
            let (font_stack, font_px) = super::device::font_atlas_params(scale);
            let window_handle = NonNull::new(view.cast::<c_void>())
                .map(|nn| RawWindowHandle::AppKit(AppKitWindowHandle::new(nn)));
            let Some(window_handle) = window_handle else {
                tracing::warn!("native-surface: view pointer was null for wgpu");
                return;
            };
            let display_handle = RawDisplayHandle::AppKit(AppKitDisplayHandle::new());
            match super::device::init_gpu(
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
                    tracing::info!("native-surface: surface installed");
                }
                None => tracing::warn!("native-surface: wgpu init failed; native view is blank"),
            }
        }
    })
}

#[cfg(test)]
// The radius passes through untouched, so exact equality is the check.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn corner_radius_is_square_in_fullscreen() {
        assert_eq!(corner_radius_for(true, Some(16.0)), 0.0);
        assert_eq!(corner_radius_for(true, None), 0.0);
    }

    #[test]
    fn corner_radius_uses_the_window_radius() {
        assert_eq!(corner_radius_for(false, Some(16.0)), 16.0);
        assert_eq!(corner_radius_for(false, Some(10.0)), 10.0);
    }

    #[test]
    fn corner_radius_falls_back_without_a_sane_report() {
        assert_eq!(corner_radius_for(false, None), FALLBACK_CORNER_RADIUS);
        assert_eq!(corner_radius_for(false, Some(0.0)), FALLBACK_CORNER_RADIUS);
        assert_eq!(corner_radius_for(false, Some(-4.0)), FALLBACK_CORNER_RADIUS);
        assert_eq!(
            corner_radius_for(false, Some(f64::NAN)),
            FALLBACK_CORNER_RADIUS
        );
        assert_eq!(
            corner_radius_for(false, Some(500.0)),
            FALLBACK_CORNER_RADIUS
        );
    }
}
