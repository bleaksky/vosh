//! The last payload of each GMCP package a window reads as it opens.
//! Aabahran resends `Char.Affects` every tick, on each change, and at
//! login, and `Char.Vitals` and `Char.Combat` as they change, and the
//! session loop emits each one as `session://gmcp/<package>`. A window
//! that opens between those, Settings among them, reads these payloads
//! so it shows your affects and vitals at once instead of after the next
//! packet.

use serde_json::Value;

use crate::affects::AFFECTS_PACKAGE;

/// The package of your health, mana and moves.
pub(crate) const VITALS_PACKAGE: &str = "Char.Vitals";
/// The package of the fight you are in.
pub(crate) const COMBAT_PACKAGE: &str = "Char.Combat";

/// The packages whose last payload the session keeps.
const KEPT: [&str; 3] = [AFFECTS_PACKAGE, VITALS_PACKAGE, COMBAT_PACKAGE];

/// The last payload of each package in [`KEPT`], each None before the
/// first one of a connection and after it ends. Held under a std mutex
/// because the work inside the lock is one clone.
#[derive(Debug, Default)]
pub(crate) struct LastPackages(std::sync::Mutex<[Option<Value>; KEPT.len()]>);

impl LastPackages {
    /// Keep `data` when the session keeps `package`.
    pub(crate) fn observe(&self, package: &str, data: &Value) {
        let Some(slot) = KEPT.iter().position(|kept| *kept == package) else {
            return;
        };
        if let Ok(mut guard) = self.0.lock() {
            guard[slot] = Some(data.clone());
        }
    }

    /// Forget every payload, as a new connection or a disconnect makes
    /// them stale.
    pub(crate) fn clear(&self) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = Default::default();
        }
    }

    /// The last payload of `package`, or None when the session keeps
    /// none.
    pub(crate) fn get(&self, package: &str) -> Option<Value> {
        let slot = KEPT.iter().position(|kept| *kept == package)?;
        self.0.lock().ok().and_then(|guard| guard[slot].clone())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn keeps_the_last_affects_list_until_cleared() {
        let snapshot = LastPackages::default();
        assert_eq!(snapshot.get(AFFECTS_PACKAGE), None);

        let first = json!({ "affects": [{ "name": "sanctuary", "duration": 12 }] });
        let second = json!({ "affects": [{ "name": "haste", "duration": 3 }] });
        snapshot.observe(AFFECTS_PACKAGE, &first);
        assert_eq!(snapshot.get(AFFECTS_PACKAGE), Some(first));
        snapshot.observe(AFFECTS_PACKAGE, &second);
        assert_eq!(snapshot.get(AFFECTS_PACKAGE), Some(second.clone()));

        // Other packages leave it alone.
        snapshot.observe(VITALS_PACKAGE, &json!({ "hp": 100 }));
        snapshot.observe("Char.Affects.Add", &json!({ "name": "fly" }));
        assert_eq!(snapshot.get(AFFECTS_PACKAGE), Some(second));

        snapshot.clear();
        assert_eq!(snapshot.get(AFFECTS_PACKAGE), None);
    }

    #[test]
    fn keeps_the_last_vitals_and_combat_until_cleared() {
        let snapshot = LastPackages::default();
        assert_eq!(snapshot.get(VITALS_PACKAGE), None);
        assert_eq!(snapshot.get(COMBAT_PACKAGE), None);

        let first =
            json!({ "hp": 0, "maxhp": 0, "mana": 0, "maxmana": 0, "move": 0, "maxmove": 0 });
        let second = json!({
            "hp": 850, "maxhp": 900, "mana": 760, "maxmana": 820, "move": 250, "maxmove": 250
        });
        let fight = json!({
            "target": "a Blackwatch guard", "condition": "quite a few wounds", "hp_pct": 54
        });
        snapshot.observe(VITALS_PACKAGE, &first);
        snapshot.observe(VITALS_PACKAGE, &second);
        snapshot.observe(COMBAT_PACKAGE, &fight);
        assert_eq!(snapshot.get(VITALS_PACKAGE), Some(second.clone()));
        assert_eq!(snapshot.get(COMBAT_PACKAGE), Some(fight.clone()));
        assert_eq!(snapshot.get(AFFECTS_PACKAGE), None);

        // A package the session does not keep stays out.
        snapshot.observe("Room.Info", &json!({ "num": 3001 }));
        assert_eq!(snapshot.get("Room.Info"), None);
        assert_eq!(snapshot.get(VITALS_PACKAGE), Some(second));
        assert_eq!(snapshot.get(COMBAT_PACKAGE), Some(fight));

        snapshot.clear();
        assert_eq!(snapshot.get(VITALS_PACKAGE), None);
        assert_eq!(snapshot.get(COMBAT_PACKAGE), None);
    }
}
