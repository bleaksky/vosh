//! The native terminal renderer (see docs/renderer.md). It builds
//! only on macOS. Windows and Linux draw the terminal with xterm. The
//! grid alone also builds for the tests on every platform, since the
//! session tests read it.
//!
//! `grid` is the cell grid the game's output builds, one for each
//! session, `gpu` turns the cells of the grid that shows into pixels with
//! wgpu, and `surface` places those pixels under the webview and answers
//! the page's commands. One Metal layer draws one pane, so the surface,
//! the pane, the frames and the style statics stay one for the app.
//!
//! Lock order. A frame runs on the main thread. It holds the surface
//! slot, then the grid map, which holds each session's find too, and
//! with both held it reads the hovered link and the style statics, one
//! at a time. Under the surface slot it also finds the selected session
//! for the size it reports. Selecting a session shows its grid, so it
//! takes the grid map and then the pointer's state under the session
//! map. The order runs surface slot, session map, grid map, then the
//! pointer's state and the style statics. Nothing that holds a lock
//! later in that order takes an earlier one, so no two threads can each
//! wait for the other. docs/architecture.md gives the same order beside
//! the app's.

#[cfg(native_surface)]
pub(crate) mod gpu;
pub(crate) mod grid;
#[cfg(native_surface)]
pub(crate) mod surface;
