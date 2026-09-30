//! The custom prompt engine.
//!
//! This crate holds every byte decision about the prompt Vosh draws, with
//! no Tauri, so each one is testable on its own. The session task feeds it
//! and emits what it returns.
//!
//! - [`template`] parses a prompt template into tokens and the pieces the
//!   editor shows.
//! - [`format`] holds the values a template draws and the plain text of
//!   each format.
//! - [`gmcp`] keeps the latest packet of each package, the pulse and the
//!   latest Char.Prompt.
//! - [`vars`] holds the catalog of fields, the session's sources and the
//!   resolver that answers the renderer.
//! - [`render`] draws a template as ANSI text with a span per piece.
//! - [`wrap`] is the word wrap both renderers share.

pub mod format;
pub mod gmcp;
pub mod render;
pub mod template;
pub mod vars;
pub mod wrap;

pub use format::{Position, Resolved, Value};
pub use render::{render, render_str, MapValues, RenderOptions, Rendered, Span, SpanColor, Values};
pub use template::{FieldRef, Format, Template};
pub use vars::{Capture, Resolver, Vars, Vosh};
