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
//! - [`template`] parses a prompt template into tokens and the pieces the
//!   editor shows.
//! - [`format`] holds the values a template draws and the plain text of
//!   each format.
//! - [`gmcp`] keeps the latest packet of each package, the pulse and the
//!   latest Char.Prompt.
//! - [`vars`] holds the catalog of fields, the session's sources and the
//!   resolver that answers the renderer, with the hidden model.
//! - [`aabahran`] holds what Vosh knows about Aabahran alone, the PROMPT
//!   compiler among it.
//! - [`render`] draws a template as ANSI text with a span per piece.
//! - [`stage`] decides what Vosh writes around your prompt, in one output
//!   per socket read, with the regions a later output replaces.
//! - [`wrap`] is the word wrap both renderers share.
//! - `testkit`, behind the `testkit` feature, prints prompts the way the
//!   game does, for tests.

pub mod aabahran;
pub mod capture;
pub mod config;
pub mod engine;
pub mod format;
pub mod gmcp;
pub mod render;
pub mod stage;
pub mod template;
#[cfg(feature = "testkit")]
pub mod testkit;
pub mod vars;
pub mod wrap;

pub use config::{CaptureConfig, PromptConfig};
pub use engine::{GamePromptSeen, PromptEngine, SeenKind, Status, StatusReport};
pub use format::{Position, Resolved, Value};
pub use render::{render, render_str, MapValues, RenderOptions, Rendered, Span, SpanColor, Values};
pub use template::{FieldRef, Format, Template};
pub use vars::{Capture, Hidden, Resolver, Vars, Vosh};
