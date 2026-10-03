//! The shared catalog wizard, which moves the aliases, triggers, and
//! macros of every profile file into catalog.toml.

pub(crate) mod apply;
mod groups;
pub(crate) mod journal;
pub(crate) mod plan;
#[cfg(test)]
mod tests;
