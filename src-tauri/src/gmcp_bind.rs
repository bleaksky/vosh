//! Auto-bind common GMCP packages onto session-scoped variables so users
//! can reference them in aliases, triggers, and (later) scripts. Phase 4
//! covers `Char.Vitals`, `Char.Status`, `Char.Name`, and `Room.Info`.
//! `Char.Combat` was wired in as part of the deferred GMCP follow-ups so
//! current target name and HP show up in the prompt-area HUD.

use serde_json::Value;
use vosh_protocol::gmcp::Message;
use vosh_vars::{Scope, VariableStore};

/// Push fields from a known GMCP package into the session variable store.
/// Unknown packages are ignored so the auto-bind stays opt-in.
pub(crate) fn apply(vars: &mut VariableStore, msg: &Message) {
    if msg.package == "Char.Combat" {
        apply_char_combat(vars, &msg.data);
        return;
    }
    let prefix = match msg.package.as_str() {
        "Char.Vitals" => "",
        "Char.Status" | "Char.Name" => "char_",
        "Room.Info" => "room_",
        _ => return,
    };
    let Value::Object(map) = &msg.data else {
        return;
    };
    for (key, value) in map {
        // Aabahran adds `"hidden":true` to Char.Vitals under lamented
        // tears and leaves it out otherwise. It is not a vital, and a
        // variable bound from it would never clear.
        if msg.package == "Char.Vitals" && key == HIDDEN_FLAG {
            continue;
        }
        let var_name = format!("{prefix}{key}");
        let value_str = stringify(value);
        vars.set(Scope::Session, var_name, value_str);
    }
}

/// Bind `Char.Combat` fields under a `target_` prefix. Aabahran ships
/// `{target: "<name>", hp_pct: <0-100>, condition: "<phrase>"}` while in
/// combat and an empty object once the target is gone. Map the field
/// names into clean variable names (`target_name`, `target_hp`,
/// `target_condition`) and clear them when the payload is empty so
/// triggers can detect "lost target". A fight packet without `hp_pct`
/// or `condition` clears that one, since the game withholds it (under
/// lamented tears, blind, against mirror image, or with the target in
/// another room) and the last reading no longer holds. The `tank`
/// object and the `hidden` flag bind nothing.
fn apply_char_combat(vars: &mut VariableStore, data: &Value) {
    let Some(obj) = data.as_object() else {
        return;
    };
    if obj.is_empty() {
        vars.set(Scope::Session, "target_name".to_string(), String::new());
        vars.set(Scope::Session, "target_hp".to_string(), String::new());
        vars.set(
            Scope::Session,
            "target_condition".to_string(),
            String::new(),
        );
        return;
    }
    if let Some(name) = obj.get("target").and_then(Value::as_str) {
        vars.set(Scope::Session, "target_name".to_string(), name.to_string());
    }
    let hp = obj.get("hp_pct").map(stringify).unwrap_or_default();
    vars.set(Scope::Session, "target_hp".to_string(), hp);
    let condition = obj
        .get("condition")
        .and_then(Value::as_str)
        .unwrap_or_default();
    vars.set(
        Scope::Session,
        "target_condition".to_string(),
        condition.to_string(),
    );
}

/// The flag Aabahran adds to a package whose values the game hides.
const HIDDEN_FLAG: &str = "hidden";

