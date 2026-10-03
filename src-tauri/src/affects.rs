//! The affects on you, as the game last sent them. `snapshot.rs` keeps
//! the last `Char.Affects` list for a window that opens between two
//! lists, and `full.rs` keeps how full each affect was at its last cast,
//! per character, for the Affects pane gauges.

pub(crate) mod full;
pub(crate) mod snapshot;
