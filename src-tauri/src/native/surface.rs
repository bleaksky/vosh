//! The native terminal renderer (see docs/renderer.md).
//!
//! The platform submodule owns the view plumbing: `install` creates the
//! native view under the webview, `place` puts the grid in the pane and
//! shows the view with its layer's scale, backdrop and corners, and the
//! rest repaints the backdrop, writes the clipboard and opens a URL.
//! macOS is the one platform: an `NSView` + `CAMetalLayer` composited
//! with the `WKWebView`, drawn by wgpu's Metal backend (`macos.rs`).
//! Windows and Linux draw with xterm.
//!
//! The surface sits BELOW the webview. It spans the whole window and
//! never moves; the grid draws at the pane's offset, the page leaves the
//! pane unpainted so the grid shows through, and DOM overlays composite
//! over live terminal pixels. Pointer input arrives from the page.
//!
//! Every window touch happens on the main thread (creation inside
//! `with_webview` / install, updates via `AppHandle::run_on_main_thread`).
//!
//! This file holds the installed surface, the pane it draws in, the
//! frame and the blink timer. `device` holds the GPU device and the
//! fonts the atlas loads, `pointer` the pointer input the page
//! forwards with the selection, scrollbar and divider drags it starts,
//! and `report` what a frame tells the page and the game.
//! `split_drag` maps a selection drag across the scrollback split.

// Platform window plumbing and the wgpu raw-handle surface are unsafe;
// the workspace forbids unsafe by default.
#![allow(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use tauri::Manager;

#[cfg(target_os = "macos")]
#[path = "surface/macos.rs"]
mod platform;

pub(crate) mod device;
pub(crate) mod pointer;
mod report;
mod split_drag;

use device::GpuState;
use pointer::{hover_url, load_f32, set_divider_frac, split_ratio, store_f32, CELLS};
pub(crate) use report::report_scroll;
use report::{forget_game_size, grid_and_game_rows, report_scroll_if_changed, report_sizes};

// The installed surface: the platform's window/view handles plus the GPU
// state. Platform handles are raw pointers, but every access is funnelled
// through the main thread, so the assertion is sound.
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

/// What decides when a frame is drawn: the redraw requests and the blink
/// timer.
struct Frames {
    // True once the frontend has positioned the surface (flag on); keeps
    // redraw requests from doing work while the surface is hidden.
    active: AtomicBool,
    // Coalesces redraw requests so a burst of output schedules one repaint.
    redraw_pending: AtomicBool,
    // A frame that flips blinking text waits on its timer. It is armed only
    // by a frame that drew text that blinks, so while nothing on screen
    // blinks no timer runs and no extra frame is drawn.
    blink_armed: AtomicBool,
    // Whether the last frame drawn showed text that blinks, for a frame
    // that gets no texture to draw on.
    last_blinks: AtomicBool,
}

static FRAMES: Frames = Frames {
    active: AtomicBool::new(false),
    redraw_pending: AtomicBool::new(false),
    blink_armed: AtomicBool::new(false),
    last_blinks: AtomicBool::new(false),
};

/// The terminal pane, from the page's latest bounds report.
struct Pane {
    // The pane rect inside the surface, in device pixels
    // [x, y, width, height]. None until the frontend first reports bounds.
    viewport: Mutex<Option<[u32; 4]>>,
    // The rows at the bottom of the pane the pinned prompt band borrows
    // while your prompt takes more than one row. Stored with the bounds,
    // so a frame never pairs new bounds with an old count.
    lent_rows: AtomicU32,
}

static PANE: Pane = Pane {
    viewport: Mutex::new(None),
    lent_rows: AtomicU32::new(0),
};

/// The pane rect clamped into a `target_w` x `target_h` render target.
/// Before the first report the pane is the whole target.
fn pane_rect(target_w: u32, target_h: u32) -> [u32; 4] {
    let full = [0, 0, target_w.max(1), target_h.max(1)];
    let Some([x, y, w, h]) = PANE.viewport.lock().ok().and_then(|v| *v) else {
        return full;
    };
    let x = x.min(target_w.saturating_sub(1));
    let y = y.min(target_h.saturating_sub(1));
    let w = w.min(target_w - x).max(1);
    let h = h.min(target_h - y).max(1);
    [x, y, w, h]
}

/// Request a repaint of the terminal surface. Called from the session loop
/// after feeding the grid. No-ops until the surface is active; coalesces
/// bursts; dispatches the actual draw to the main thread (Metal requires
/// it).
pub(crate) fn request_redraw() {
    if !FRAMES.active.load(Ordering::Acquire) {
        return;
    }
    let Some(app) = APP.get() else {
        return;
    };
    if FRAMES.redraw_pending.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = app.run_on_main_thread(redraw_now);
}

