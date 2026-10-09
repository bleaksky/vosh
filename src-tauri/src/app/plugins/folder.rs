//! A plugin's folder as the Scripts page in Settings makes, reads,
//! saves and removes it. Every function here checks the name it gets
//! against the rule New plugin shows, letters, digits and underscores,
//! so a name from the page can only ever name one folder inside the
//! plugins folder.
//! The loader in [`super`] reads a folder you made by hand with a looser
//! rule, so it still loads at launch. The Scripts page lists it as one
//! it cannot open, and can turn it off.
//!
//! Each file goes in whole through [`swap_in`], with no backup beside it,
//! since a backup in the folder would ride along when you export the
//! plugin.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tracing::warn;

use super::{entry_stays_inside, read_plugin, PluginManifest, PluginManifestFile};
use crate::disk::atomic::swap_in;

/// What a command says when a plugin name breaks the rule.
pub(crate) const NAME_RULE: &str = "Use only letters, digits, and underscores in a plugin name.";

/// The version a new plugin starts at.
const FIRST_VERSION: &str = "0.1.0";

/// The file a new plugin runs first.
const FIRST_ENTRY: &str = "main.lua";

/// True when `name` is one or more ASCII letters, digits and
/// underscores, the rule every Scripts command holds a plugin name to.
pub(crate) fn plugin_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// The folder of the plugin `name` in `plugins_dir`, once the name keeps
/// the rule and a folder of that name, in that case, is there. A disk
/// that ignores case would find `wait_full` under `Wait_Full`, and a save
/// would then write a manifest that names it unlike its folder.
pub(crate) fn existing(plugins_dir: &Path, name: &str) -> Result<PathBuf, String> {
    if !plugin_name_ok(name) {
        return Err(NAME_RULE.to_string());
    }
    let found = std::fs::read_dir(plugins_dir).is_ok_and(|mut entries| {
        entries.any(|entry| entry.is_ok_and(|entry| entry.file_name() == name))
    });
    let dir = plugins_dir.join(name);
    if found && dir.is_dir() {
        Ok(dir)
    } else {
        Err(format!("You have no plugin named {name}."))
    }
}

/// Make the folder of a new plugin `name` in `plugins_dir`, with the two
/// files New plugin writes: a manifest that names it and runs
/// `main.lua`, and a `main.lua` that says when it runs. A name you
/// already use, in any case, is refused, since the disks of macOS and
/// Windows ignore case.
pub(crate) fn create(plugins_dir: &Path, name: &str) -> Result<(), String> {
    if !plugin_name_ok(name) {
        return Err(NAME_RULE.to_string());
    }
    let could_not = |e: std::io::Error| {
        warn!(plugin = %name, error = %e, "could not make a plugin folder");
        format!("Vosh could not make the folder for {name}.")
    };
    std::fs::create_dir_all(plugins_dir).map_err(could_not)?;
    if let Some(taken) = in_any_case(plugins_dir, name).map_err(could_not)? {
        return Err(already_have(&taken));
    }
    let dir = plugins_dir.join(name);
    // Not `create_dir_all`, so a folder made since the look above stays
    // as it is.
    std::fs::create_dir(&dir).map_err(could_not)?;
    let manifest = PluginManifest {
        name: name.to_string(),
        version: FIRST_VERSION.to_string(),
        description: String::new(),
        author: String::new(),
        entry: FIRST_ENTRY.to_string(),
    };
    let code = format!("-- {name}\n-- Vosh runs this file when {name} is on.\n");
    let written =
        write_manifest(&dir, manifest).and_then(|()| swap_in(&dir.join(FIRST_ENTRY), &code));
    if let Err(e) = written {
        // A folder with half its files would block the name.
        let _ = std::fs::remove_dir_all(&dir);
        return Err(could_not(e));
    }
    Ok(())
}

/// The name of the folder in `plugins_dir` that has `name` in any case,
/// if one does, which on the disks of macOS and Windows is the folder a
/// write to `name` reaches. One in the same case comes first.
fn in_any_case(plugins_dir: &Path, name: &str) -> std::io::Result<Option<String>> {
    let mut found = None;
    for entry in std::fs::read_dir(plugins_dir)? {
        let taken = entry?.file_name().to_string_lossy().into_owned();
        if taken == name {
            return Ok(Some(taken));
        }
        if found.is_none() && taken.eq_ignore_ascii_case(name) {
            found = Some(taken);
        }
    }
    Ok(found)
}

fn already_have(taken: &str) -> String {
    format!("You already have a plugin named {taken}.")
}

