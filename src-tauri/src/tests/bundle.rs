//! What the bundle ships. The fonts in `public/fonts` reach every build,
//! so their license has to sit beside them and in the package, and no
//! font we hold no license for may ride along. D10 removed Berkeley Mono,
//! and these tests keep it out. The store text has to describe the app
//! as it ships, too.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The `src-tauri` folder.
fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Reads tauri.conf.json as text.
fn conf_text() -> String {
    let path = manifest_dir().join("tauri.conf.json");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("tauri.conf.json does not open, {e}"))
}

/// Reads tauri.conf.json as JSON.
fn conf() -> Value {
    serde_json::from_str(&conf_text())
        .unwrap_or_else(|e| panic!("tauri.conf.json is not valid JSON, {e}"))
}

/// Every file under `dir`, at any depth.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("{} does not open, {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a folder entry reads").path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn font_license_ships_in_the_bundle() {
    let doc = conf();
    let resources = doc
        .pointer("/bundle/resources")
        .and_then(Value::as_object)
        .expect("tauri.conf.json has no bundle.resources map");
    let source = "../public/fonts/OFL.txt";
    assert_eq!(
        resources.get(source).and_then(Value::as_str),
        Some("licenses/JetBrainsMono-OFL.txt"),
        "bundle.resources should copy {source} to licenses/JetBrainsMono-OFL.txt"
    );
    let bundled = fs::read(manifest_dir().join(source))
        .unwrap_or_else(|e| panic!("{source} does not open, {e}"));
    let beside = fs::read(manifest_dir().join("icons/source/OFL-JetBrainsMono.txt"))
        .expect("icons/source/OFL-JetBrainsMono.txt opens");
    assert_eq!(
        bundled, beside,
        "public/fonts/OFL.txt differs from icons/source/OFL-JetBrainsMono.txt. Copy one over the other."
    );
}

#[test]
fn no_berkeley_mono_ships() {
    let public = manifest_dir().join("../public");
    for path in files_under(&public) {
        let name = path
            .strip_prefix(&public)
            .expect("under public")
            .to_string_lossy();
        assert!(
            !name.to_lowercase().contains("berkeley"),
            "public/{name} would ship Berkeley Mono, which Vosh holds no license for. Remove it."
        );
    }
    assert!(
        !conf_text().to_lowercase().contains("berkeley"),
        "tauri.conf.json names Berkeley Mono, which Vosh holds no license for. Remove it."
    );
}

#[test]
fn public_fonts_hold_jetbrains_mono_and_its_license() {
    let dir = manifest_dir().join("../public/fonts");
    let mut names: Vec<String> = files_under(&dir)
        .iter()
        .map(|p| {
            p.strip_prefix(&dir)
                .expect("under public/fonts")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["JetBrainsMonoNerdFont-Bold.ttf", "JetBrainsMonoNerdFont-Regular.ttf", "OFL.txt"],
        "public/fonts should hold only the two JetBrains Mono files and their license. Each font needs a license before it ships."
    );
}

#[test]
fn long_description_matches_the_app() {
    let doc = conf();
    let text = doc
        .pointer("/bundle/longDescription")
        .and_then(Value::as_str)
        .expect("tauri.conf.json has no bundle.longDescription");
    assert!(
        !text.to_lowercase().contains("map window"),
        "bundle.longDescription promises a map window, but the map is a pane in the main window. Describe it as a map pane."
    );
}
