//! What the prompt card on the page receives.
//!
//! - [`state`] reports each field's live state and source.
//! - [`describe`] says what each piece and token of a design is, with the
//!   forms a value takes.
//! - [`edit`] writes the template changes the editor makes, keeping the
//!   look of every other piece.
//! - [`report`] says what a capture compiles to, with the [`presets`] it
//!   offers to start from.
//! - [`presets`] holds the designs Vosh ships, its default among them.
//! - [`candidates`] groups the candidates ring by shape and checks a
//!   capture against it and your scrollback.
//! - [`sentences`] says what a setting reads and shows, with the label
//!   each code and value goes by.

pub mod candidates;
pub mod describe;
pub mod edit;
pub mod presets;
pub mod report;
pub mod sentences;
pub mod state;
