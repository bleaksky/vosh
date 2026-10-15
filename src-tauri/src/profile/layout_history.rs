//! The pane layouts each profile had before, which arrange mode and
//! Panel layout in Settings > Characters offer to put back. Each profile
//! keeps its own file, `layouts/<name>.toml`, newest first and at most
//! [`KEEP`] long.
//!
//! A layout goes in when a change of which panes show, or where, replaces
//! it. A run of changes counts once, so a layout you kept only for a few
//! minutes inside a run does not crowd out the ones you kept for longer.
//! Arranging, a reset and a restore always count. The first launch of a
//! new version keeps every profile's layout as it found it, marked with
//! that version, before anything this version does can change it. A
//! change of sizes alone never counts.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::disk::atomic::swap_in;
use crate::disk::paths;
use crate::profile::file::ProfileConfig;
use crate::profile::panes::{PaneLayoutPersist, PaneNode, LUA_PANE};
use crate::profile::set::ProfileSet;

/// How many earlier layouts a profile keeps.
pub(crate) const KEEP: usize = 20;

/// How long after a counted change the next change still belongs to the
/// same run.
const RUN_MS: i64 = 10 * 60 * 1000;

/// What replaced a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Why {
    /// A pane closed, added or moved, in the panel or in Settings.
    Change,
    /// You arranged the panes and pressed Done.
    Arrange,
    /// Reset to default.
    Reset,
    /// You put an earlier layout back.
    Restore,
    /// A new version of Vosh started.
    Update,
}

impl Why {
    /// Whether a change of this kind always counts, rather than once a run.
    fn always(self) -> bool {
        self != Self::Change
    }
}

/// One layout a profile had before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Earlier {
    /// When it gave way, in Unix milliseconds. It also names the entry.
    pub until: i64,
    pub why: Why,
    /// For [`Why::Update`], the version that started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub panes: PaneLayoutPersist,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    earlier: Vec<Earlier>,
    #[serde(flatten)]
    extra: BTreeMap<String, toml::Value>,
}

/// The version stamp `layouts/seen.toml` keeps.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Seen {
    #[serde(default)]
    version: String,
}

/// The time now in Unix milliseconds.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn read_file(app_data: &Path, name: &str) -> HistoryFile {
    let path = paths::layout_history_path(app_data, name);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return HistoryFile::default();
    };
    toml::from_str(&text).unwrap_or_else(|e| {
        warn!(error = %e, path = %path.display(), "earlier layouts did not read");
        HistoryFile::default()
    })
}

fn write_file(app_data: &Path, name: &str, file: &HistoryFile) {
    let path = paths::layout_history_path(app_data, name);
    let written = std::fs::create_dir_all(paths::layouts_dir(app_data))
        .map_err(|e| e.to_string())
        .and_then(|()| toml::to_string_pretty(file).map_err(|e| e.to_string()))
        .and_then(|text| swap_in(&path, &text).map_err(|e| e.to_string()));
    if let Err(e) = written {
        warn!(error = %e, path = %path.display(), "earlier layouts did not save");
    }
}

/// The layouts profile `name` had before, newest first.
pub(crate) fn read(app_data: &Path, name: &str) -> Vec<Earlier> {
    read_file(app_data, name).earlier
}

/// What a tree shows, without sizes or ids: each pane by its type, a Lua
/// pane by its plugin and id too, and each split by its direction.
fn shape(node: &PaneNode, out: &mut String) {
    if let Some(pane) = &node.pane {
        out.push_str(pane);
        if pane == LUA_PANE {
            for key in ["plugin", "id"] {
                out.push(':');
                out.push_str(node.props.get(key).map_or("", String::as_str));
            }
        }
        out.push(';');
        return;
    }
    out.push_str(node.split.as_deref().unwrap_or("column"));
    out.push('(');
    for child in &node.children {
        shape(child, out);
    }
    out.push(')');
}

/// Whether `a` and `b` show the same panes in the same places, whatever
/// their sizes.
pub(crate) fn same_arrangement(a: &PaneLayoutPersist, b: &PaneLayoutPersist) -> bool {
    let of = |l: &PaneLayoutPersist| {
        let mut s = String::new();
        shape(&l.root, &mut s);
        s.push('|');
        shape(&l.sidebar, &mut s);
        s
    };
    of(a) == of(b)
}

