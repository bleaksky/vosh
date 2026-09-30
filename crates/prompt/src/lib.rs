//! The custom prompt engine.
//!
//! This crate holds every byte decision about the prompt Vosh draws, with
//! no Tauri, so each one is testable on its own. The session task feeds it
//! and emits what it returns.
//!
//! - [`template`] parses a prompt template into tokens and the pieces the
//!   editor shows.

pub mod template;

pub use template::{FieldRef, Format, Template};
