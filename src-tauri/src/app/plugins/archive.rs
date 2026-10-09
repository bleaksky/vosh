//! A plugin as it travels from one player to another. Export to Downloads
//! writes a plugin's folder as one .zip, and
//! Install takes a .zip or a folder you drop on the window.
//!
//! Install reads all of it here, and nothing lands on disk until every
//! check passes: 5 MB and 200 files at most, unpacked as well as packed
//! so a zip bomb counts, no entry with `..`, a root, a drive or a link,
//! every entry in one folder, and a manifest whose name keeps the rule
//! New plugin shows. The files then land in a folder of their own beside
//! your plugins, which takes the place of the old one by a rename.

use std::fmt::Display;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use tracing::warn;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::folder::{self, plugin_name_ok};
use super::{PluginManifest, PluginManifestFile};

/// The most an install takes, the archive and its files unpacked alike.
const MAX_BYTES: u64 = 5 * 1024 * 1024;

/// The most files an install takes.
const MAX_FILES: usize = 200;

/// The file type bits of a unix mode, and their value for a link.
const S_IFMT: u32 = 0o170_000;
const S_IFLNK: u32 = 0o120_000;

/// The folder Finder adds when it compresses one, with the extended
/// attributes of each file, which no plugin needs.
const FINDER_EXTRAS: &str = "__MACOSX";

/// One file of a folder you drop on the window, as the page reads it.
#[derive(Debug, Deserialize)]
pub(crate) struct DroppedFile {
    /// Its path inside what you dropped, with `/` between parts.
    pub(crate) path: String,
    pub(crate) bytes: Vec<u8>,
}

/// What Install reads a plugin from.
#[derive(Debug)]
pub(crate) enum Source {
    /// The bytes of a .zip.
    Zip(Vec<u8>),
    /// The files of a dropped folder.
    Folder(Vec<DroppedFile>),
}

/// A plugin that passed every check, ready to land in the plugins
/// folder.
#[derive(Debug)]
pub(crate) struct Package {
    /// The name of the file or folder you picked, which every sentence
    /// about the install names.
    file: String,
    pub(crate) manifest: PluginManifest,
    /// Each file by its parts inside the plugin's folder, with its bytes.
    files: Vec<(Vec<String>, Vec<u8>)>,
}

/// One entry of what you picked, by its parts inside it. A folder has no
/// bytes.
struct Entry {
    parts: Vec<String>,
    bytes: Option<Vec<u8>>,
}

// What Install says when it refuses `file`.

fn no_manifest(file: &str) -> String {
    format!("Vosh found no manifest.toml in {file}.")
}

fn outside(file: &str) -> String {
    format!("Vosh did not install {file}. It holds a file outside its own folder.")
}

fn too_big(file: &str) -> String {
    format!("Vosh did not install {file}. It holds more than 5 MB or 200 files.")
}

/// Read `source`, which you picked as `file`, and check all of it.
/// Returns the plugin it holds, or the sentence that says why Vosh
/// refuses it.
pub(crate) fn read(file: &str, source: Source) -> Result<Package, String> {
    let entries = match source {
        Source::Zip(bytes) => zip_entries(file, &bytes)?,
        Source::Folder(dropped) => folder_entries(file, dropped)?,
    };
    let depth = plugin_depth(file, &entries)?;
    let mut manifest = None;
    let mut files = Vec::new();
    for entry in entries {
        let Some(bytes) = entry.bytes else {
            continue;
        };
        let parts = entry.parts[depth..].to_vec();
        if parts == ["manifest.toml"] {
            manifest = Some(parse_manifest(file, &bytes)?);
        }
        files.push((parts, bytes));
    }
    let manifest = manifest.ok_or_else(|| no_manifest(file))?;
    if !plugin_name_ok(&manifest.name) {
        return Err(format!(
            "Vosh did not install {file}. Its plugin name holds more than letters, digits, and underscores."
        ));
    }
    Ok(Package {
        file: file.to_string(),
        manifest,
        files,
    })
}

