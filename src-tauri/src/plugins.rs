//! Lua plugin manager. A plugin is a directory under
//! `<app_data_dir>/plugins/<slug>/` containing a `manifest.toml` and one or
//! more Lua files. The manifest declares which file is the entry point;
//! enabled plugins have their entry script loaded into the shared
//! `ScriptEngine` on launch.
//!
//! Hot enable runs the entry script through the engine immediately. Hot
//! reload re-runs it. Hot disable persists the choice but takes effect on
//! next launch (the engine doesn't track per-script trigger ownership yet,
//! so we can't safely yank a script's registrations mid-flight).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error)]
pub(crate) enum PluginError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest parse: {0}")]
    Manifest(#[from] toml::de::Error),
    #[error("plugin `{0}` not found")]
    NotFound(String),
    #[error("plugin `{0}` entry script `{1}` is missing")]
    EntryMissing(String, String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PluginManifestFile {
    pub plugin: PluginManifest,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Path to the Lua entry script, relative to the plugin directory.
    #[serde(default = "default_entry")]
    pub entry: String,
}

fn default_entry() -> String {
    "main.lua".to_string()
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PluginRecord {
    pub manifest: PluginManifest,
    /// Absolute path to the plugin's directory.
    pub dir: PathBuf,
    pub enabled: bool,
}

#[derive(Debug, Default)]
pub(crate) struct PluginManager {
    plugins_dir: Option<PathBuf>,
    plugins: Vec<PluginRecord>,
    enabled: BTreeSet<String>,
}

pub(crate) type SharedPluginManager = Arc<Mutex<PluginManager>>;

impl PluginManager {
    pub(crate) fn set_plugins_dir(&mut self, dir: PathBuf) {
        self.plugins_dir = Some(dir);
    }

    /// Replace the persisted enabled-set. Call once at startup with the
    /// list loaded from profile.toml.
    pub(crate) fn set_enabled(&mut self, enabled: impl IntoIterator<Item = String>) {
        self.enabled = enabled.into_iter().collect();
        for record in &mut self.plugins {
            record.enabled = self.enabled.contains(&record.manifest.name);
        }
    }

    pub(crate) fn enabled_names(&self) -> Vec<String> {
        self.enabled.iter().cloned().collect()
    }

    /// Re-scan the plugins directory and update the in-memory list.
    pub(crate) fn discover(&mut self) -> Result<(), PluginError> {
        let Some(dir) = &self.plugins_dir else {
            self.plugins.clear();
            return Ok(());
        };
        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
        }
        let mut found = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("manifest.toml");
            if !manifest_path.exists() {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
                continue;
            };
            let Ok(parsed) = toml::from_str::<PluginManifestFile>(&raw) else {
                continue;
            };
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if dir_name != parsed.plugin.name {
                continue;
            }
            let enabled = self.enabled.contains(&parsed.plugin.name);
            found.push(PluginRecord {
                manifest: parsed.plugin,
                dir: path,
                enabled,
            });
        }
        found.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        self.plugins = found;
        Ok(())
    }

    pub(crate) fn list(&self) -> &[PluginRecord] {
        &self.plugins
    }

    pub(crate) fn get(&self, name: &str) -> Option<&PluginRecord> {
        self.plugins.iter().find(|p| p.manifest.name == name)
    }

    /// Read the entry script body for `name`. Errors when the plugin or
    /// its entry file are missing.
    pub(crate) fn read_entry(&self, name: &str) -> Result<String, PluginError> {
        let record = self
            .get(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
        let entry_path = record.dir.join(&record.manifest.entry);
        if !entry_path.exists() {
            return Err(PluginError::EntryMissing(
                name.to_string(),
                record.manifest.entry.clone(),
            ));
        }
        Ok(std::fs::read_to_string(entry_path)?)
    }

    /// Mark a plugin enabled in the in-memory state. Persistence is the
    /// caller's responsibility (profile.toml). Returns false when the
    /// plugin isn't known.
    pub(crate) fn mark_enabled(&mut self, name: &str, enabled: bool) -> bool {
        let exists = self.plugins.iter().any(|p| p.manifest.name == name);
        if !exists {
            return false;
        }
        if enabled {
            self.enabled.insert(name.to_string());
        } else {
            self.enabled.remove(name);
        }
        for record in &mut self.plugins {
            if record.manifest.name == name {
                record.enabled = enabled;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    fn write_plugin(root: &Path, slug: &str, manifest_name: &str, entry_body: &str) {
        let dir = root.join(slug);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = format!(
            "[plugin]\nname = \"{manifest_name}\"\nversion = \"0.1.0\"\ndescription = \"x\"\nauthor = \"y\"\nentry = \"main.lua\"\n"
        );
        std::fs::write(dir.join("manifest.toml"), manifest).unwrap();
        let mut f = std::fs::File::create(dir.join("main.lua")).unwrap();
        f.write_all(entry_body.as_bytes()).unwrap();
    }

    #[test]
    fn discover_picks_up_well_formed_plugin() {
        let tmp = tempdir();
        write_plugin(tmp.path(), "alpha", "alpha", "-- hello\n");
        let mut mgr = PluginManager::default();
        mgr.set_plugins_dir(tmp.path().to_path_buf());
        mgr.discover().unwrap();
        assert_eq!(mgr.list().len(), 1);
        assert_eq!(mgr.list()[0].manifest.name, "alpha");
    }

    #[test]
    fn discover_skips_when_dir_name_does_not_match_manifest() {
        let tmp = tempdir();
        write_plugin(tmp.path(), "wrong_dir", "actual_name", "");
        let mut mgr = PluginManager::default();
        mgr.set_plugins_dir(tmp.path().to_path_buf());
        mgr.discover().unwrap();
        assert!(mgr.list().is_empty());
    }

    #[test]
    fn enabled_state_round_trips_via_set_enabled() {
        let tmp = tempdir();
        write_plugin(tmp.path(), "a", "a", "");
        write_plugin(tmp.path(), "b", "b", "");
        let mut mgr = PluginManager::default();
        mgr.set_plugins_dir(tmp.path().to_path_buf());
        mgr.discover().unwrap();
        mgr.set_enabled(["b".to_string()]);
        let listed: Vec<(String, bool)> = mgr
            .list()
            .iter()
            .map(|p| (p.manifest.name.clone(), p.enabled))
            .collect();
        assert_eq!(
            listed,
            vec![("a".to_string(), false), ("b".to_string(), true)]
        );
        assert_eq!(mgr.enabled_names(), vec!["b".to_string()]);
    }

    #[test]
    fn mark_enabled_updates_record_and_set() {
        let tmp = tempdir();
        write_plugin(tmp.path(), "x", "x", "");
        let mut mgr = PluginManager::default();
        mgr.set_plugins_dir(tmp.path().to_path_buf());
        mgr.discover().unwrap();
        assert!(mgr.mark_enabled("x", true));
        assert!(mgr.list()[0].enabled);
        assert_eq!(mgr.enabled_names(), vec!["x".to_string()]);
        assert!(!mgr.mark_enabled("missing", true));
    }

    #[test]
    fn read_entry_returns_lua_body() {
        let tmp = tempdir();
        write_plugin(tmp.path(), "p", "p", "print('hi')");
        let mut mgr = PluginManager::default();
        mgr.set_plugins_dir(tmp.path().to_path_buf());
        mgr.discover().unwrap();
        let body = mgr.read_entry("p").unwrap();
        assert!(body.contains("print('hi')"));
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }
}
