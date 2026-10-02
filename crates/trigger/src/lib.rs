//! Regex trigger engine for Vosh.
//!
//! Triggers match against the plain text of a server line (ANSI escapes
//! stripped) and fire any of six actions: highlight, gag, replace, send,
//! route, or a Lua script. The store compiles regexes once on insert;
//! matching pays no parsing cost per line.
//!
//! A trigger's target picks the lines it sees. `Line` takes each completed
//! line and `Prompt` the prompt text the game ends with GA or EOR. `Room`
//! takes the lines a room look lists after its exits, and `RoomTarget`
//! (Your target in Settings) the line of the one you target among them.
//!
//! The `readable` module keeps the colors triggers paint readable on the
//! theme's ground.

pub mod action;
pub mod color;
pub mod engine;
pub mod readable;
pub mod store;

pub use action::{HighlightStyle, TriggerAction};
pub use color::NamedColor;
pub use engine::{matching, process_on_ground, LineResult, MatchScope, ScriptInvocation};
#[cfg(any(test, feature = "testkit"))]
pub use engine::{process, process_scoped};
pub use store::{Trigger, TriggerError, TriggerPattern, TriggerStore, TriggerTarget};
