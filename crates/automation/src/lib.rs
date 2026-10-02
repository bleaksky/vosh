//! Aliases, variables and triggers, as plain data with no Lua.
//!
//! A script alias or a script trigger only hands its Lua body back.
//! vosh-script runs it, so the Lua build stays out of this crate.
//!
//! [`alias`] expands the first word of a command into the commands it
//! stands for.
//! [`vars`] keeps variables in profile and session scope and fills in
//! `$name`.
//! [`trigger`] matches server lines and fires highlight, gag, replace,
//! send, route and script actions.

pub mod alias;
mod revision;
pub mod trigger;
pub mod vars;
