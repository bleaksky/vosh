//! One version across the app. Cargo.toml holds the workspace version,
//! and package.json, package-lock.json and tauri.conf.json each carry a
//! copy. A release that bumps one and misses another ships a build that
//! names two versions, so this test holds every copy to Cargo.toml.

use std::fs;
use std::path::Path;

use serde_json::Value;

/// Reads a JSON file relative to `src-tauri`.
fn json(rel: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel} does not open, {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{rel} is not valid JSON, {e}"))
}

/// Fails when the version at `pointer` in `rel` differs from Cargo.toml.
fn agrees(rel: &str, pointer: &str) {
    let cargo = env!("CARGO_PKG_VERSION");
    let name = rel.trim_start_matches("../");
    let doc = json(rel);
    let found = doc
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{name} has no version at {pointer}"));
    assert_eq!(
        found, cargo,
        "{name} says version {found} but Cargo.toml says {cargo}. Set them to the same version."
    );
}

#[test]
fn package_json_matches_cargo_version() {
    agrees("../package.json", "/version");
}

#[test]
fn package_lock_matches_cargo_version() {
    agrees("../package-lock.json", "/version");
    agrees("../package-lock.json", "/packages//version");
}

#[test]
fn tauri_conf_matches_cargo_version() {
    agrees("tauri.conf.json", "/version");
}
