//! The one time upgrades, each recorded under its id in `profiles.toml`.
//! Launch runs them before any profile loads, after a shared catalog
//! wizard run finishes. `prompt_capture.rs` moves the prompt capture
//! triggers into the profiles first, then `presets.rs` turns on each
//! preset a build adds.

pub(crate) mod presets;
pub(crate) mod prompt_capture;
