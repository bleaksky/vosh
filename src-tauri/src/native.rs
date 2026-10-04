//! The native terminal renderer (see docs/native-renderer.md). It builds
//! only on macOS. Windows and Linux draw the terminal with xterm. The
//! grid alone also builds for the tests on every platform, since the
//! session tests read it.
//!
//! `grid` is the cell grid the game's output builds, `gpu` turns its
//! cells into pixels with wgpu, and `surface` places those pixels under
//! the webview and answers the page's commands.
//!
//! Lock order. A frame runs on the main thread. It holds the surface
//! slot, then the shared grid, and with both held it reads the find
//! list, the hovered link and the style statics, one at a time. Nothing
//! that holds a lock later in that order takes an earlier one, so no two
//! threads can each wait for the other. docs/architecture.md gives the
//! same order beside the app's.

#[cfg(native_surface)]
pub(crate) mod gpu;
pub(crate) mod grid;
#[cfg(native_surface)]
pub(crate) mod surface;
