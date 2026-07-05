//! Windows window glue for the native surface: a child `HWND` created over
//! the `WebView2` inside the Tauri window, drawn by wgpu's D3D12 backend.
//! The window procedure mirrors the macOS view's mouse handling (wheel
//! scroll, selection drag, divider drag, Ctrl+click URLs, hover tracking,
//! divider resize cursor) through the shared pointer logic in the parent
//! module. Every window touch happens on the main (message-loop) thread.
//!
//! Written against the Win32 API without a Windows machine in the loop:
//! compile-checked cross-target; runtime behavior needs a hardware pass
//! before the frontend enables the surface on Windows.

use std::ffi::c_void;
use std::num::NonZeroIsize;
use std::sync::OnceLock;

use raw_window_handle::{
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, LoadCursorW, RegisterClassW, SetCursor,
    SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW, HWND_TOP, IDC_ARROW, IDC_SIZENS,
    SWP_NOACTIVATE, SW_HIDE, SW_SHOWNA, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_NCDESTROY, WM_SETCURSOR, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS,
};

// Stable Win32 values that windows-sys scatters across feature modules.
const MK_LBUTTON: usize = 0x0001;
const MK_CONTROL: usize = 0x0008;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_MBUTTONDOWN: u32 = 0x0207;

use super::{
    divider_frac, middle_click, pointer_down, pointer_dragged, pointer_moved, pointer_up, render,
    surface_slot, wheel_scroll, PointerEvent, SurfaceHandle,
};

/// wgpu backend for this platform.
const BACKENDS: wgpu::Backends = wgpu::Backends::DX12;

// One WHEEL_DELTA notch (120) scrolls three grid lines through the shared
// accumulator (which converts point deltas at 0.12 lines per point).
const WHEEL_POINTS_PER_NOTCH: f64 = 25.0;

/// The platform's window handle: the child `HWND` hosting the swapchain.
/// Raw pointers are not Send, but every access is funnelled through the
/// main thread, so the marker is sound.
pub(super) struct PlatformSurface {
    hwnd: HWND,
}
unsafe impl Send for PlatformSurface {}

/// Hide or show the surface window. Main thread only.
pub(super) fn set_hidden(platform: &PlatformSurface, hidden: bool) {
    // SAFETY: main thread; the window is live until NCDESTROY.
    unsafe {
        let _ = ShowWindow(platform.hwnd, if hidden { SW_HIDE } else { SW_SHOWNA });
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
    // SAFETY: main thread; the window is live.
    unsafe {
        let _ = SetWindowPos(
            platform.hwnd,
            HWND_TOP,
            px(x),
            px(y),
            px(width).max(1),
            px(height).max(1),
            SWP_NOACTIVATE,
        );
    }
}

/// Post-redraw hook. The divider cursor is handled in `WM_SETCURSOR`, so
/// nothing to refresh here.
pub(super) fn after_redraw(_platform: &PlatformSurface) {}

/// Put `text` on the clipboard as `CF_UNICODETEXT`.
pub(super) fn set_clipboard(text: &str) {
    const CF_UNICODETEXT: u32 = 13;
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = utf16.len() * 2;
    // SAFETY: standard Win32 clipboard write on the main thread; the global
    // allocation is handed off to the clipboard on success.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return;
        }
        let _ = EmptyClipboard();
        let hglobal = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if !hglobal.is_null() {
            let dst = GlobalLock(hglobal);
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(utf16.as_ptr().cast::<u8>(), dst.cast::<u8>(), bytes);
                let _ = GlobalUnlock(hglobal);
                let _ = SetClipboardData(CF_UNICODETEXT, hglobal.cast::<c_void>());
            }
        }
        let _ = CloseClipboard();
    }
}

/// Open a URL in the default browser via `ShellExecuteW`.
pub(super) fn open_url(url: &str) {
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let target: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: standard shell open on the main thread.
    unsafe {
        let _ = ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        );
    }
}

/// Split an `LPARAM` mouse position into signed client coordinates
/// (physical pixels on Windows).
fn mouse_pos(lparam: LPARAM) -> (f64, f64) {
    let x = (lparam & 0xffff) as u16 as i16;
    let y = ((lparam >> 16) & 0xffff) as u16 as i16;
    (f64::from(x), f64::from(y))
}

/// Build the shared pointer event (client pixels, top-left origin) from a
/// window-message mouse position.
fn pointer_event(hwnd: HWND, lparam: LPARAM, open_modifier: bool) -> PointerEvent {
    let (x, y) = mouse_pos(lparam);
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: the window handle comes from a live WndProc callback.
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    PointerEvent {
        x,
        y,
        width: f64::from(rect.right - rect.left),
        height: f64::from(rect.bottom - rect.top),
        open_modifier,
    }
}

