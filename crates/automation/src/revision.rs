//! The list revision counter the alias and trigger stores share.

use std::sync::atomic::{AtomicU64, Ordering};

/// Hands out list revisions. One counter serves every store, so a store
/// built to replace another never reads as the same list by accident.
static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}
