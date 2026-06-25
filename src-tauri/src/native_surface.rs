//! Tier 3, milestone M1 (see docs/native-renderer.md).
//!
//! Before committing to wgpu, prove the one thing the whole hybrid
//! approach hinges on: a native child view can be layered over the
//! `WKWebView` inside the Tauri window and actually composite on screen.
//! This installs a solid magenta `NSView` at a fixed frame. If you see
//! the rectangle floating over the terminal, native compositing works
//! and M1b (replace the colored layer with a wgpu Metal surface) is just
//! rendering. If it does not show, the overlay-subview architecture is
//! dead and we stay in Tier 1/2.
//!
//! macOS only. Raw objc2 message-sends, matching `enable_macos_spellcheck`
//! in lib.rs, so this needs no new dependencies yet.

#![cfg(target_os = "macos")]
// objc2 message-sends and the CoreGraphics Encode impls are unsafe; the
// workspace forbids unsafe by default, same as enable_macos_spellcheck.
#![allow(unsafe_code)]

use objc2::runtime::AnyObject;
use objc2::{class, msg_send, Encode, Encoding};

// Self-describing CoreGraphics geometry so `initWithFrame:` can be
// message-sent without pulling objc2-foundation. The encoding strings
// match what the objc runtime expects for CGRect on 64-bit.
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

/// Install the M1 magenta probe view over the main window's content view.
/// Best-effort: logs and returns on any missing handle rather than
/// panicking, so a webview that has not finished wiring up just no-ops.
pub(crate) fn install_probe(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    window.with_webview(|webview| {
        let wk = webview.inner().cast::<AnyObject>();
        if wk.is_null() {
            tracing::warn!("native-surface: webview.inner() was null");
            return;
        }
        unsafe {
            // WKWebView -> its NSWindow -> the window's contentView.
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

            // Fixed frame in the contentView's bottom-left origin space.
            // Deliberately small and offset so it reads as an overlay, not
            // a full-window paint.
            let frame = CGRect {
                origin: CGPoint { x: 140.0, y: 160.0 },
                size: CGSize {
                    width: 360.0,
                    height: 240.0,
                },
            };

            let view: *mut AnyObject = msg_send![class!(NSView), alloc];
            let view: *mut AnyObject = msg_send![view, initWithFrame: frame];
            if view.is_null() {
                tracing::warn!("native-surface: NSView alloc/init failed");
                return;
            }
            // Layer-backed so it has a CALayer we can color (and later swap
            // for a CAMetalLayer).
            let _: () = msg_send![view, setWantsLayer: true];
            let layer: *mut AnyObject = msg_send![view, layer];
            if layer.is_null() {
                tracing::warn!("native-surface: view has no layer after setWantsLayer");
                return;
            }
            let magenta: *mut AnyObject = msg_send![class!(NSColor), magentaColor];
            let cgcolor: *mut AnyObject = msg_send![magenta, CGColor];
            let _: () = msg_send![layer, setBackgroundColor: cgcolor];

            // Add last so it sits above the webview in the subview order.
            let _: () = msg_send![content_view, addSubview: view];
            tracing::info!("native-surface: M1 magenta probe view installed");
        }
    })
}
