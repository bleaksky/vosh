//! Reads the ANSI escape sequences in the lines the game sends. Built on
//! the `vte` crate.
//!
//! - [`plain_text`] drops the escapes from a line and gives the text the
//!   session, the log and the triggers read.
//! - [`pieces`] splits a line's bytes into the stretches [`plain_text`]
//!   reads, so a span of the plain text maps back to the bytes the game
//!   sent. A highlight draws over those bytes, and the rest of the line
//!   keeps the game's colors.
//!
//! - The SGR model in `sgr.rs` and `color.rs`. Its `AnsiParser` splits a
//!   line into spans that each carry their SGR attributes. Save a scene
//!   reads them to write a line's colors as HTML, and the readable
//!   highlight tests in vosh-automation read the color of each span.

pub mod color;
pub mod parser;
pub mod sgr;

pub use color::Color;
pub use parser::{pieces, plain_text, Piece, PieceKind};
pub use sgr::{AnsiParser, Attributes, Sgr, Span};
