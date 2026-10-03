//! The shared catalog wizard, which moves the aliases, triggers, and
//! macros of every profile file into catalog.toml.

pub(crate) mod apply;
pub(crate) mod journal;
#[cfg(test)]
mod tests;
