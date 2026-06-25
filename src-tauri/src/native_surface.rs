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

use objc2::runtime::AnyObject;
use objc2::{class, msg_send, Encode, Encoding};
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

/// Request a repaint of the terminal surface. Called from the session loop
/// after feeding the grid. No-ops until the surface is active; coalesces
/// bursts; dispatches the actual draw to the main thread (Metal requires
/// it).
pub(crate) fn request_redraw() {
    if !ACTIVE.load(Ordering::Acquire) {
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

            let view: *mut AnyObject = msg_send![class!(NSView), alloc];
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
        let _: () = msg_send![handle.view, setHidden: false];
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

    let config = surface.get_default_config(&adapter, width, height)?;
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

    // Disjoint borrows of GpuState fields so the grid-reading closure can
    // hold the renderer mutably and the device/queue immutably.
    let device = &state.device;
    let queue = &state.queue;
    let width = state.config.width;
    let height = state.config.height;
    let cell_renderer = &mut state.cell_renderer;
    let drew = crate::term_grid::with_grid(|grid| {
        if let Some(grid) = grid {
            cell_renderer.draw(device, queue, &mut encoder, &view, grid, width, height);
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
