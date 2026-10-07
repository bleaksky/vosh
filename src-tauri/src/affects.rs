//! The affects on you, as the game last sent them. `full.rs` keeps how
//! full each affect was at its last cast, per character, for the Affects
//! pane gauges. The session keeps the last `Char.Affects` list for a
//! window that opens between two lists, see
//! [`crate::session::last_packages`].

pub(crate) mod full;

/// The package of the affects on you.
pub(crate) const AFFECTS_PACKAGE: &str = "Char.Affects";