/// The entries of the .zip `bytes`, each read in full. Reading stops
/// one byte past the cap, whatever an entry claims its size is.
fn zip_entries(file: &str, bytes: &[u8]) -> Result<Vec<Entry>, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(too_big(file));
    }
    let unreadable = |e: &dyn Display| {
        warn!(file, error = %e, "could not read a plugin archive");
        format!("Vosh could not read {file}.")
    };
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| unreadable(&e))?;
    let mut entries = Vec::new();
    let mut files = 0;
    let mut total = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| unreadable(&e))?;
        let parts = parts_of(entry.name()).ok_or_else(|| outside(file))?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & S_IFMT == S_IFLNK)
        {
            return Err(outside(file));
        }
        if parts[0] == FINDER_EXTRAS {
            continue;
        }
        if entry.is_dir() {
            entries.push(Entry { parts, bytes: None });
            continue;
        }
        files += 1;
        if files > MAX_FILES {
            return Err(too_big(file));
        }
        let mut data = Vec::new();
        (&mut entry)
            .take(MAX_BYTES - total + 1)
            .read_to_end(&mut data)
            .map_err(|e| unreadable(&e))?;
        total += data.len() as u64;
        if total > MAX_BYTES {
            return Err(too_big(file));
        }
        entries.push(Entry {
            parts,
            bytes: Some(data),
        });
    }
    Ok(entries)
}

/// The entries of a dropped folder. The page reads links as the files
/// they lead to, so a folder can hold none.
fn folder_entries(file: &str, dropped: Vec<DroppedFile>) -> Result<Vec<Entry>, String> {
    let total: u64 = dropped.iter().map(|f| f.bytes.len() as u64).sum();
    if dropped.len() > MAX_FILES || total > MAX_BYTES {
        return Err(too_big(file));
    }
    dropped
        .into_iter()
        .map(|f| {
            Ok(Entry {
                parts: parts_of(&f.path).ok_or_else(|| outside(file))?,
                bytes: Some(f.bytes),
            })
        })
        .collect()
}

/// The parts of `name`, an entry's path inside what you picked, or None
/// when it climbs out with `..`, starts at a root or a drive, or names
/// nothing. Both `/` and `\` part a name, since a .zip made on Windows
/// may use either.
fn parts_of(name: &str) -> Option<Vec<String>> {
    if name.starts_with(['/', '\\']) {
        return None;
    }
    let mut parts = Vec::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            // A drive, like `C:`, leads out on Windows.
            _ if part.contains(':') => return None,
            _ => parts.push(part.to_string()),
        }
    }
    // Each part must be one plain name on this system too.
    let plain = parts.iter().all(|part| {
        let mut parts = Path::new(part).components();
        matches!(
            (parts.next(), parts.next()),
            (Some(Component::Normal(_)), None)
        )
    });
    (plain && !parts.is_empty()).then_some(parts)
}

/// How many leading parts come before the plugin's own files: none when
/// manifest.toml sits at the root, or one for the folder it sits in,
/// which every entry must then sit in too.
fn plugin_depth(file: &str, entries: &[Entry]) -> Result<usize, String> {
    let manifest_at = |entry: &Entry, depth: usize| {
        entry.bytes.is_some()
            && entry.parts.len() == depth + 1
            && entry.parts[depth] == "manifest.toml"
    };
    if entries.iter().any(|entry| manifest_at(entry, 0)) {
        return Ok(0);
    }
    let Some(top) = entries
        .iter()
        .find(|entry| manifest_at(entry, 1))
        .map(|entry| &entry.parts[0])
    else {
        return Err(no_manifest(file));
    };
    let inside = entries
        .iter()
        .all(|entry| entry.parts[0] == *top && (entry.bytes.is_none() || entry.parts.len() > 1));
    if inside {
        Ok(1)
    } else {
        Err(outside(file))
    }
}

