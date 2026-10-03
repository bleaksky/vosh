//! The last `Char.Affects` list of this connection. Aabahran resends the
//! list every tick, on each change, and at login, and the session loop
//! emits each one as `session://gmcp/Char-Affects`. A window that opens
//! between those, Settings among them, reads this snapshot so it shows
//! the affects on you at once instead of after the next tick.

use serde_json::Value;

/// The package whose payload the snapshot keeps.
pub(crate) const AFFECTS_PACKAGE: &str = "Char.Affects";

/// The last `Char.Affects` payload, or None before the first one of a
/// connection and after it ends. Held under a std mutex because the
/// work inside the lock is one clone.
#[derive(Debug, Default)]
pub(crate) struct AffectsSnapshot(std::sync::Mutex<Option<Value>>);

impl AffectsSnapshot {
    /// Keep `data` when `package` is `Char.Affects`.
    pub(crate) fn observe(&self, package: &str, data: &Value) {
        if package != AFFECTS_PACKAGE {
            return;
        }
        if let Ok(mut guard) = self.0.lock() {
            *guard = Some(data.clone());
        }
    }

    /// Forget the list, as a new connection or a disconnect makes it
    /// stale.
    pub(crate) fn clear(&self) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = None;
        }
    }

    pub(crate) fn get(&self) -> Option<Value> {
        self.0.lock().ok().and_then(|guard| guard.clone())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn keeps_the_last_affects_list_until_cleared() {
        let snapshot = AffectsSnapshot::default();
        assert_eq!(snapshot.get(), None);

        let first = json!({ "affects": [{ "name": "sanctuary", "duration": 12 }] });
        let second = json!({ "affects": [{ "name": "haste", "duration": 3 }] });
        snapshot.observe(AFFECTS_PACKAGE, &first);
        assert_eq!(snapshot.get(), Some(first));
        snapshot.observe(AFFECTS_PACKAGE, &second);
        assert_eq!(snapshot.get(), Some(second.clone()));

        // Other packages leave it alone.
        snapshot.observe("Char.Vitals", &json!({ "hp": 100 }));
        snapshot.observe("Char.Affects.Add", &json!({ "name": "fly" }));
        assert_eq!(snapshot.get(), Some(second));

        snapshot.clear();
        assert_eq!(snapshot.get(), None);
    }
}
