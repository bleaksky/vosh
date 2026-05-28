//! GMCP message parsing and serialization.
//!
//! GMCP wraps `package.name JSON` payloads inside telnet subnegotiation
//! blocks. The telnet parser delivers the payload bytes; this crate splits
//! the package name from the JSON value and parses both.
//!
//! Outgoing messages serialize back into the same wire format. The byte
//! framing (IAC SB GMCP ... IAC SE) lives in the telnet crate next to the
//! other subnegotiation builders.

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

/// One decoded GMCP message: a package name like `Char.Vitals` plus the
/// JSON payload that follows. A package without a payload yields
/// `Value::Null`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub package: String,
    pub data: Value,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("payload is not utf-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("package name is empty")]
    EmptyPackage,
    #[error("invalid json payload: {0}")]
    Json(#[from] serde_json::Error),
}

/// Parse a subnegotiation payload into a [`Message`]. The bytes should be
/// the contents of `IAC SB GMCP <bytes> IAC SE` after the telnet parser has
/// stripped the framing and IAC IAC escaping.
pub fn parse(payload: &[u8]) -> Result<Message, ParseError> {
    let s = std::str::from_utf8(payload)?;
    let trimmed = s.trim();
    let (package, json_part) = match trimmed.find(char::is_whitespace) {
        Some(idx) => trimmed.split_at(idx),
        None => (trimmed, ""),
    };
    if package.is_empty() {
        return Err(ParseError::EmptyPackage);
    }
    let data = if json_part.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(json_part.trim())?
    };
    Ok(Message {
        package: package.to_string(),
        data,
    })
}

/// Serialize a package name plus a JSON-serializable value into the GMCP
/// wire payload (without telnet framing). The session layer wraps the
/// returned bytes in IAC SB GMCP ... IAC SE before sending.
pub fn build<T: Serialize>(package: &str, value: &T) -> serde_json::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(package.len() + 64);
    out.extend_from_slice(package.as_bytes());
    let json = serde_json::to_string(value)?;
    if !json.is_empty() && json != "null" {
        out.push(b' ');
        out.extend_from_slice(json.as_bytes());
    }
    Ok(out)
}

/// Build a payload from an already-stringified JSON body. Useful when the
/// caller has the JSON ready as a `&str` and wants to skip another parse.
pub fn build_raw(package: &str, json_body: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(package.len() + json_body.len() + 1);
    out.extend_from_slice(package.as_bytes());
    let trimmed = json_body.trim();
    if !trimmed.is_empty() {
        out.push(b' ');
        out.extend_from_slice(trimmed.as_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_package_with_object() {
        let msg = parse(br#"Char.Vitals {"hp":150,"mp":80}"#).unwrap();
        assert_eq!(msg.package, "Char.Vitals");
        assert_eq!(msg.data, json!({ "hp": 150, "mp": 80 }));
    }

    #[test]
    fn parses_package_with_array() {
        let msg = parse(br#"Comm.Channels.List ["chat","tells"]"#).unwrap();
        assert_eq!(msg.package, "Comm.Channels.List");
        assert_eq!(msg.data, json!(["chat", "tells"]));
    }

    #[test]
    fn parses_package_without_payload() {
        let msg = parse(b"Core.Ping").unwrap();
        assert_eq!(msg.package, "Core.Ping");
        assert_eq!(msg.data, Value::Null);
    }

    #[test]
    fn parses_package_with_string() {
        let msg = parse(br#"Char.Name "Aleph""#).unwrap();
        assert_eq!(msg.package, "Char.Name");
        assert_eq!(msg.data, json!("Aleph"));
    }

    #[test]
    fn rejects_empty_payload() {
        assert!(matches!(parse(b""), Err(ParseError::EmptyPackage)));
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(matches!(parse(b"Foo {bad"), Err(ParseError::Json(_))));
    }

    #[test]
    fn rejects_non_utf8_payload() {
        assert!(matches!(parse(&[0x80, 0xff]), Err(ParseError::Utf8(_))));
    }

    #[test]
    fn build_with_value() {
        let bytes = build("Core.Hello", &json!({"client": "vosh", "version": "0.0.1"})).unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("Core.Hello "));
        let json_part = s.strip_prefix("Core.Hello ").unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(json_part).unwrap(),
            json!({ "client": "vosh", "version": "0.0.1" })
        );
    }

    #[test]
    fn build_with_array() {
        let bytes = build("Core.Supports.Set", &vec!["Char 1", "Room 1"]).unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert_eq!(s, r#"Core.Supports.Set ["Char 1","Room 1"]"#);
    }

    #[test]
    fn build_no_payload_for_null() {
        let bytes = build("Core.Ping", &Value::Null).unwrap();
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "Core.Ping");
    }

    #[test]
    fn build_raw_with_body() {
        let bytes = build_raw("Char.Status", r#"{"hp":100}"#);
        assert_eq!(
            std::str::from_utf8(&bytes).unwrap(),
            r#"Char.Status {"hp":100}"#
        );
    }

    #[test]
    fn build_raw_skips_empty_body() {
        let bytes = build_raw("Core.Ping", "");
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "Core.Ping");
    }

    #[test]
    fn round_trip() {
        let bytes = build("Char.Vitals", &json!({"hp": 100, "mp": 50})).unwrap();
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.package, "Char.Vitals");
        assert_eq!(msg.data, json!({ "hp": 100, "mp": 50 }));
    }
}