/// Another session's grid shows. The pointer lets go of what it held on
/// the grid that showed before, and a frame draws the new one and tells
/// its session the game's size. Takes no lock but the pointer's state, so
/// the session map may be held.
pub(crate) fn grid_shown() {
    pointer::let_go();
    forget_game_size();
    request_redraw();
}

/// Past the flip, so the timer's frame lands in the new half even when
/// its clock and the wall clock part by a millisecond. The page's blink
/// timer waits the same (`BLINK_SLACK_MS` in src/lib/blink.ts).
const BLINK_SLACK: std::time::Duration = std::time::Duration::from_millis(2);

/// The moment a frame draws its blinking text at, in milliseconds since
/// the epoch. None while Blinking text is off, so a frame then reads no
/// clock.
fn blink_now() -> Option<u64> {
    crate::native::gpu::style::blink_text().then(crate::native::gpu::style::epoch_ms)
}

/// Whether a frame leaves text on screen that blinks: what it drew when
/// it drew (`Some`), and what the last frame drew when it got no texture
/// (`None`). A failed frame the blink timer asked for then waits for the
/// next flip, so a hidden half it was meant to end cannot stick.
fn frame_blinks(drew: Option<bool>) -> bool {
    match drew {
        Some(blinks) => {
            FRAMES.last_blinks.store(blinks, Ordering::Release);
            blinks
        }
        None => FRAMES.last_blinks.load(Ordering::Acquire),
    }
}

/// How long after a frame at `now_ms` to draw the one that flips its
/// blinking text. None when the frame left no text that blinks
/// (`blinks`) or Blinking text is off (no `now_ms`): then nothing waits.
fn blink_wait(blinks: bool, now_ms: Option<u64>) -> Option<std::time::Duration> {
    let now_ms = now_ms.filter(|_| blinks)?;
    Some(crate::native::gpu::style::until_blink_flip(now_ms) + BLINK_SLACK)
}

/// After a frame that left text that blinks, ask for a frame at the next
/// flip after `now_ms`, the moment the frame drew its half at, unless one
/// is asked for already. The ask goes through `request_redraw`, so it
/// joins any frame the game or your typing asks for meanwhile, and that
/// frame draws at once in the half of its own moment. A flip never holds
/// a frame back.
fn arm_blink(blinks: bool, now_ms: Option<u64>) {
    let Some(wait) = blink_wait(blinks, now_ms) else {
        return;
    };
    if FRAMES.blink_armed.swap(true, Ordering::AcqRel) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(wait).await;
        FRAMES.blink_armed.store(false, Ordering::Release);
        request_redraw();
    });
}

fn redraw_now() {
    FRAMES.redraw_pending.store(false, Ordering::Release);
    if let Ok(mut slot) = surface_slot().lock() {
        if let Some(handle) = slot.as_mut() {
            // A theme change repaints through here, so the backdrop that
            // shows during a resize follows it. Cached, so cheap.
            platform::set_backdrop(&handle.platform, crate::native::gpu::style::theme_bg_rgb());
            render(&mut handle.gpu);
        }
    }
}

/// True once the surface installed and its GPU came up. The page checks
/// this before it leaves the terminal pane transparent, so a failed
/// install falls back to xterm instead of a see-through hole.
pub(crate) fn is_ready() -> bool {
    surface_slot().lock().is_ok_and(|s| s.is_some())
}

/// Install the native surface under the main window's webview, and keep
/// the app handle every redraw and report goes through. The atlas gets
/// the bundled font first, so the first atlas can load it. Runs on the
/// main thread. The surface spans the window and starts hidden, and the
/// page's first `set_bounds` call shows it and places the grid in the
/// pane. A missing handle or a GPU that fails to start only logs, so
/// `is_ready` stays false and the page draws with xterm. The error comes
/// from the webview, when it could not run the install at all.
pub(crate) fn install(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    let app = window.app_handle();
    let _ = APP.set(app.clone());
    device::hand_in_bundled_fonts(app);
    platform::install(window)
}

/// Place the grid in the terminal pane and show the surface. `x`/`y`/`w`/`h`
/// are CSS pixels in the webview's top-left coordinate space; `dpr` is the
/// device pixel ratio. `lent` is the rows at the pane's bottom the pinned
/// prompt band borrows (`grid_and_game_rows`). The metrics a frame reads
/// are kept here, and the platform's `place` puts the grid in the pane
/// before the frame draws. Must run on the main thread.
pub(crate) fn set_bounds(x: f64, y: f64, width: f64, height: f64, dpr: f64, lent: u32) {
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
    FRAMES.active.store(true, Ordering::Release);
    PANE.lent_rows.store(lent, Ordering::Release);
    store_f32(&CELLS.dpr, dpr as f32);
    platform::place(handle, x, y, width, height, dpr);
    render(&mut handle.gpu);
}

