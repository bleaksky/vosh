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
use std::sync::{Mutex, OnceLock};

use objc2::runtime::AnyObject;
use objc2::{class, msg_send, Encode, Encoding};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
};

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

const SHADER: &str = r"
@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(
        vec2<f32>( 0.0,  0.6),
        vec2<f32>(-0.6, -0.6),
        vec2<f32>( 0.6, -0.6),
    );
    return vec4<f32>(p[i], 0.0, 1.0);
}

@fragment
fn fs() -> @location(0) vec4<f32> {
    return vec4<f32>(0.1, 0.85, 0.78, 1.0); // teal
}
";

// Live wgpu objects for the terminal surface.
struct GpuState {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
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

/// Install the native surface over the main window's content view.
/// Best-effort: logs and returns on any missing handle. Runs on the main
/// thread (the `with_webview` callback). The surface starts at a
/// placeholder frame; the frontend's first `set_bounds` call snaps it to
/// the terminal pane.
pub(crate) fn install_probe(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
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
            match init_gpu(view.cast::<c_void>(), px_w, px_h) {
                Some(gpu) => {
                    let handle = SurfaceHandle {
                        view,
                        metal_layer,
                        gpu,
                    };
                    render(&handle.gpu);
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
        render(&handle.gpu);
    }
}

unsafe fn init_gpu(ns_view: *mut c_void, width: u32, height: u32) -> Option<GpuState> {
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

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("term-surface"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("term-surface"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs",
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs",
            targets: &[Some(config.format.into())],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    Some(GpuState {
        _instance: instance,
        surface,
        device,
        queue,
        config,
        pipeline,
    })
}

fn render(state: &GpuState) {
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
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("term-surface"),
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
        rpass.set_pipeline(&state.pipeline);
        rpass.draw(0..3, 0..1);
    }
    state.queue.submit(Some(encoder.finish()));
    frame.present();
}