/// The version of the plugin `name` in `plugins_dir` that an install
/// would replace, or None when you have no plugin of that name. One you
/// have in another case is refused as New plugin refuses it, since the
/// install would replace a plugin of another name there.
pub(crate) fn installed(plugins_dir: &Path, name: &str) -> Result<Option<String>, String> {
    if !plugin_name_ok(name) {
        return Err(NAME_RULE.to_string());
    }
    let taken = match in_any_case(plugins_dir, name) {
        Ok(taken) => taken,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            warn!(error = %e, "could not read the plugins folder");
            return Err("Vosh could not read your plugins folder.".to_string());
        }
    };
    match taken {
        None => Ok(None),
        Some(taken) if taken != name => Err(already_have(&taken)),
        // A manifest Vosh cannot read names no version.
        Some(_) => Ok(Some(
            std::fs::read_to_string(plugins_dir.join(name).join("manifest.toml"))
                .ok()
                .and_then(|raw| toml::from_str::<PluginManifestFile>(&raw).ok())
                .map(|file| file.plugin.version)
                .unwrap_or_default(),
        )),
    }
}

/// Delete the folder of the plugin `name` in `plugins_dir`, for Remove.
/// A link in its place goes, and never what it leads to.
pub(crate) fn remove(plugins_dir: &Path, name: &str) -> Result<(), String> {
    let dir = existing(plugins_dir, name)?;
    std::fs::remove_dir_all(&dir).map_err(|e| {
        warn!(plugin = %name, error = %e, "could not remove a plugin");
        format!("Vosh could not remove {name}.")
    })
}

/// A plugin's folder as the Scripts page shows it.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PluginFolder {
    /// What manifest.toml says.
    pub(crate) manifest: PluginManifest,
    /// The code of the file it runs first.
    pub(crate) code: String,
    /// Every Lua file in the folder, by its path inside the folder with
    /// `/` between parts, sorted, which Runs first offers.
    pub(crate) files: Vec<String>,
    /// The folder as the Manifest tab names it, like
    /// `plugins/vitals_alert`.
    pub(crate) folder: String,
}

/// Read the plugin `name` in `plugins_dir` as it stands, with the checks
/// a load makes: the manifest names the folder, and the file it runs
/// first sits inside it. With `file`, one of its Lua files, the code is
/// that file's, for a new pick under Runs first, which the editor then
/// shows. Save writes the code to the file Runs first names, so without
/// it the code of the old file would land in the new one.
pub(crate) fn read(
    plugins_dir: &Path,
    name: &str,
    file: Option<&str>,
) -> Result<PluginFolder, String> {
    let dir = existing(plugins_dir, name)?;
    let could_not = |e: &dyn std::fmt::Display| {
        warn!(plugin = %name, error = %e, "could not read a plugin");
        format!("Vosh could not read plugin {name}.")
    };
    let plugin = read_plugin(plugins_dir, name).map_err(|e| could_not(&e))?;
    let mut files = regular_files(&dir).map_err(|e| could_not(&e))?;
    files.retain(|file| {
        Path::new(file)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    });
    let code = match file {
        None => plugin.code,
        // Only a regular Lua file the walk found, so no link and no
        // path leads out of the folder.
        Some(file) if files.iter().any(|found| found == file) => {
            std::fs::read_to_string(dir.join(file)).map_err(|e| could_not(&e))?
        }
        Some(_) => {
            return Err(format!(
                "Pick a file inside the {name} folder for Runs first."
            ))
        }
    };
    Ok(PluginFolder {
        manifest: plugin.manifest,
        code,
        files,
        folder: format!("plugins/{name}"),
    })
}

/// Every regular file under `dir`, by its path inside it with `/`
/// between parts, sorted, for Runs first and for Export. A link is no
/// regular file and is never followed, so a link never offers or carries
/// a file from elsewhere. A name that is not UTF-8 is left out.
pub(crate) fn regular_files(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();
    walk(dir, "", &mut files)?;
    files.sort();
    Ok(files)
}