fn render(state: &mut GpuState) {
    // One moment for the half this frame draws and the flip it waits for.
    let now_ms = blink_now();
    let frame = match state.surface.get_current_texture() {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "native-surface: get_current_texture failed");
            arm_blink(frame_blinks(None), now_ms);
            return;
        }
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = state
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // Size the grid to the pane so the terminal fills it instead of a fixed
    // 80x24 corner. The pane is a rect inside the window-sized target.
    let [pane_x, pane_y, pane_w, pane_h] = pane_rect(state.config.width, state.config.height);
    let (cols, fit) = state.cell_renderer.grid_size_for(pane_w, pane_h);
    let lent = PANE.lent_rows.load(Ordering::Acquire) as usize;
    let (rows, game_rows) = grid_and_game_rows(fit, lent);
    crate::native::grid::resize_grid(cols, rows);
    report_sizes(cols, rows, game_rows);
    // Publish the cell size so the pointer code can map points to cells.
    let (cw, ch) = state.cell_renderer.cell_size_px();
    store_f32(&CELLS.cell_w, cw);
    store_f32(&CELLS.cell_h, ch);

    // Disjoint borrows of GpuState fields so the grid-reading closure can
    // hold the renderer mutably and the device/queue immutably.
    let device = &state.device;
    let queue = &state.queue;
    let placement = crate::native::gpu::Placement {
        x: pane_x,
        y: pane_y,
        scale: load_f32(&CELLS.dpr, 2.0),
        target: [state.config.width, state.config.height],
        blink_hidden: now_ms.is_some_and(|now| !crate::native::gpu::style::blink_shown(now)),
    };
    let cell_renderer = &mut state.cell_renderer;
    let drawn = crate::native::grid::with_shown(|shown| {
        let shown = shown?;
        let grid = shown.term()?;
        // Read with the grid map held, so the lock order stays surface
        // slot, then grid map, then hover.
        let (find, find_active) = shown.find().snapshot();
        let hover = hover_url();
        Some(cell_renderer.draw(
            device,
            queue,
            &mut encoder,
            &view,
            grid,
            hover,
            find,
            find_active,
            shown.prompt_bands(),
            pane_w,
            pane_h,
            split_ratio(),
            placement,
        ))
    });
    if let Some(drawn) = drawn {
        set_divider_frac(drawn.divider);
    } else {
        // No grid yet: clear to the terminal background, since this fills
        // the whole window behind the page. The pass records its clear
        // when dropped at the end of this block.
        let (bg_r, bg_g, bg_b) = crate::native::gpu::style::theme_bg_rgb();
        let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("term-surface-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(bg_r) / 255.0,
                        g: f64::from(bg_g) / 255.0,
                        b: f64::from(bg_b) / 255.0,
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
    // Every scroll path repaints through here, and the grid lock is free
    // again, so this is where the page hears about the new offset.
    report_scroll_if_changed();
    arm_blink(frame_blinks(Some(drawn.is_some_and(|d| d.blinks))), now_ms);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn nothing_waits_for_a_flip_while_nothing_blinks() {
        // A frame with no blinking text on screen arms no timer, and
        // neither does one while Blinking text is off.
        for now in [0, 599, 600, 1_234_567] {
            assert_eq!(blink_wait(false, Some(now)), None);
        }
        assert_eq!(blink_wait(true, None), None);
        assert_eq!(blink_wait(false, None), None);
        // Text that blinks waits for the next flip and a hair past it.
        assert_eq!(blink_wait(true, Some(0)), Some(Duration::from_millis(602)));
        assert_eq!(blink_wait(true, Some(1199)), Some(Duration::from_millis(3)));
    }

    #[test]
    fn a_frame_with_no_texture_waits_for_the_flip_the_last_frame_left() {
        // A frame drew blinking text, then the frame its timer asked for
        // got no texture. It still waits for the next flip, so the half
        // on screen flips then.
        assert!(frame_blinks(Some(true)));
        assert!(frame_blinks(None));
        let wait = blink_wait(frame_blinks(None), Some(1300));
        assert_eq!(wait, Some(Duration::from_millis(502)));
        // Once a frame draws with nothing blinking, a failed frame waits
        // for nothing.
        assert!(!frame_blinks(Some(false)));
        assert!(!frame_blinks(None));
        assert_eq!(blink_wait(frame_blinks(None), Some(1300)), None);
    }
}
