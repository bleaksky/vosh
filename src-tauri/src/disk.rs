//! How Vosh writes files to disk. `paths.rs` names every file in the app
//! data folder, `atomic.rs` holds the safe writes and the list of files
//! Vosh could not read, `custom_themes.rs` the move that gathers the
//! custom themes profile files hold into global.toml, `save.rs` the save
//! engine that writes the live profile, and `upgrades/` the one time
//! upgrades.

pub(crate) mod atomic;
pub(crate) mod custom_themes;
pub(crate) mod paths;
pub(crate) mod save;
pub(crate) mod upgrades;
