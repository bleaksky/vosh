//! Tier 3, milestone M1 (see docs/native-renderer.md).
//!
//! M1a proved a native child view composites over the `WKWebView` inside
//! the Tauri window. M1b replaces that flat colored layer with a real
//! wgpu Metal surface and draws a test scene: clear to dark blue, then a
//! teal triangle. The signal is graduated: a triangle on blue means the
//! full GPU pipeline into the native surface works; blue only means the
//! surface works but the render pipeline failed; nothing means surface
//! creation failed. That settles whether the terminal can be
//! GPU-rendered natively; the rest of Tier 3 feeds a real grid into this
//! surface.
//!
//! macOS only. The view/layer plumbing is raw objc2 message-sends, same
//! style as `enable_macos_spellcheck`.

#![cfg(target_os = "macos")]
// objc2 message-sends, the CoreGraphics Encode impls, and the wgpu raw
// window handle are all unsafe; the workspace forbids unsafe by default.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::runtime::AnyObject;
use objc2::{class, msg_send, Encode, Encoding};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
};

// Self-describing CoreGraphics geometry so `initWithFrame:` can be
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

// Live wgpu objects. Leaked for the M1 probe so the surface keeps
// presenting; real lifetime management lands when the renderer is wired
// to the terminal pane.
struct GpuState {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
}

/// Install the M1 native surface over the main window's content view.
/// Best-effort: logs and returns on any missing handle.
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
                origin: CGPoint { x: 140.0, y: 160.0 },
                size: CGSize {
                    width: 360.0,
                    height: 240.0,
                },
            };
            let scale: f64 = msg_send![ns_window, backingScaleFactor];

            let view: *mut AnyObject = msg_send![class!(NSView), alloc];
            let view: *mut AnyObject = msg_send![view, initWithFrame: frame];
            if view.is_null() {
                tracing::warn!("native-surface: NSView alloc/init failed");
                return;
            }
            // Host a CAMetalLayer so wgpu can target it.
            let metal_layer: *mut AnyObject = msg_send![class!(CAMetalLayer), layer];
            if metal_layer.is_null() {
                tracing::warn!("native-surface: CAMetalLayer creation failed");
                return;
            }
            let _: () = msg_send![metal_layer, setContentsScale: scale];
            let _: () = msg_send![view, setLayer: metal_layer];
            let _: () = msg_send![view, setWantsLayer: true];
            let _: () = msg_send![content_view, addSubview: view];

            let px_w = (frame.size.width * scale).max(1.0) as u32;
            let px_h = (frame.size.height * scale).max(1.0) as u32;
            match init_gpu(view.cast::<c_void>(), px_w, px_h) {
                Some(state) => {
                    render(&state);
                    // Keep it alive for the life of the process (probe only).
                    Box::leak(Box::new(state));
                    tracing::info!("native-surface: M1b wgpu surface rendered");
                }
                None => tracing::warn!("native-surface: wgpu init failed; native view is blank"),
            }
        }
    })
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
        label: Some("m1-probe"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("m1-probe"),
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
            label: Some("m1-probe"),
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
