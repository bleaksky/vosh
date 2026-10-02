//! The custom prompt engine.
//!
//! This crate holds every byte decision about the prompt Vosh draws, with
//! no Tauri, so each one is testable on its own. The session task feeds it
//! and emits what it returns.
//!
//! - [`config`] is the `[prompt]` table of a profile file.
//! - [`engine`] is the live profile's custom prompt, its table and the
//!   session's variables together.
//! - [`capture`] reads patterns you point at, and the capture triggers
//!   older builds used, into a capture.
//! - [`design`] is the template language. It parses a design into tokens
//!   and the pieces the editor shows, writes tokens back as text, and
//!   holds the look algebra the editor uses to keep each piece's look.
//! - [`edit`] writes the template changes the editor makes, keeping the
//!   look of every other piece.
//! - [`describe`] says what each piece and token of a design is, for the
//!   card, with the forms a value takes.
//! - [`format`] holds the values a template draws and the plain text of
//!   each format.
//! - [`generic`] builds a capture from a line another game prints.
//! - [`gmcp`] keeps the latest packet of each package, the pulse and the
//!   latest Char.Prompt.
//! - [`vars`] holds the catalog of fields, the session's sources and the
//!   resolver that answers the renderer, with the hidden model.
//! - [`aabahran`] holds what Vosh knows about Aabahran alone, the PROMPT
//!   compiler among it.
//! - [`render`] draws a template as ANSI text with a span per piece.
//! - [`presets`] holds the designs Vosh ships, its default among them.
//! - [`report`] says what a capture compiles to, for the card, with the
//!   [`presets`] it offers to start from.
//! - [`stage`] decides what Vosh writes around your prompt, in one output
//!   per socket read, with the regions a later output replaces.
//! - [`candidates`] groups the candidates ring by shape and checks a
//!   capture against it and your scrollback.
//! - [`overrides`] draws a preview's values in place of the live ones.
//! - [`state`] reports each field's live state and source for the card.
//! - [`wrap`] is the word wrap both renderers share.
//! - `testkit`, behind the `testkit` feature, prints prompts the way the
//!   game does and plays a fake Aabahran for tests and scripted runs.

pub mod aabahran;
pub mod candidates;
pub mod capture;
pub mod config;
pub mod describe;
pub mod design;
pub mod edit;
pub mod engine;
pub mod generic;
pub mod presets;
pub mod render;
pub mod report;
pub mod stage;
pub mod state;
#[cfg(feature = "testkit")]
pub mod testkit;
pub mod values;
pub mod wrap;

pub use config::{CaptureConfig, PromptConfig, PromptShow};
pub use design::{FieldRef, Template};
pub use engine::{GamePromptSeen, PromptEngine, Status};
pub use presets::DEFAULT_DESIGN;
pub use render::{render, render_str, RenderOptions, Rendered, Span, Values};
pub use values::format::{Resolved, Value};
pub use values::{Capture, Vosh};

// The old names of `values` and the modules it now holds, so their
// callers keep compiling until their imports move to the new paths.
pub use values as vars;
pub use values::{format, gmcp, overrides};

// The tests in `tests/` import these from the root.
#[cfg(feature = "testkit")]
pub use render::SpanColor;
#[cfg(feature = "testkit")]
pub use testkit::map_values::MapValues;
#[cfg(feature = "testkit")]
pub use values::Vars;