/// Add each regular file under `dir` to `files`, by its path inside the
/// folder [`regular_files`] walks, which `prefix` begins.
fn walk(dir: &Path, prefix: &str, files: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let Some(file) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let path = format!("{prefix}{file}");
        if kind.is_dir() {
            walk(&entry.path(), &format!("{path}/"), files)?;
        } else if kind.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

/// Write `code` and `manifest` for the plugin `name` in `plugins_dir`:
/// the file it runs first, then manifest.toml, with the name the folder
/// has whatever `manifest` says. That file must sit inside the folder, as
/// a load asks, and a link on the way to it must lead there too. The code
/// goes first, so a write that fails there leaves the manifest naming
/// the file it named.
pub(crate) fn save(
    plugins_dir: &Path,
    name: &str,
    mut manifest: PluginManifest,
    code: &str,
) -> Result<(), String> {
    let dir = existing(plugins_dir, name)?;
    name.clone_into(&mut manifest.name);
    let outside = || format!("Pick a file inside the {name} folder for Runs first.");
    if !entry_stays_inside(&manifest.entry) {
        return Err(outside());
    }
    let could_not = |e: std::io::Error| {
        warn!(plugin = %name, error = %e, "could not save a plugin");
        format!("Vosh could not save {name}.")
    };
    let entry = dir.join(&manifest.entry);
    let real_dir = dir.canonicalize().map_err(could_not)?;
    // The rename that saves the file replaces a link and never writes
    // through it, so only the folders on the way can lead out, and a
    // link the file is now must lead inside, as a load asks.
    let leads_inside = |path: &Path| {
        path.canonicalize()
            .is_ok_and(|real| real.starts_with(&real_dir))
    };
    let parent = entry.parent().unwrap_or(&dir);
    let exists = entry.symlink_metadata().is_ok();
    if !leads_inside(parent) || (exists && !leads_inside(&entry)) {
        return Err(outside());
    }
    swap_in(&entry, code).map_err(could_not)?;
    write_manifest(&dir, manifest).map_err(could_not)
}

/// Write `manifest` to manifest.toml in `dir`.
fn write_manifest(dir: &Path, manifest: PluginManifest) -> std::io::Result<()> {
    let text =
        toml::to_string(&PluginManifestFile { plugin: manifest }).map_err(std::io::Error::other)?;
    swap_in(&dir.join("manifest.toml"), &text)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_page_holds_plugin_names_to_the_same_rule() {
        // New plugin checks a name as you type it with pluginNameOk in
        // src/settings/scripts/pluginName.ts.
        let page = include_str!("../../../../src/settings/scripts/pluginName.ts");
        let rule = regex::Regex::new(r"return /(\^[^/]*\$)/\.test\(name\);")
            .unwrap()
            .captures(page)
            .expect("pluginName.ts tests a name against one pattern");
        let page_rule = regex::Regex::new(&rule[1]).unwrap();
        for name in [
            "wait_full",
            "Wait_Full9",
            "_",
            "",
            "weather-pane",
            "old pane",
            "../x",
            "a/b",
            "caf\u{e9}",
            "x.lua",
        ] {
            assert_eq!(plugin_name_ok(name), page_rule.is_match(name), "{name:?}");
        }
    }

    use super::*;

    /// The two files New plugin writes for `wait_full`.
    const WAIT_FULL_MANIFEST: &str = "[plugin]
name = \"wait_full\"
version = \"0.1.0\"
description = \"\"
author = \"\"
entry = \"main.lua\"
";
    const WAIT_FULL_MAIN: &str = "-- wait_full
-- Vosh runs this file when wait_full is on.
";

    fn plugins() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugins = tmp.path().join("plugins");
        (tmp, plugins)
    }

    /// The names in the folder `dir`, sorted.
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn new_plugin_writes_the_two_files_of_board_two() {
        let (_tmp, plugins) = plugins();
        create(&plugins, "wait_full").unwrap();
        let dir = plugins.join("wait_full");
        assert_eq!(names_in(&dir), ["main.lua", "manifest.toml"]);
        let read = |file: &str| std::fs::read_to_string(dir.join(file)).unwrap();
        assert_eq!(read("manifest.toml"), WAIT_FULL_MANIFEST);
        assert_eq!(read("main.lua"), WAIT_FULL_MAIN);
        // A load reads it as it reads any plugin.
        assert_eq!(
            read_plugin(&plugins, "wait_full").unwrap().code,
            WAIT_FULL_MAIN
        );
    }

    #[test]
    fn a_name_that_breaks_the_rule_names_no_folder() {
        let (tmp, plugins) = plugins();
        create(&plugins, "wait_full").unwrap();
        let absolute = tmp.path().join("x");
        let absolute = absolute.to_str().unwrap();
        for name in [
            "../x",
            absolute,
            "wait-full",
            "wait full",
            "",
            "wait_full/.",
        ] {
            assert_eq!(create(&plugins, name), Err(NAME_RULE.to_string()), "{name}");
            assert_eq!(
                read(&plugins, name, None).map(|_| ()),
                Err(NAME_RULE.to_string()),
                "{name}"
            );
            let saved = save(&plugins, name, manifest("main.lua"), "");
            assert_eq!(saved, Err(NAME_RULE.to_string()), "{name}");
            assert_eq!(existing(&plugins, name), Err(NAME_RULE.to_string()));
        }
        assert_eq!(names_in(tmp.path()), ["plugins"]);
        assert_eq!(names_in(&plugins), ["wait_full"]);
    }

    #[test]
    fn a_name_you_have_in_another_case_is_taken() {
        let (_tmp, plugins) = plugins();
        create(&plugins, "wait_full").unwrap();
        for name in ["Wait_Full", "wait_full"] {
            assert_eq!(
                create(&plugins, name),
                Err("You already have a plugin named wait_full.".to_string())
            );
        }
        assert_eq!(names_in(&plugins), ["wait_full"]);
        // Nor does the other case find it, on a disk that ignores case or
        // not, so no save writes a manifest unlike its folder.
        let none = Err("You have no plugin named Wait_Full.".to_string());
        assert_eq!(read(&plugins, "Wait_Full", None).map(|_| ()), none);
        assert_eq!(save(&plugins, "Wait_Full", manifest("main.lua"), ""), none);
        assert_eq!(
            std::fs::read_to_string(plugins.join("wait_full").join("manifest.toml")).unwrap(),
            WAIT_FULL_MANIFEST
        );
    }

    /// A manifest for `wait_full` that runs `entry`, with a name that is
    /// not the folder's.
    fn manifest(entry: &str) -> PluginManifest {
        PluginManifest {
            name: "other".into(),
            version: "0.2.0".into(),
            description: "Stand up once your hit points are full.".into(),
            author: "Orla".into(),
            entry: entry.into(),
        }
    }

    #[test]
    fn save_writes_both_files_whole_and_keeps_the_folder_name() {
        let (_tmp, plugins) = plugins();
        create(&plugins, "wait_full").unwrap();
        save(
            &plugins,
            "wait_full",
            manifest("main.lua"),
            "mud.send('stand')\n",
        )
        .unwrap();
        save(
            &plugins,
            "wait_full",
            manifest("main.lua"),
            "mud.send('rest')\n",
        )
        .unwrap();
        let folder = read(&plugins, "wait_full", None).unwrap();
        assert_eq!(folder.manifest.name, "wait_full");
        assert_eq!(folder.manifest.version, "0.2.0");
        assert_eq!(folder.manifest.author, "Orla");
        assert_eq!(folder.code, "mud.send('rest')\n");
        assert_eq!(folder.folder, "plugins/wait_full");
        // No backup and no temp file rides along in the folder.
        assert_eq!(
            names_in(&plugins.join("wait_full")),
            ["main.lua", "manifest.toml"]
        );
    }

    #[test]
    fn read_lists_every_lua_file_by_its_path_inside() {
        let (tmp, plugins) = plugins();
        create(&plugins, "weather_pane").unwrap();
        let dir = plugins.join("weather_pane");
        std::fs::create_dir(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib").join("draw.lua"), "").unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        std::fs::write(tmp.path().join("outside.lua"), "").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.path().join("outside.lua"), dir.join("link.lua")).unwrap();
        let folder = read(&plugins, "weather_pane", None).unwrap();
        assert_eq!(folder.files, ["lib/draw.lua", "main.lua"]);
        // Runs first may name the file in the folder below.
        save(
            &plugins,
            "weather_pane",
            manifest("lib/draw.lua"),
            "-- draw\n",
        )
        .unwrap();
        assert_eq!(
            read(&plugins, "weather_pane", None).unwrap().code,
            "-- draw\n"
        );
    }

    #[test]
    fn read_hands_back_the_code_of_a_file_runs_first_may_pick() {
        let (tmp, plugins) = plugins();
        create(&plugins, "weather_pane").unwrap();
        let dir = plugins.join("weather_pane");
        std::fs::create_dir(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib").join("draw.lua"), "-- draw\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        std::fs::write(tmp.path().join("outside.lua"), "-- yours\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.path().join("outside.lua"), dir.join("link.lua")).unwrap();
        let folder = read(&plugins, "weather_pane", Some("lib/draw.lua")).unwrap();
        assert_eq!(folder.code, "-- draw\n");
        // The manifest still names the file the plugin runs first now.
        assert_eq!(folder.manifest.entry, "main.lua");
        let outside = Err("Pick a file inside the weather_pane folder for Runs first.".to_string());
        let absolute = tmp.path().join("outside.lua");
        for file in [
            "../outside.lua",
            absolute.to_str().unwrap(),
            "notes.txt",
            "link.lua",
            "lib",
            "",
        ] {
            assert_eq!(
                read(&plugins, "weather_pane", Some(file)).map(|f| f.code),
                outside,
                "{file}"
            );
        }
    }

    #[test]
    fn save_keeps_the_file_it_runs_first_inside_the_folder() {
        let (tmp, plugins) = plugins();
        create(&plugins, "wait_full").unwrap();
        let dir = plugins.join("wait_full");
        let outside = "Pick a file inside the wait_full folder for Runs first.".to_string();
        let absolute = tmp.path().join("outside.lua");
        for entry in [
            "../outside.lua",
            "../../outside.lua",
            absolute.to_str().unwrap(),
            ".",
            "",
        ] {
            assert_eq!(
                save(&plugins, "wait_full", manifest(entry), "mud.send('look')"),
                Err(outside.clone()),
                "{entry}"
            );
        }
        // Nor through a link, to a file or a folder outside.
        #[cfg(unix)]
        {
            std::fs::write(tmp.path().join("outside.lua"), "-- yours\n").unwrap();
            std::os::unix::fs::symlink(tmp.path().join("outside.lua"), dir.join("link.lua"))
                .unwrap();
            std::os::unix::fs::symlink(tmp.path(), dir.join("away")).unwrap();
            for entry in ["link.lua", "away/outside.lua"] {
                assert_eq!(
                    save(&plugins, "wait_full", manifest(entry), "mud.send('look')"),
                    Err(outside.clone()),
                    "{entry}"
                );
            }
            assert_eq!(
                std::fs::read_to_string(tmp.path().join("outside.lua")).unwrap(),
                "-- yours\n"
            );
        }
        // A refused save writes neither file.
        assert_eq!(
            std::fs::read_to_string(dir.join("manifest.toml")).unwrap(),
            WAIT_FULL_MANIFEST
        );
        assert_eq!(names_in(&tmp.path().join("plugins")), ["wait_full"]);
    }

    #[test]
    fn installed_names_the_version_an_install_replaces() {
        let (_tmp, plugins) = plugins();
        // No plugins folder yet, so nothing to replace.
        assert_eq!(installed(&plugins, "wait_full"), Ok(None));
        create(&plugins, "wait_full").unwrap();
        assert_eq!(installed(&plugins, "wait_full"), Ok(Some("0.1.0".into())));
        assert_eq!(installed(&plugins, "weather_pane"), Ok(None));
        // One you have in another case is taken, as for New plugin.
        assert_eq!(
            installed(&plugins, "Wait_Full"),
            Err("You already have a plugin named wait_full.".to_string())
        );
        assert_eq!(installed(&plugins, "../x"), Err(NAME_RULE.to_string()));
        // A manifest Vosh cannot read names no version.
        std::fs::write(plugins.join("wait_full").join("manifest.toml"), "[plugin").unwrap();
        assert_eq!(installed(&plugins, "wait_full"), Ok(Some(String::new())));
    }

    #[test]
    fn remove_deletes_the_folder() {
        let (_tmp, plugins) = plugins();
        create(&plugins, "wait_full").unwrap();
        create(&plugins, "weather_pane").unwrap();
        remove(&plugins, "wait_full").unwrap();
        assert_eq!(names_in(&plugins), ["weather_pane"]);
        assert_eq!(
            remove(&plugins, "wait_full"),
            Err("You have no plugin named wait_full.".to_string())
        );
        assert_eq!(remove(&plugins, "../plugins"), Err(NAME_RULE.to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn remove_takes_a_link_and_leaves_what_it_leads_to() {
        let (tmp, plugins) = plugins();
        create(&plugins, "weather_pane").unwrap();
        let elsewhere = tmp.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("main.lua"), "-- yours\n").unwrap();
        std::os::unix::fs::symlink(&elsewhere, plugins.join("linked")).unwrap();
        remove(&plugins, "linked").unwrap();
        assert_eq!(names_in(&plugins), ["weather_pane"]);
        assert_eq!(names_in(&elsewhere), ["main.lua"]);
    }

    #[test]
    fn save_and_read_need_the_folder() {
        let (_tmp, plugins) = plugins();
        std::fs::create_dir_all(&plugins).unwrap();
        let none = Err("You have no plugin named wait_full.".to_string());
        assert_eq!(save(&plugins, "wait_full", manifest("main.lua"), ""), none);
        assert_eq!(read(&plugins, "wait_full", None).map(|_| ()), none);
        let leftover = &names_in(&plugins);
        assert!(leftover.is_empty(), "{leftover:?}");
    }
}
