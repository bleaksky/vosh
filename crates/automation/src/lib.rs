//! Aliases, variables and triggers, as plain data with no Lua.
//!
//! A script alias or a script trigger only hands its Lua body back, as a
//! [`ScriptCall`]. vosh-script runs it, so the Lua build stays out of
//! this crate.
//!
//! [`alias`] expands the first word of a command into the commands it
//! stands for.
//! [`vars`] keeps variables in profile and session scope and fills in
//! `$name`.
//! [`trigger`] matches server lines and fires highlight, gag, replace,
//! send, route and script actions.
//! [`alert`] says what an alert on a trigger, a preset or a Lua call does.

pub mod alert;
pub mod alias;
mod groups;
mod revision;
mod script_call;
mod split;
mod stops;
pub mod trigger;
pub mod vars;

pub use groups::compare_groups;
pub use script_call::ScriptCall;
pub use stops::StopKey;
