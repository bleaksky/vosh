//! Regex trigger engine for Vosh.
//!
//! Triggers match against the plain text of a server line (ANSI escapes
//! stripped) and take one of five actions: highlight, gag, replace, send, or
//! route. The store compiles regexes once on insert; matching pays no
//! parsing cost per line.

pub mod action;
pub mod color;
pub mod engine;
pub mod store;

pub use action::{HighlightStyle, TriggerAction};
pub use color::NamedColor;
pub use engine::{
    process, process_scoped, process_with_plain, LineResult, MatchScope, ScriptInvocation,
};
pub use store::{Trigger, TriggerError, TriggerPattern, TriggerStore, TriggerTarget};