fn parse_manifest(file: &str, bytes: &[u8]) -> Result<PluginManifest, String> {
    std::str::from_utf8(bytes)
        .map_err(|e| e.to_string())
        .and_then(|text| toml::from_str::<PluginManifestFile>(text).map_err(|e| e.to_string()))
        .map(|parsed| parsed.plugin)
        .map_err(|e| {
            warn!(file, error = %e, "could not read a plugin manifest");
            format!("Vosh could not read the manifest.toml in {file}.")
        })
}

/// Put `package` in `plugins_dir` as the folder its manifest names, in
/// place of a folder of that name. Its files land in a folder of their
/// own first, under a name no plugin can have, and the old folder moves
/// aside before the new one moves in, so a failure on the way leaves the
/// plugin you had.
pub(crate) fn install(plugins_dir: &Path, package: &Package) -> Result<(), String> {
    let name = &package.manifest.name;
    let could_not = |e: &dyn Display| {
        warn!(plugin = %name, error = %e, "could not install a plugin");
        format!("Vosh could not install {}.", package.file)
    };
    std::fs::create_dir_all(plugins_dir).map_err(|e| could_not(&e))?;
    let staged = plugins_dir.join(format!(".{name}.install"));
    let old = plugins_dir.join(format!(".{name}.old"));
    // What an install that never finished left behind.
    for leftover in [&staged, &old] {
        let _ = std::fs::remove_dir_all(leftover);
    }
    std::fs::create_dir(&staged).map_err(|e| could_not(&e))?;
    if let Err(e) = unpack(&staged, &package.files) {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(match e {
            Unpack::Outside => outside(&package.file),
            Unpack::Io(e) => could_not(&e),
        });
    }
    let target = plugins_dir.join(name);
    let had_old = target.symlink_metadata().is_ok();
    if had_old {
        if let Err(e) = std::fs::rename(&target, &old) {
            let _ = std::fs::remove_dir_all(&staged);
            return Err(could_not(&e));
        }
    }
    if let Err(e) = std::fs::rename(&staged, &target) {
        if had_old {
            let _ = std::fs::rename(&old, &target);
        }
        let _ = std::fs::remove_dir_all(&staged);
        return Err(could_not(&e));
    }
    if had_old {
        let _ = std::fs::remove_dir_all(&old);
    }
    Ok(())
}

/// Why [`unpack`] stopped.
enum Unpack {
    Outside,
    Io(std::io::Error),
}

