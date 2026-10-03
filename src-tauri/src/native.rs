//! The native terminal renderer (see docs/native-renderer.md). It builds
//! only on macOS. Windows and Linux draw the terminal with xterm.
//!
//! `grid` is the cell grid the game's output builds, `gpu` turns its
//! cells into pixels with wgpu, and `surface` places those pixels under
//! the webview and answers the page's commands.

pub(crate) mod gpu;
pub(crate) mod grid;
pub(crate) mod surface;