fn stringify(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(package: &str, data: Value) -> Message {
        Message {
            package: package.to_string(),
            data,
        }
    }

    #[test]
    fn char_vitals_binds_top_level_keys() {
        let mut v = VariableStore::new();
        apply(
            &mut v,
            &msg("Char.Vitals", json!({"hp": 150, "maxhp": 200, "mp": 80})),
        );
        assert_eq!(v.get("hp"), Some("150"));
        assert_eq!(v.get("maxhp"), Some("200"));
        assert_eq!(v.get("mp"), Some("80"));
    }

    #[test]
    fn room_info_prefixes_with_room() {
        let mut v = VariableStore::new();
        apply(
            &mut v,
            &msg(
                "Room.Info",
                json!({"name": "The Square", "area": "Midgaard", "id": 1234}),
            ),
        );
        assert_eq!(v.get("room_name"), Some("The Square"));
        assert_eq!(v.get("room_area"), Some("Midgaard"));
        assert_eq!(v.get("room_id"), Some("1234"));
    }

    #[test]
    fn char_name_prefixes_with_char() {
        let mut v = VariableStore::new();
        apply(
            &mut v,
            &msg(
                "Char.Name",
                json!({"name": "Aleph", "fullname": "Aleph the Wanderer"}),
            ),
        );
        assert_eq!(v.get("char_name"), Some("Aleph"));
        assert_eq!(v.get("char_fullname"), Some("Aleph the Wanderer"));
    }

    #[test]
    fn unknown_package_ignored() {
        let mut v = VariableStore::new();
        apply(&mut v, &msg("Foo.Bar", json!({"a": 1})));
        assert_eq!(v.get("a"), None);
    }

    #[test]
    fn non_object_payload_ignored() {
        let mut v = VariableStore::new();
        apply(&mut v, &msg("Char.Vitals", json!([1, 2, 3])));
        assert_eq!(v.get("hp"), None);
    }

    #[test]
    fn nested_value_serializes_as_json() {
        let mut v = VariableStore::new();
        apply(
            &mut v,
            &msg("Room.Info", json!({"name": "X", "exits": {"n": 1, "e": 2}})),
        );
        assert_eq!(v.get("room_exits"), Some(r#"{"e":2,"n":1}"#));
    }

    #[test]
    fn char_combat_binds_target_name_hp_and_condition() {
        // Real Aabahran payload shape captured from a live session.
        let mut v = VariableStore::new();
        apply(
            &mut v,
            &msg(
                "Char.Combat",
                json!({
                    "condition": "a few scratches",
                    "hp_pct": 91,
                    "target": "The Baron Grisvald",
                }),
            ),
        );
        assert_eq!(v.get("target_name"), Some("The Baron Grisvald"));
        assert_eq!(v.get("target_hp"), Some("91"));
        assert_eq!(v.get("target_condition"), Some("a few scratches"));
    }

    #[test]
    fn char_combat_empty_payload_clears_target_fields() {
        let mut v = VariableStore::new();
        apply(
            &mut v,
            &msg(
                "Char.Combat",
                json!({"target": "kobold", "hp_pct": 5, "condition": "near death"}),
            ),
        );
        assert_eq!(v.get("target_name"), Some("kobold"));
        // Server pushes an empty object when the target is gone.
        apply(&mut v, &msg("Char.Combat", json!({})));
        assert_eq!(v.get("target_name"), Some(""));
        assert_eq!(v.get("target_hp"), Some(""));
        assert_eq!(v.get("target_condition"), Some(""));
    }

    /// An Aabahran packet from fixtures/gmcp/aabahran.
    fn packet(text: &str) -> Message {
        vosh_protocol::gmcp::parse(text.as_bytes()).expect("fixture parses")
    }

    const VITALS: &str = include_str!("../../fixtures/gmcp/aabahran/char-vitals.gmcp");
    const VITALS_HIDDEN: &str =
        include_str!("../../fixtures/gmcp/aabahran/char-vitals-hidden.gmcp");
    const COMBAT: &str = include_str!("../../fixtures/gmcp/aabahran/char-combat.gmcp");
    const COMBAT_HIDDEN: &str =
        include_str!("../../fixtures/gmcp/aabahran/char-combat-hidden.gmcp");
    const COMBAT_TANK: &str = include_str!("../../fixtures/gmcp/aabahran/char-combat-tank.gmcp");
    const COMBAT_TANK_HIDDEN: &str =
        include_str!("../../fixtures/gmcp/aabahran/char-combat-tank-hidden.gmcp");

    #[test]
    fn char_combat_withheld_health_clears_the_last_reading() {
        // Lamented tears, blindness, mirror image, or a target in
        // another room withhold the health. The last numbers must not
        // stand in for it.
        let mut v = VariableStore::new();
        apply(&mut v, &packet(COMBAT));
        assert_eq!(v.get("target_hp"), Some("54"));
        assert_eq!(v.get("target_condition"), Some("quite a few wounds"));
        apply(&mut v, &packet(COMBAT_HIDDEN));
        assert_eq!(v.get("target_name"), Some("a Blackwatch guard"));
        assert_eq!(v.get("target_hp"), Some(""));
        assert_eq!(v.get("target_condition"), Some(""));
        assert_eq!(v.get("target_hidden"), None);
    }

    #[test]
    fn char_combat_with_a_tank_binds_the_target_as_before() {
        let mut v = VariableStore::new();
        apply(&mut v, &packet(COMBAT_TANK));
        assert_eq!(v.get("target_name"), Some("a Blackwatch guard"));
        assert_eq!(v.get("target_hp"), Some("54"));
        apply(&mut v, &packet(COMBAT_TANK_HIDDEN));
        assert_eq!(v.get("target_name"), Some("a Blackwatch guard"));
        assert_eq!(v.get("target_hp"), Some(""));
    }

    #[test]
    fn char_vitals_hidden_flag_binds_no_variable() {
        // The flag rides only the hidden packets, so a bound variable
        // would read true long after the game shows your vitals again.
        let mut v = VariableStore::new();
        apply(&mut v, &packet(VITALS_HIDDEN));
        assert_eq!(v.get("hp"), Some("0"));
        assert_eq!(v.get("maxhp"), Some("0"));
        assert_eq!(v.get("hidden"), None);
        apply(&mut v, &packet(VITALS));
        assert_eq!(v.get("hp"), Some("850"));
        assert_eq!(v.get("hidden"), None);
    }
}