/// Keep `before`, the layout `after` replaces on profile `name`, when the
/// two differ in more than sizes. A [`Why::Change`] counts once a run,
/// and a layout the same as the newest one kept is not kept again.
pub(crate) fn note(
    app_data: &Path,
    name: &str,
    before: &PaneLayoutPersist,
    after: &PaneLayoutPersist,
    why: Why,
    now: i64,
) {
    if same_arrangement(before, after) {
        return;
    }
    let mut file = read_file(app_data, name);
    if let Some(newest) = file.earlier.first() {
        let in_run =
            !why.always() && newest.why == Why::Change && now.saturating_sub(newest.until) < RUN_MS;
        if in_run || same_arrangement(&newest.panes, before) {
            return;
        }
    }
    push(
        &mut file,
        Earlier {
            until: now,
            why,
            version: None,
            panes: before.clone(),
        },
    );
    write_file(app_data, name, &file);
}

fn push(file: &mut HistoryFile, entry: Earlier) {
    file.earlier.insert(0, entry);
    file.earlier.truncate(KEEP);
}

/// On the first launch of `version`, keep each profile's layout as its
/// file holds it, marked with the version. Runs at launch before any
/// profile opens, so nothing this version does has touched the files.
/// `layouts/seen.toml` names the last version that did this.
pub(crate) fn note_update(set: &ProfileSet, app_data: &Path, version: &str, now: i64) {
    let seen_path = paths::layouts_seen_path(app_data);
    let seen: Seen = std::fs::read_to_string(&seen_path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    if seen.version == version {
        return;
    }
    for entry in set.list() {
        let path = set.profile_path(&entry.name);
        if !path.is_file() {
            continue;
        }
        let Ok(config) = ProfileConfig::load(&path) else {
            continue;
        };
        let Some(panes) = config.ui.panes else {
            continue;
        };
        let mut file = read_file(app_data, &entry.name);
        push(
            &mut file,
            Earlier {
                until: now,
                why: Why::Update,
                version: Some(version.to_string()),
                panes,
            },
        );
        // The same layout kept just before adds nothing beside the mark.
        if file.earlier.len() > 1
            && same_arrangement(&file.earlier[0].panes, &file.earlier[1].panes)
        {
            file.earlier.remove(1);
        }
        write_file(app_data, &entry.name, &file);
    }
    let stamp = Seen {
        version: version.to_string(),
    };
    let written = std::fs::create_dir_all(paths::layouts_dir(app_data))
        .map_err(|e| e.to_string())
        .and_then(|()| toml::to_string(&stamp).map_err(|e| e.to_string()))
        .and_then(|text| swap_in(&seen_path, &text).map_err(|e| e.to_string()));
    if let Err(e) = written {
        warn!(error = %e, "the version earlier layouts last kept did not save");
    }
}

/// Move profile `old`'s earlier layouts to `new`, as a rename does.
pub(crate) fn rename(app_data: &Path, old: &str, new: &str) {
    let from = paths::layout_history_path(app_data, old);
    if from.is_file() {
        if let Err(e) = std::fs::rename(&from, paths::layout_history_path(app_data, new)) {
            warn!(error = %e, "earlier layouts did not follow the rename");
        }
    }
}

/// Forget profile `name`'s earlier layouts, as a delete does.
pub(crate) fn delete(app_data: &Path, name: &str) {
    let path = paths::layout_history_path(app_data, name);
    if path.is_file() {
        if let Err(e) = std::fs::remove_file(&path) {
            warn!(error = %e, "earlier layouts did not go with the profile");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(root: &[&str], sidebar: &[&str]) -> PaneLayoutPersist {
        let mut l = PaneLayoutPersist::default_layout();
        l.root = PaneNode::split(
            "root",
            "column",
            root.iter().map(|p| PaneNode::leaf(p, 1.0)).collect(),
        );
        l.sidebar = PaneNode::split(
            "sidebar",
            "column",
            sidebar.iter().map(|p| PaneNode::leaf(p, 1.0)).collect(),
        );
        l.sanitize();
        l
    }

    fn panes_of(e: &Earlier) -> Vec<String> {
        let mut s = String::new();
        shape(&e.panes.root, &mut s);
        s.push('|');
        shape(&e.panes.sidebar, &mut s);
        vec![s]
    }

    #[test]
    fn a_change_keeps_the_layout_it_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let a = layout(&["map", "affects"], &["stats"]);
        let b = layout(&["map"], &["stats", "affects"]);
        note(dir.path(), "Tolliver", &a, &b, Why::Change, 1_000);
        let kept = read(dir.path(), "Tolliver");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].until, 1_000);
        assert_eq!(kept[0].why, Why::Change);
        assert!(same_arrangement(&kept[0].panes, &a));
    }

    #[test]
    fn sizes_alone_never_count() {
        let dir = tempfile::tempdir().unwrap();
        let a = layout(&["map", "affects"], &[]);
        let mut b = a.clone();
        b.root.children[0].weight = 0.8;
        b.root.children[1].weight = 0.2;
        b.panel_width = Some(420);
        note(dir.path(), "Tolliver", &a, &b, Why::Change, 1_000);
        assert_eq!(read(dir.path(), "Tolliver"), Vec::<Earlier>::new());
    }

    #[test]
    fn a_run_of_changes_counts_once_and_arranging_always_counts() {
        let dir = tempfile::tempdir().unwrap();
        let a = layout(&["map", "affects"], &[]);
        let b = layout(&["map"], &["affects"]);
        let c = layout(&["map", "chat"], &["affects"]);
        let d = layout(&["chat"], &["affects", "map"]);
        note(dir.path(), "Maren", &a, &b, Why::Change, 0);
        note(dir.path(), "Maren", &b, &c, Why::Change, 5 * 60 * 1000);
        assert_eq!(read(dir.path(), "Maren").len(), 1, "b fell inside the run");
        note(dir.path(), "Maren", &c, &d, Why::Arrange, 6 * 60 * 1000);
        let kept = read(dir.path(), "Maren");
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].why, Why::Arrange);
        assert!(same_arrangement(&kept[0].panes, &c));
        note(dir.path(), "Maren", &d, &a, Why::Change, 30 * 60 * 1000);
        assert_eq!(
            read(dir.path(), "Maren").len(),
            3,
            "a change after the run counts"
        );
    }

    #[test]
    fn the_same_layout_is_not_kept_twice_in_a_row() {
        let dir = tempfile::tempdir().unwrap();
        let a = layout(&["map", "affects"], &[]);
        let b = layout(&["map"], &["affects"]);
        note(dir.path(), "Orla", &a, &b, Why::Reset, 0);
        note(dir.path(), "Orla", &a, &b, Why::Restore, 1);
        assert_eq!(read(dir.path(), "Orla").len(), 1);
    }

    #[test]
    fn a_profile_keeps_twenty() {
        let dir = tempfile::tempdir().unwrap();
        let a = layout(&["map", "affects"], &[]);
        let b = layout(&["map"], &["affects"]);
        for n in 0..30 {
            let (before, after) = if n % 2 == 0 { (&a, &b) } else { (&b, &a) };
            note(dir.path(), "Orla", before, after, Why::Arrange, n);
        }
        let kept = read(dir.path(), "Orla");
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept[0].until, 29);
    }

    #[test]
    fn the_first_launch_of_a_version_keeps_each_layout_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Tolliver").unwrap();
        let mut config = ProfileConfig::default();
        config.ui.panes = Some(layout(&["map"], &["stats"]));
        config.save(&set.profile_path("Tolliver")).unwrap();

        note_update(&set, dir.path(), "0.95.6", 5);
        note_update(&set, dir.path(), "0.95.6", 9);

        let kept = read(dir.path(), "Tolliver");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].why, Why::Update);
        assert_eq!(kept[0].version.as_deref(), Some("0.95.6"));
        assert_eq!(panes_of(&kept[0]), ["column(map;)|column(stats;)"]);
        assert_eq!(
            read(dir.path(), "default"),
            Vec::<Earlier>::new(),
            "a profile with no file keeps none"
        );

        note_update(&set, dir.path(), "0.95.7", 20);
        let kept = read(dir.path(), "Tolliver");
        assert_eq!(kept.len(), 1, "the same layout again takes the new mark");
        assert_eq!(kept[0].version.as_deref(), Some("0.95.7"));
    }

    #[test]
    fn a_rename_takes_the_layouts_along_and_a_delete_drops_them() {
        let dir = tempfile::tempdir().unwrap();
        let a = layout(&["map", "affects"], &[]);
        let b = layout(&["map"], &["affects"]);
        note(dir.path(), "Orla", &a, &b, Why::Arrange, 1);
        rename(dir.path(), "Orla", "Maren");
        assert_eq!(read(dir.path(), "Orla"), Vec::<Earlier>::new());
        assert_eq!(read(dir.path(), "Maren").len(), 1);
        delete(dir.path(), "Maren");
        assert_eq!(read(dir.path(), "Maren"), Vec::<Earlier>::new());
    }
}
