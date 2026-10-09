//! Everything about a profile. `live.rs` holds the profile in memory,
//! `file.rs` the profile file format, `ui.rs` its `[ui]` table and
//! `panes.rs` its pane layout. `shared.rs` holds global.toml and the
//! sharing scope, `set.rs` profiles.toml and the set of profiles,
//! `login_match.rs` which profile a login picks and `worlds.rs` the
//! worlds Vosh knows. `open.rs` holds a profile the sessions play,
//! `switch.rs` moves a session to another profile, `inactive.rs`
//! reads and edits a profile that is not active, and `export.rs` holds
//! the table an export adds after a profile's settings.

pub(crate) mod export;
pub(crate) mod file;
pub(crate) mod inactive;
pub(crate) mod live;
pub(crate) mod login_match;
pub(crate) mod open;
pub(crate) mod panes;
pub(crate) mod set;
pub(crate) mod shared;
pub(crate) mod switch;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod text_size;
pub(crate) mod ui;
pub(crate) mod worlds;
