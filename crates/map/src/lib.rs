//! SQLite-backed map storage and BFS pathfinding for Vosh.
//!
//! Phase 7 keeps the schema narrow: rooms hold identity, coordinates, and a
//! note plus avoid flag; exits are directed edges with a string direction.
//! The auto-positioner in the session layer feeds this store from GMCP
//! Room.Info; the renderer reads it back to draw nodes and edges.

pub mod direction;
pub mod store;

pub use direction::{classify, reverse, Offset};
pub use store::{Exit, MapError, MapStore, Room};