/// Ask for a `WM_MOUSELEAVE` when the pointer exits, so hover clears.
fn track_leave(hwnd: HWND) {
    let mut track = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    // SAFETY: the window handle comes from a live WndProc callback.
    unsafe {
        let _ = TrackMouseEvent(&mut track);
    }
}

/// The surface window procedure: wheel scroll, selection/divider drags,
/// Ctrl+click URLs, hover tracking, and the divider resize cursor, all
/// through the shared pointer logic in the parent module.
extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEWHEEL => {
            // High word of wparam is the signed wheel delta in 1/120 notches.
            let delta = ((wparam >> 16) & 0xffff) as u16 as i16;
            wheel_scroll(f64::from(delta) / 120.0 * WHEEL_POINTS_PER_NOTCH);
            0
        }
        WM_LBUTTONDOWN => {
            // SAFETY: live window in its own WndProc.
            unsafe {
                SetCapture(hwnd);
            }
            pointer_down(&pointer_event(hwnd, lparam, wparam & MK_CONTROL != 0));
            0
        }
        WM_MBUTTONDOWN => {
            // Middle-click toggles the split-scrollback view.
            middle_click();
            0
        }
        WM_MOUSEMOVE => {
            track_leave(hwnd);
            let ev = pointer_event(hwnd, lparam, false);
            if wparam & MK_LBUTTON != 0 {
                pointer_dragged(&ev);
            } else {
                pointer_moved(Some(&ev));
            }
            0
        }
        WM_LBUTTONUP => {
            // SAFETY: releasing this window's own capture.
            unsafe {
                let _ = ReleaseCapture();
            }
            pointer_up();
            0
        }
        WM_MOUSELEAVE => {
            pointer_moved(None);
            0
        }
        WM_SETCURSOR => {
            // Arrow everywhere; vertical-resize over the divider band. A
            // cursor position query would need GetCursorPos + ScreenToClient,
            // so approximate with the divider fraction from the last frame.
            let cursor = if divider_frac().is_some() {
                IDC_SIZENS
            } else {
                IDC_ARROW
            };
            // SAFETY: LoadCursorW with a system cursor id is always valid.
            unsafe {
                let _ = SetCursor(LoadCursorW(std::ptr::null_mut(), cursor));
            }
            1
        }
        WM_NCDESTROY => 0,
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Register the surface window class once.
fn surface_class() -> *const u16 {
    static CLASS_NAME: OnceLock<Vec<u16>> = OnceLock::new();
    let name = CLASS_NAME.get_or_init(|| {
        let name: Vec<u16> = "VoshSurfaceWindow\0".encode_utf16().collect();
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            // SAFETY: current-module handle; null instance is valid here.
            hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: name.as_ptr(),
        };
        // SAFETY: one-time class registration with a live wnd_proc.
        unsafe {
            let _ = RegisterClassW(&class);
        }
        name
    });
    name.as_ptr()
}

/// Create the surface child window over the Tauri window and stand up the
/// GPU. Best-effort: logs and returns on any missing handle. Runs on the
/// main thread.
// Result-shaped to match the macOS install (whose with_webview call is
// genuinely fallible); this one only warns.
#[allow(clippy::unnecessary_wraps)]
pub(super) fn install(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    let Ok(handle) = window.window_handle() else {
        tracing::warn!("native-surface: no window handle");
        return Ok(());
    };
    let RawWindowHandle::Win32(parent) = handle.as_raw() else {
        tracing::warn!("native-surface: expected a Win32 window handle");
        return Ok(());
    };
    let parent_hwnd = parent.hwnd.get() as HWND;

    // SAFETY: main thread; the parent window is live during setup.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            surface_class(),
            std::ptr::null(),
            // Start hidden (no WS_VISIBLE): the surface is opaque and would
            // occlude xterm until the frontend opts in and reports bounds.
            WS_CHILD | WS_CLIPSIBLINGS,
            0,
            0,
            320,
            200,
            parent_hwnd,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        tracing::warn!("native-surface: CreateWindowExW failed");
        return Ok(());
    }

    // SAFETY: the child window is live.
    let scale = unsafe { f64::from(GetDpiForWindow(hwnd)) / 96.0 };
    let (font_stack, font_px) = super::font_atlas_params(scale.max(1.0));

    let Some(nz) = NonZeroIsize::new(hwnd as isize) else {
        tracing::warn!("native-surface: child HWND was null for wgpu");
        return Ok(());
    };
    let window_handle = RawWindowHandle::Win32(Win32WindowHandle::new(nz));
    let display_handle = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
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
                platform: PlatformSurface { hwnd },
                gpu,
            };
            render(&mut handle.gpu);
            if let Ok(mut slot) = surface_slot().lock() {
                *slot = Some(handle);
            }
            tracing::info!("native-surface: windows surface installed");
        }
        None => tracing::warn!("native-surface: wgpu init failed; native window is blank"),
    }
    Ok(())
}
