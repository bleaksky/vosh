//! The custom prompt engine.
//!
//! This crate holds every byte decision about the prompt Vosh draws, with
//! no Tauri, so each one is testable on its own. The session task feeds it
//! and emits what it returns.
//!
//! - [`config`] is the `[prompt]` table of a profile file, with the
//!   default design a fresh table takes.
//! - [`engine`] is the live profile's custom prompt, its table and the
//!   session's variables together.
//! - [`capture`] reads patterns you point at, and the capture triggers
//!   older builds used, into a capture.
//!   - [`capture::generic`] builds a capture from a line another game
//!     prints.
//! - [`design`] is the template language. It parses a design into tokens
//!   and the pieces the editor shows, writes tokens back as text, and
//!   holds the look algebra the editor uses to keep each piece's look.
//! - [`values`] holds the catalog of fields and their samples, the
//!   session's sources and the resolver that answers the renderer, with
//!   the hidden model.
//!   - [`values::format`] holds the values a template draws and the plain
//!     text of each format.
//!   - [`values::gmcp`] keeps the latest packet of each package, the pulse
//!     and the latest Char.Prompt.
//!   - [`values::overrides`] draws a preview's values in place of the live
//!     ones.
//! - [`aabahran`] holds what Vosh knows about Aabahran alone, the PROMPT
//!   compiler among it.
//! - [`render`] draws a template as ANSI text with a span per piece.
//! - [`stage`] decides what Vosh writes around your prompt, in one output
//!   per socket read, with the regions a later output replaces.
//! - [`card`] is what the prompt card on the page receives.
//!   - [`card::state`] reports each field's live state and source.
//!   - [`card::describe`] says what each piece and token of a design is,
//!     with the forms a value takes.
//!   - [`card::edit`] writes the template changes the editor makes,
//!     keeping the look of every other piece.
//!   - [`card::report`] says what a capture compiles to, with the
//!     [`card::presets`] it offers to start from.
//!   - [`card::presets`] holds the designs Vosh ships, its default among
//!     them.
//!   - [`card::candidates`] groups the candidates ring by shape and checks
//!     a capture against it and your scrollback.
//!   - [`card::sentences`] says what a setting reads and shows, with the
//!     label each code and value goes by.
//! - [`wrap`] is the word wrap both renderers share.
//! - `testkit`, behind the `testkit` feature, prints prompts the way the
//!   game does and plays a fake Aabahran for tests and scripted runs.

pub mod aabahran;
pub mod capture;
pub mod card;
pub mod config;
pub mod design;
pub mod engine;
pub mod render;
pub mod stage;
#[cfg(feature = "testkit")]
pub mod testkit;
pub mod values;
pub mod wrap;

pub use config::{CaptureConfig, PromptConfig, PromptShow, DEFAULT_DESIGN};
pub use design::{FieldRef, Template};
pub use engine::{GamePromptSeen, PromptEngine, Status};
pub use render::{render, render_str, RenderOptions, Rendered, Span, Values};
pub use values::format::{Resolved, Value};
pub use values::{Capture, Vosh};

// The tests in `tests/` import these from the root.
#[cfg(feature = "testkit")]
pub use render::SpanColor;
#[cfg(feature = "testkit")]
pub use testkit::map_values::MapValues;
#[cfg(feature = "testkit")]
pub use values::Vars;
