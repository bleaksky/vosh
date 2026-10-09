//! The GPU device the surface draws with and the fonts its glyph atlas
//! loads. Install hands the atlas the bundled font, and `init_gpu`
//! stands the device up over the platform's view. A font or cell size
//! the page reports rebuilds the atlas, loading the fonts off the main
//! thread, and the newest request wins.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use tauri::Manager;

use super::pointer::{load_f32, CELLS};
use super::{render, surface_slot, APP};
use crate::native::gpu::atlas::{hand_in_bundled, BUNDLED_FILES};

// Live wgpu objects for the terminal surface.
pub(super) struct GpuState {
    _instance: wgpu::Instance,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) cell_renderer: crate::native::gpu::CellRenderer,
}

/// The device cell xterm reported, as the page last sent it.
struct XtermCell {
    // xterm's reported device cell size. When set, the atlas uses it
    // instead of deriving from font metrics, so the surface matches xterm's
    // density exactly (0 = unset, fall back to the font's metrics).
    width: AtomicU32,
    height: AtomicU32,
    // xterm's device glyph box height, reported with the cell. The cell is
    // the box times the line height, and xterm centers the box in it, so
    // the atlas needs both to put its baseline where xterm's is (0 =
    // unset).
    char_height: AtomicU32,
}

static XTERM_CELL: XtermCell = XtermCell {
    width: AtomicU32::new(0),
    height: AtomicU32::new(0),
    char_height: AtomicU32::new(0),
};

/// xterm's reported device cell size, if the frontend has sent it. The glyph
/// atlas sizes its cells to this so spacing matches the webview.
fn reported_cell() -> Option<(u32, u32)> {
    let w = XTERM_CELL.width.load(Ordering::Acquire);
    let h = XTERM_CELL.height.load(Ordering::Acquire);
    if w > 0 && h > 0 {
        Some((w, h))
    } else {
        None
    }
}

/// xterm's reported device glyph box height, if the frontend sent one with
/// the cell size.
fn reported_char_height() -> Option<u32> {
    let h = XTERM_CELL.char_height.load(Ordering::Acquire);
    (h > 0).then_some(h)
}

/// Swap in a renderer built from `fonts` and repaint. Runs on the main
/// thread. It keeps the surface and device. The fonts arrive loaded from
/// the blocking pool, so all that is left here is rasterizing the ASCII
/// glyphs and uploading the atlas.
fn swap_font(fonts: crate::native::gpu::atlas::AtlasFonts, font_px: f32) {
    if let Ok(mut slot) = surface_slot().lock() {
        if let Some(handle) = slot.as_mut() {
            handle.gpu.cell_renderer = crate::native::gpu::CellRenderer::with_fonts(
                &handle.gpu.device,
                &handle.gpu.queue,
                handle.gpu.config.format,
                fonts,
                font_px,
                reported_cell(),
                reported_char_height(),
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
        move || crate::native::gpu::atlas::AtlasFonts::load(&family),
        move |swap| {
            let _ = app.run_on_main_thread(swap);
        },
        move |fonts| swap_font(fonts, font_px),
    );
}

/// Re-create the atlas at the configured font/size (CSS px * scale).
/// Called when the font setting changes. `font_size` takes a half step,
/// such as 13.5, and the atlas draws it at the fractional point size.
/// The cell stays whole device pixels, since it comes from xterm's device
/// cell or from rounded font metrics.
pub(crate) fn request_set_font(family: String, font_size: f32) {
    let font_px = (font_size * load_f32(&CELLS.dpr, 2.0)).max(6.0);
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
    let prev_w = XTERM_CELL.width.swap(width, Ordering::AcqRel);
    let prev_h = XTERM_CELL.height.swap(height, Ordering::AcqRel);
    let prev_char = XTERM_CELL.char_height.swap(char_height, Ordering::AcqRel);
    if prev_w == width && prev_h == height && prev_char == char_height {
        return;
    }
    tracing::debug!(
        width,
        height,
        char_height,
        "native-surface: xterm reported cell metrics"
    );
    let scale = f64::from(load_f32(&CELLS.dpr, 2.0));
    let (family, font_px) = font_atlas_params(scale);
    request_font_rebuild(family, font_px);
}

/// The configured terminal font family stack and the atlas pixel size.
/// Falls back to the system monospace at 14 CSS px. `scale` is the backing
/// scale factor, so the returned px is physical pixels (crisp at retina)
/// and the glyphs match the xterm font size.
#[allow(clippy::cast_precision_loss)]
pub(super) fn font_atlas_params(scale: f64) -> (String, f32) {
    let size_for = |css: f32| (css * scale as f32).max(6.0);
    let Some(app) = APP.get() else {
        return ("monospace".to_string(), size_for(14.0));
    };
    let state = app.state::<crate::app::state::SharedState>();
    let open = state.selected_session().profile();
    let font = open
        .try_lock()
        .map(|p| (p.ui.font_family.clone(), p.ui.font_size));
    match font {
        Some((family, size)) => (family, size_for(size.px())),
        None => ("monospace".to_string(), size_for(14.0)),
    }
}

/// Hand the atlas the font the page ships, before the first atlas loads
/// its fonts. Without it the atlas looks the family up among your
/// installed fonts, as it does when a bundled face fails to load.
pub(super) fn hand_in_bundled_fonts(app: &tauri::AppHandle) {
    let [regular, bold] = BUNDLED_FILES.map(|file| bundled_font(app, file));
    if let (Some(regular), Some(bold)) = (regular, bold) {
        hand_in_bundled(regular, bold);
    } else {
        tracing::warn!("native-surface: the app lacks the bundled JetBrains Mono");
    }
}

/// A bundled font file from the page's assets. The resolver answers a
/// path it does not hold with index.html, so only a font counts.
#[cfg(not(dev))]
fn bundled_font(app: &tauri::AppHandle, file: &str) -> Option<Vec<u8>> {
    app.asset_resolver()
        .get(format!("fonts/{file}"))
        .filter(|asset| asset.mime_type != "text/html")
        .map(|asset| asset.bytes)
}

/// A bundled font file from the repo, which a dev build reads in place
/// of the page's assets.
#[cfg(dev)]
fn bundled_font(_app: &tauri::AppHandle, file: &str) -> Option<Vec<u8>> {
    crate::native::gpu::atlas::repo_font(file)
}

/// Clamp a drawable size to the device's texture limit. `configure` panics
/// on anything larger, and that panic would abort the app from inside an
/// `AppKit` callback.
pub(super) fn clamp_to_device(device: &wgpu::Device, width: u32, height: u32) -> (u32, u32) {
    let max = device.limits().max_texture_dimension_2d.max(1);
    (width.clamp(1, max), height.clamp(1, max))
}

/// Build the wgpu surface + device + cell renderer over the platform's raw
/// window handle. `backends` is the platform's preferred wgpu backend.
pub(super) unsafe fn init_gpu(
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
            // The drawable spans the window, which can pass the default
            // 8192 px cap on a window stretched across displays. Ask for
            // what the GPU actually supports (16384 on Apple).
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

    let cell_renderer = crate::native::gpu::CellRenderer::new(
        &device,
        &queue,
        config.format,
        font_stack,
        font_px,
        reported_cell(),
        reported_char_height(),
    )?;

    Some(GpuState {
        _instance: instance,
        surface,
        device,
        queue,
        config,
        cell_renderer,
    })
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
}