impl From<std::io::Error> for Unpack {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Write `files` into the new folder `dir`. Each file's folder is joined,
/// made and canonicalized, and must sit inside `dir` before the file is
/// written, and a file is only ever made new, so none is written through
/// a link.
fn unpack(dir: &Path, files: &[(Vec<String>, Vec<u8>)]) -> Result<(), Unpack> {
    let real_dir = dir.canonicalize()?;
    for (parts, bytes) in files {
        let path: PathBuf = parts
            .iter()
            .fold(dir.to_path_buf(), |path, part| path.join(part));
        let parent = path.parent().unwrap_or(dir);
        std::fs::create_dir_all(parent)?;
        if !parent.canonicalize()?.starts_with(&real_dir) {
            return Err(Unpack::Outside);
        }
        File::create_new(&path)?.write_all(bytes)?;
    }
    Ok(())
}

/// Write the plugin `name` in `plugins_dir` to `<name>.zip` in
/// `downloads`, or `<name> (2).zip` and on when that file is there, so an
/// export never replaces a file you have. Its regular files go in
/// deflated under `<name>/`. Returns where the file went.
pub(crate) fn export(plugins_dir: &Path, name: &str, downloads: &Path) -> Result<PathBuf, String> {
    let dir = folder::existing(plugins_dir, name)?;
    let could_not = |e: &dyn Display| {
        warn!(plugin = %name, error = %e, "could not export a plugin");
        format!("Vosh could not save {name} in your Downloads folder.")
    };
    let files = folder::regular_files(&dir).map_err(|e| could_not(&e))?;
    let path = crate::disk::paths::export_path(downloads, name, "zip");
    // Made new, so a file that came since the look stays as it is.
    let out = File::create_new(&path).map_err(|e| could_not(&e))?;
    if let Err(e) = write_zip(out, &dir, name, &files) {
        let _ = std::fs::remove_file(&path);
        return Err(could_not(&e));
    }
    Ok(path)
}

/// Write `files`, by their paths inside `dir`, to `out` as a .zip, each
/// under `<name>/`.
fn write_zip(out: File, dir: &Path, name: &str, files: &[String]) -> zip::result::ZipResult<()> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(out);
    for file in files {
        zip.start_file(format!("{name}/{file}"), options)?;
        std::io::copy(&mut File::open(dir.join(file))?, &mut zip)?;
    }
    zip.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_page_holds_the_same_caps_and_refusal() {
        // A dropped folder is checked as the page reads it, in
        // src/settings/scripts/pluginPackage.ts, so a folder dropped by
        // mistake is never read whole into the window.
        const MB: u64 = 1024 * 1024;
        let page = include_str!("../../../../src/settings/scripts/pluginPackage.ts");
        assert!(page.contains(&format!(
            "const MAX_BYTES = {} * 1024 * 1024;",
            MAX_BYTES / MB
        )));
        assert!(page.contains(&format!("const MAX_FILES = {MAX_FILES};")));
        let refusal = too_big("${fileName}");
        assert!(
            page.contains(&format!("new Error(`{refusal}`)")),
            "{refusal}"
        );
        // The refusal names the caps it enforces.
        assert!(refusal.contains(&format!(
            "more than {} MB or {MAX_FILES} files",
            MAX_BYTES / MB
        )));
    }

    use super::*;

    /// The manifest of `weather_pane`, a sample plugin with a pane.
    const WEATHER_MANIFEST: &str = "[plugin]
name = \"weather_pane\"
version = \"0.2.0\"
description = \"Show the weather, your position and your language in a pane of its own.\"
author = \"Tolliver\"
entry = \"main.lua\"
";
    const WEATHER_MAIN: &str = "-- weather_pane\n-- Vosh runs this file when weather_pane is on.\n";

    const NO_MANIFEST: &str = "Vosh found no manifest.toml in weather_pane.zip.";
    const OUTSIDE: &str =
        "Vosh did not install weather_pane.zip. It holds a file outside its own folder.";
    const TOO_BIG: &str =
        "Vosh did not install weather_pane.zip. It holds more than 5 MB or 200 files.";

    /// A .zip that holds `entries`, each a name and its bytes, deflated.
    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    /// `weather_pane` in one folder, with `extra` beside it in the .zip.
    fn weather_zip(extra: &[(&str, &[u8])]) -> Vec<u8> {
        let mut entries: Vec<(&str, &[u8])> = vec![
            ("weather_pane/manifest.toml", WEATHER_MANIFEST.as_bytes()),
            ("weather_pane/main.lua", WEATHER_MAIN.as_bytes()),
        ];
        entries.extend_from_slice(extra);
        zip_of(&entries)
    }

    fn read_zip(bytes: Vec<u8>) -> Result<Package, String> {
        read("weather_pane.zip", Source::Zip(bytes))
    }

    fn dropped(path: &str, bytes: &[u8]) -> DroppedFile {
        DroppedFile {
            path: path.to_string(),
            bytes: bytes.to_vec(),
        }
    }

