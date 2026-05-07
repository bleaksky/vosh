//! Auto-bind common GMCP packages onto session-scoped variables so users
//! can reference them in aliases, triggers, and (later) scripts. Phase 4
//! covers `Char.Vitals`, `Char.Status`, `Char.Name`, and `Room.Info`. More
//! packages land alongside the script engine in Phase 8.

use mudclient_gmcp::Message;
use mudclient_vars::{Scope, VariableStore};
use serde_json::Value;

/// Push fields from a known GMCP package into the session variable store.
/// Unknown packages are ignored so the auto-bind stays opt-in.
pub(crate) fn apply(vars: &mut VariableStore, msg: &Message) {
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
        let var_name = format!("{prefix}{key}");
        let value_str = stringify(value);
        vars.set(Scope::Session, var_name, value_str);
    }
}

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
}
