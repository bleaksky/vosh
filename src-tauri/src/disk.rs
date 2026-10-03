//! How Vosh writes files to disk. `atomic.rs` holds the safe writes and
//! the list of files Vosh could not read, and `save.rs` the save engine
//! that writes the live profile.

pub(crate) mod atomic;
pub(crate) mod save;
