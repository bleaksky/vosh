//! Aliases, variables and triggers, as plain data with no Lua.
//!
//! A script alias or a script trigger only hands its Lua body back.
//! vosh-script runs it, so the Lua build stays out of this crate.
//!
//! [`alias`] expands the first word of a command into the commands it
//! stands for.
//! [`vars`] keeps variables in profile and session scope and fills in
//! `$name`.

pub mod alias;
pub mod vars;
