//! Loadout mode, where every character shares one catalog of aliases,
//! triggers, and macros. `catalog.rs` holds catalog.toml and `set.rs`
//! loadouts.toml. `gating.rs` lays the group state of the active
//! loadouts over the live profile, and `presets.rs` keeps the list of
//! trigger presets that are on. `wizard/` builds the catalog from the
//! profile files.
//!
//! ## Concept
//!
//! Today every alias / trigger / macro lives inside a specific
//! profile, and switching profiles swaps the whole authored content
//! base. Loadout mode inverts that:
//!
//!   - **[`GlobalCatalog`]** holds every item the user ever defined.
//!     Items are gated for effective enable/disable by their `group`
//!     field — the same per-store `disabled_groups` machinery added
//!     in v0.3.0.
//!   - **[`Loadout`]** is a named set of groups to enable. The
//!     per-character state (vars, tick config, timers, UI settings)
//!     stays in each profile file, which loadout mode loads as per
//!     profile mode does.
//!   - **[`LoadoutSet`]** holds every loadout the user has plus a
//!     list of currently-active ones. Multiple loadouts can stack:
//!     the runtime enables the union of `enabled_groups` across
//!     every currently-active loadout (stack-by-union).
//!
//! [`Loadout`]: set::Loadout

pub(crate) mod catalog;
pub(crate) mod gating;
pub(crate) mod presets;
pub(crate) mod set;
#[cfg(test)]
mod tests;
pub(crate) mod wizard;

use std::path::Path;

use thiserror::Error;

use catalog::{catalog_path, load_global_catalog, GlobalCatalog, UNREAD_CATALOG_NOTICE};
use set::{load_loadout_set, loadouts_path, LoadoutSet, UNREAD_LOADOUTS_NOTICE};

#[derive(Debug, Error)]
pub(crate) enum LoadoutStoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("toml parse error: {0}")]
    Deserialize(#[from] toml::de::Error),
}

/// Read catalog.toml and loadouts.toml at launch in loadout mode. When
/// either one does not read, the session runs on the profile files alone,
/// so a save from it would write a catalog without your shared items.
/// Vosh then holds both files with [`crate::disk::atomic::hold_unread`],
/// since the pair only makes sense together, and the error carries the
/// sentences that tell you so.
pub(crate) fn load_at_launch(app_data: &Path) -> Result<(GlobalCatalog, LoadoutSet), Vec<String>> {
    let catalog = load_global_catalog(app_data);
    let set = load_loadout_set(app_data);
    let mut notices = Vec::new();
    if let Err(e) = &catalog {
        tracing::error!(
            error = %e,
            path = %catalog_path(app_data).display(),
            "catalog.toml unreadable at startup; it will not be saved over",
        );
        notices.push(UNREAD_CATALOG_NOTICE.to_string());
    }
    if let Err(e) = &set {
        tracing::error!(
            error = %e,
            path = %loadouts_path(app_data).display(),
            "loadouts.toml unreadable at startup; it will not be saved over",
        );
        notices.push(UNREAD_LOADOUTS_NOTICE.to_string());
    }
    match (catalog, set) {
        (Ok(catalog), Ok(set)) => Ok((catalog, set)),
        _ => {
            crate::disk::atomic::hold_unread(&catalog_path(app_data));
            crate::disk::atomic::hold_unread(&loadouts_path(app_data));
            Err(notices)
        }
    }
}