    fn plugins() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugins = tmp.path().join("plugins");
        (tmp, plugins)
    }

    /// Every file under `dir` with its text, by its path inside it.
    fn tree(dir: &Path) -> Vec<(String, String)> {
        folder::regular_files(dir)
            .unwrap()
            .into_iter()
            .map(|file| {
                let text = std::fs::read_to_string(dir.join(&file)).unwrap();
                (file, text)
            })
            .collect()
    }

    /// The names in the folder `dir`, sorted, hidden ones included.
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn every_refusal_reads_as_board_four_words_it() {
        assert_eq!(
            read_zip(zip_of(&[("weather_pane/main.lua", b"")])).unwrap_err(),
            NO_MANIFEST
        );
        assert_eq!(
            read_zip(weather_zip(&[("weather_pane/../x", b"")])).unwrap_err(),
            OUTSIDE
        );
        let big = vec![0u8; 6 * 1024 * 1024];
        assert_eq!(
            read_zip(weather_zip(&[("weather_pane/big.txt", &big)])).unwrap_err(),
            TOO_BIG
        );
    }

    #[test]
    fn an_entry_that_leads_out_is_refused() {
        let absolute = if cfg!(windows) { "C:/x" } else { "/x" };
        for name in [
            "../x",
            "weather_pane/../../x",
            absolute,
            "/weather_pane/x",
            "\\x",
            "weather_pane\\..\\x",
            "C:x",
        ] {
            // Refused by its own parts, not only by the folder check.
            assert_eq!(parts_of(name), None, "{name}");
            let zip = weather_zip(&[(name, b"mud.send('look')")]);
            assert_eq!(read_zip(zip).unwrap_err(), OUTSIDE, "{name}");
            let folder = vec![
                dropped("weather_pane/manifest.toml", WEATHER_MANIFEST.as_bytes()),
                dropped(name, b"mud.send('look')"),
            ];
            let refused = read("weather_pane", Source::Folder(folder)).unwrap_err();
            assert_eq!(
                refused,
                "Vosh did not install weather_pane. It holds a file outside its own folder.",
                "{name}"
            );
        }
    }

    #[test]
    fn a_link_entry_is_refused() {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for (name, text) in [
            ("weather_pane/manifest.toml", WEATHER_MANIFEST),
            ("weather_pane/main.lua", WEATHER_MAIN),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.add_symlink("weather_pane/notes.lua", "/etc/hosts", options)
            .unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert_eq!(read_zip(bytes).unwrap_err(), OUTSIDE);
    }

    #[test]
    fn entries_in_more_than_one_top_folder_are_refused() {
        for extra in ["other/x.lua", "notes.txt", "weather_pane"] {
            let zip = weather_zip(&[(extra, b"")]);
            assert_eq!(read_zip(zip).unwrap_err(), OUTSIDE, "{extra}");
        }
        // A manifest at the root lets every entry sit where it likes.
        let zip = zip_of(&[
            ("manifest.toml", WEATHER_MANIFEST.as_bytes()),
            ("main.lua", WEATHER_MAIN.as_bytes()),
            ("lib/draw.lua", b"-- draw\n"),
        ]);
        let package = read_zip(zip).unwrap();
        let mut paths: Vec<String> = package.files.iter().map(|(p, _)| p.join("/")).collect();
        paths.sort();
        assert_eq!(paths, ["lib/draw.lua", "main.lua", "manifest.toml"]);
        // A manifest two folders down is none.
        let zip = zip_of(&[("a/weather_pane/manifest.toml", WEATHER_MANIFEST.as_bytes())]);
        assert_eq!(read_zip(zip).unwrap_err(), NO_MANIFEST);
    }

    #[test]
    fn more_than_two_hundred_files_are_refused() {
        let names: Vec<String> = (0..199).map(|n| format!("weather_pane/f{n}.lua")).collect();
        let extra: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        // weather_pane's two files and 199 more make 201.
        assert_eq!(read_zip(weather_zip(&extra)).unwrap_err(), TOO_BIG);
        assert!(read_zip(weather_zip(&extra[1..])).is_ok());
        let mut folder: Vec<DroppedFile> = names.iter().map(|n| dropped(n, b"")).collect();
        folder.push(dropped(
            "weather_pane/manifest.toml",
            WEATHER_MANIFEST.as_bytes(),
        ));
        folder.push(dropped("weather_pane/main.lua", WEATHER_MAIN.as_bytes()));
        assert_eq!(
            read("weather_pane", Source::Folder(folder)).unwrap_err(),
            "Vosh did not install weather_pane. It holds more than 5 MB or 200 files."
        );
    }

    #[test]
    fn more_than_five_mb_is_refused_packed_or_unpacked() {
        // Six MB of zeros pack into a few kB, so only the unpacked total
        // catches them.
        let zeros = vec![0u8; 6 * 1024 * 1024];
        let bomb = weather_zip(&[("weather_pane/big.txt", &zeros)]);
        assert!(bomb.len() < 64 * 1024, "{}", bomb.len());
        assert_eq!(read_zip(bomb).unwrap_err(), TOO_BIG);
        // Three files of 2 MB each pass one by one but not together.
        let two = vec![0u8; 2 * 1024 * 1024];
        let parts: Vec<(&str, &[u8])> = vec![
            ("weather_pane/a.txt", &two),
            ("weather_pane/b.txt", &two),
            ("weather_pane/c.txt", &two),
        ];
        assert_eq!(read_zip(weather_zip(&parts)).unwrap_err(), TOO_BIG);
        assert!(read_zip(weather_zip(&parts[1..])).is_ok());
        // An archive over the cap is refused before it is read.
        let packed = vec![0u8; 5 * 1024 * 1024 + 1];
        assert_eq!(read_zip(packed).unwrap_err(), TOO_BIG);
        let folder = vec![
            dropped("weather_pane/manifest.toml", WEATHER_MANIFEST.as_bytes()),
            dropped("weather_pane/big.txt", &zeros),
        ];
        assert_eq!(
            read("weather_pane", Source::Folder(folder)).unwrap_err(),
            "Vosh did not install weather_pane. It holds more than 5 MB or 200 files."
        );
    }

    #[test]
    fn the_manifest_name_keeps_the_rule() {
        let manifest = WEATHER_MANIFEST.replace("\"weather_pane\"", "\"../weather\"");
        let zip = zip_of(&[("weather_pane/manifest.toml", manifest.as_bytes())]);
        assert_eq!(
            read_zip(zip).unwrap_err(),
            "Vosh did not install weather_pane.zip. Its plugin name holds more than letters, digits, and underscores."
        );
        let zip = zip_of(&[("weather_pane/manifest.toml", b"[plugin\n")]);
        assert_eq!(
            read_zip(zip).unwrap_err(),
            "Vosh could not read the manifest.toml in weather_pane.zip."
        );
        assert_eq!(
            read_zip(b"not a zip".to_vec()).unwrap_err(),
            "Vosh could not read weather_pane.zip."
        );
    }

    #[test]
    fn what_finder_adds_stays_out() {
        let zip = weather_zip(&[
            ("weather_pane/", b""),
            ("__MACOSX/weather_pane/._main.lua", b"\0\x05\x16\x07"),
        ]);
        let (_tmp, plugins) = plugins();
        install(&plugins, &read_zip(zip).unwrap()).unwrap();
        assert_eq!(names_in(&plugins), ["weather_pane"]);
        assert_eq!(
            tree(&plugins.join("weather_pane")),
            [
                ("main.lua".to_string(), WEATHER_MAIN.to_string()),
                ("manifest.toml".to_string(), WEATHER_MANIFEST.to_string()),
            ]
        );
    }

    #[test]
    fn install_takes_the_place_of_the_folder_you_had() {
        let (_tmp, plugins) = plugins();
        folder::create(&plugins, "weather_pane").unwrap();
        std::fs::write(plugins.join("weather_pane").join("old.lua"), "-- old\n").unwrap();
        let package =
            read_zip(weather_zip(&[("weather_pane/lib/draw.lua", b"-- draw\n")])).unwrap();
        assert_eq!(package.manifest.version, "0.2.0");
        install(&plugins, &package).unwrap();
        // Nothing of the old folder stays, and nothing of the install is
        // left beside it.
        assert_eq!(names_in(&plugins), ["weather_pane"]);
        assert_eq!(
            tree(&plugins.join("weather_pane")),
            [
                ("lib/draw.lua".to_string(), "-- draw\n".to_string()),
                ("main.lua".to_string(), WEATHER_MAIN.to_string()),
                ("manifest.toml".to_string(), WEATHER_MANIFEST.to_string()),
            ]
        );
        assert_eq!(
            super::super::read_plugin(&plugins, "weather_pane")
                .unwrap()
                .code,
            WEATHER_MAIN
        );
        // A dropped folder installs the same way, here with no folder of
        // its own around the files.
        let folder = vec![
            dropped("manifest.toml", WEATHER_MANIFEST.as_bytes()),
            dropped("main.lua", b"-- dropped\n"),
        ];
        install(
            &plugins,
            &read("weather_pane", Source::Folder(folder)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            tree(&plugins.join("weather_pane")),
            [
                ("main.lua".to_string(), "-- dropped\n".to_string()),
                ("manifest.toml".to_string(), WEATHER_MANIFEST.to_string()),
            ]
        );
    }

    #[test]
    fn an_export_installs_as_it_left() {
        let (tmp, plugins) = plugins();
        let downloads = tmp.path().join("Downloads");
        std::fs::create_dir(&downloads).unwrap();
        folder::create(&plugins, "wait_full").unwrap();
        let dir = plugins.join("wait_full");
        std::fs::create_dir(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib").join("draw.lua"), "-- draw\n").unwrap();
        let first = export(&plugins, "wait_full", &downloads).unwrap();
        assert_eq!(first, downloads.join("wait_full.zip"));
        // A second export never replaces the first.
        let second = export(&plugins, "wait_full", &downloads).unwrap();
        assert_eq!(second, downloads.join("wait_full (2).zip"));
        let other = tmp.path().join("elsewhere");
        let bytes = std::fs::read(&first).unwrap();
        install(&other, &read("wait_full.zip", Source::Zip(bytes)).unwrap()).unwrap();
        assert_eq!(tree(&other.join("wait_full")), tree(&dir));
        assert_eq!(names_in(&other), ["wait_full"]);
    }

    #[test]
    fn export_carries_regular_files_only() {
        let (tmp, plugins) = plugins();
        let downloads = tmp.path().join("Downloads");
        std::fs::create_dir(&downloads).unwrap();
        folder::create(&plugins, "wait_full").unwrap();
        // Two saves keep no backup to ride along.
        for code in ["mud.send('stand')\n", "mud.send('rest')\n"] {
            let mut manifest = folder::read(&plugins, "wait_full", None).unwrap().manifest;
            manifest.author = "Orla".into();
            folder::save(&plugins, "wait_full", manifest, code).unwrap();
        }
        #[cfg(unix)]
        {
            let secret = tmp.path().join("secret");
            std::fs::create_dir(&secret).unwrap();
            std::fs::write(secret.join("notes.lua"), "-- yours\n").unwrap();
            let dir = plugins.join("wait_full");
            std::os::unix::fs::symlink(secret.join("notes.lua"), dir.join("notes.lua")).unwrap();
            std::os::unix::fs::symlink(&secret, dir.join("away")).unwrap();
        }
        let path = export(&plugins, "wait_full", &downloads).unwrap();
        let mut archive = ZipArchive::new(File::open(path).unwrap()).unwrap();
        let mut names: Vec<String> = archive.file_names().map(str::to_string).collect();
        names.sort();
        assert_eq!(names, ["wait_full/main.lua", "wait_full/manifest.toml"]);
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            assert_eq!(entry.compression(), CompressionMethod::Deflated);
            assert!(entry.is_file(), "{}", entry.name());
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            assert!(!text.contains("yours"), "{text}");
        }
        assert_eq!(
            export(&plugins, "../wait_full", &downloads).unwrap_err(),
            folder::NAME_RULE
        );
        assert_eq!(
            export(&plugins, "weather_pane", &downloads).unwrap_err(),
            "You have no plugin named weather_pane."
        );
    }
}
