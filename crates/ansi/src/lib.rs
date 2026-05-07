//! ANSI escape sequence parser. Built on the `vte` crate.
//!
//! Phase 1 wires this up for tests and to seed the trigger pipeline that
//! lands in Phase 3. The actual MUD output renders through xterm.js on the
//! frontend, which handles the full ANSI repertoire including 256 color and
//! 24 bit truecolor on its own.

pub mod color;
pub mod parser;
pub mod sgr;

pub use color::Color;
pub use parser::{plain_text, AnsiParser, Span};
pub use sgr::{Attributes, Sgr};
