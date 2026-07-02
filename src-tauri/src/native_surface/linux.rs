//! Linux window glue for the native surface: a raw X11 child window
//! created over the GTK toplevel Tauri owns, drawn by wgpu's Vulkan (or
//! GL) backend. The child selects no input events, so clicks and wheel
//! propagate up the X window tree to the webview underneath — mouse
//! interaction stays on the DOM side for now. On Wayland there is no
//! equivalent child-window mechanism, so install detects the backend and
//! falls back to xterm. Every window touch happens on the main (GTK)
//! thread.
//!
//! Written against the X11/GTK APIs without Linux hardware in the loop:
//! compile-checked in a container; runtime behavior needs a hardware pass
//! before the frontend enables the surface on Linux.

use std::ffi::{c_ulong, c_void};
use std::ptr::NonNull;

use gdk::glib::translate::ToGlibPtr;
use gdk::prelude::*;
use gtk::prelude::*;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, XlibDisplayHandle, XlibWindowHandle};
use x11_dl::xlib::Xlib;

use super::{render, surface_slot, SurfaceHandle};

/// wgpu backend for this platform.
const BACKENDS: wgpu::Backends = wgpu::Backends::VULKAN.union(wgpu::Backends::GL);

// The X11 accessors live in libgdk-3 (linked via the gtk crate) regardless
// of the runtime backend; install only calls them after confirming the
// display is X11.
extern "C" {
    fn gdk_x11_window_get_xid(window: *mut gdk::ffi::GdkWindow) -> c_ulong;
    fn gdk_x11_display_get_xdisplay(display: *mut gdk::ffi::GdkDisplay) -> *mut c_void;
}

/// The platform's window handles: the dlopened Xlib, the display, and the
/// child window id. Raw pointers are not Send, but every access is
/// funnelled through the main thread, so the marker is sound.
pub(super) struct PlatformSurface {
    xlib: Xlib,
    display: *mut c_void,
    window: c_ulong,
}
unsafe impl Send for PlatformSurface {}

/// Hide or show the surface window. Main thread only.
pub(super) fn set_hidden(platform: &PlatformSurface, hidden: bool) {
    let dpy = platform.display.cast();
    // SAFETY: main thread; the display connection and window are live.
    unsafe {
        if hidden {
            (platform.xlib.XUnmapWindow)(dpy, platform.window);
        } else {
            (platform.xlib.XMapWindow)(dpy, platform.window);
            (platform.xlib.XRaiseWindow)(dpy, platform.window);
        }
        (platform.xlib.XFlush)(dpy);
    }
}

/// Move the window to the pane rect (CSS px, top-left origin, scaled by
/// `dpr` to physical pixels) and keep it above the webview. Main thread
/// only.
pub(super) fn set_frame(
    platform: &PlatformSurface,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    dpr: f64,
) {
    let px = |v: f64| (v * dpr).round() as i32;
    let dpy = platform.display.cast();
    // SAFETY: main thread; the display connection and window are live.
    unsafe {
        (platform.xlib.XMoveResizeWindow)(
            dpy,
            platform.window,
            px(x),
            px(y),
            px(width).max(1) as u32,
            px(height).max(1) as u32,
        );
        (platform.xlib.XRaiseWindow)(dpy, platform.window);
        (platform.xlib.XFlush)(dpy);
    }
}

/// Post-redraw hook. Nothing to refresh: input passes through to the
/// webview, so there are no cursor rects to manage.
pub(super) fn after_redraw(_platform: &PlatformSurface) {}

/// Put `text` on the clipboard via GTK.
pub(super) fn set_clipboard(text: &str) {
    let clipboard = gtk::Clipboard::get(&gdk::Atom::intern("CLIPBOARD"));
    clipboard.set_text(text);
}

/// Open a URL in the default browser via `xdg-open`.
pub(super) fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Create the child X window over the realized GTK toplevel and stand up
/// the GPU. Best-effort: logs and returns on Wayland or any missing
/// handle.
fn install_on(gtk_window: &gtk::ApplicationWindow) {
    // Only once (the map signal can fire again after unmap/map cycles).
    if let Ok(slot) = surface_slot().lock() {
        if slot.is_some() {
            return;
        }
    }
    let Some(gdk_window) = gtk_window.window() else {
        tracing::warn!("native-surface: GTK toplevel has no GdkWindow yet");
        return;
    };
    let display = gdk_window.display();
    if display.backend() != gdk::Backend::X11 {
        tracing::info!("native-surface: non-X11 GDK backend; xterm renders the terminal");
        return;
    }

    let gdk_ptr: *mut gdk::ffi::GdkWindow = gdk_window.to_glib_none().0;
    let display_ptr: *mut gdk::ffi::GdkDisplay = display.to_glib_none().0;
    // SAFETY: the display was just confirmed to be the X11 backend.
    let (parent_xid, xdisplay) = unsafe {
        (
            gdk_x11_window_get_xid(gdk_ptr),
            gdk_x11_display_get_xdisplay(display_ptr),
        )
    };
    if xdisplay.is_null() || parent_xid == 0 {
        tracing::warn!("native-surface: X11 display/window unavailable");
        return;
    }
    let Ok(xlib) = Xlib::open() else {
        tracing::warn!("native-surface: libX11 failed to load");
        return;
    };

    // SAFETY: live X connection; the child rides the toplevel. No event
    // mask, so input propagates up the tree to the webview.
    let (child, screen) = unsafe {
        let dpy = xdisplay.cast();
        let screen = (xlib.XDefaultScreen)(dpy);
        let black = (xlib.XBlackPixel)(dpy, screen);
        let child = (xlib.XCreateSimpleWindow)(dpy, parent_xid, 0, 0, 320, 200, 0, black, black);
        // Start unmapped (hidden): the surface is opaque and would occlude
        // xterm until the frontend opts in and reports bounds.
        (xlib.XFlush)(dpy);
        (child, screen)
    };
    if child == 0 {
        tracing::warn!("native-surface: XCreateSimpleWindow failed");
        return;
    }

    let scale = f64::from(gtk_window.scale_factor());
    let (font_stack, font_px) = super::font_atlas_params(scale.max(1.0));

    let window_handle = RawWindowHandle::Xlib(XlibWindowHandle::new(child));
    let display_handle =
        RawDisplayHandle::Xlib(XlibDisplayHandle::new(NonNull::new(xdisplay), screen));
    // SAFETY: the handles reference the live child window created above.
    match unsafe {
        super::init_gpu(
            window_handle,
            display_handle,
            BACKENDS,
            320,
            200,
            &font_stack,
            font_px,
        )
    } {
        Some(gpu) => {
            let mut handle = SurfaceHandle {
                platform: PlatformSurface {
                    xlib,
                    display: xdisplay,
                    window: child,
                },
                gpu,
            };
            render(&mut handle.gpu);
            if let Ok(mut slot) = surface_slot().lock() {
                *slot = Some(handle);
            }
            tracing::info!("native-surface: linux X11 surface installed");
        }
        None => tracing::warn!("native-surface: wgpu init failed; native window is blank"),
    }
}

/// Install the surface over the Tauri window's GTK toplevel, deferring to
/// the map signal when the toplevel is not yet realized. Runs on the main
/// thread.
pub(super) fn install(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    let gtk_window = window.gtk_window()?;
    if gtk_window.window().is_some() {
        install_on(&gtk_window);
    } else {
        gtk_window.connect_map(install_on);
    }
    Ok(())
}
