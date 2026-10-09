//! The move that gathers the custom themes profile files still hold
//! into global.toml while the `theme` scope category is global. It runs
//! at startup and when the category turns global, keeps every theme,
//! and gives a theme whose id another profile holds for a different
//! theme a fresh id. The hand out that runs when the category turns per
//! profile again merges by the same rules.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::profile::file::{ConfigError, ProfileConfig};
use crate::profile::live::Profile;
use crate::profile::set::ProfileSet;
use crate::profile::shared::{GlobalConfig, ScopeConfig};
use crate::profile::ui::{CustomTheme, UiConfig};

/// True when two custom themes paint the same colors under the same
/// description. Their ids and labels may differ.
fn same_colors(a: &CustomTheme, b: &CustomTheme) -> bool {
    a.description == b.description && a.xterm == b.xterm && a.chrome == b.chrome
}

fn label_taken(list: &[CustomTheme], label: &str) -> bool {
    let wanted = label.trim().to_lowercase();
    list.iter().any(|t| t.label.trim().to_lowercase() == wanted)
}

/// `label` marked with the profile it came from, for example
/// `Ember variant (Healer)`, and clear of every label in `list`.
fn owned_label(label: &str, owner: &str, list: &[CustomTheme]) -> String {
    let marked = format!("{label} ({owner})");
    if !label_taken(list, &marked) {
        return marked;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{label} ({owner} {n})");
        if !label_taken(list, &candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// `base` itself when no theme in `list` holds it, else the first free
/// `base-2`, `base-3`, and so on, the way Settings picks a new id. No
/// built in theme id ends in a number, so these never shadow one.
fn free_theme_id(base: &str, list: &[CustomTheme]) -> String {
    let taken = |id: &str| list.iter().any(|t| t.id == id);
    if !taken(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Add each theme in `incoming` to `list` and keep every one of them.
/// A theme equal to the one that already holds its id is the same theme
/// and merges into it. A different theme under a taken id joins under a
/// fresh id, and when its label is taken too its label names `owner`.
/// Before Settings picked ids from the label, every profile named its
/// first custom theme `custom`, so two profiles often hold different
/// themes under one id. A copy an earlier move added under a fresh id is
/// found again, so a move that runs twice adds nothing twice. Returns
/// the id each incoming theme holds in `list`, in order.
pub(crate) fn merge_custom_themes(
    list: &mut Vec<CustomTheme>,
    incoming: &[CustomTheme],
    owner: &str,
) -> Vec<String> {
    let mut landed = Vec::with_capacity(incoming.len());
    for theme in incoming {
        let Some(holder) = list.iter().find(|t| t.id == theme.id) else {
            list.push(theme.clone());
            landed.push(theme.id.clone());
            continue;
        };
        if holder == theme {
            landed.push(theme.id.clone());
            continue;
        }
        let marked = format!("{} ({owner})", theme.label);
        let copy = list
            .iter()
            .find(|t| same_colors(t, theme) && (t.label == theme.label || t.label == marked));
        if let Some(copy) = copy {
            landed.push(copy.id.clone());
            continue;
        }
        let id = free_theme_id(&theme.id, list);
        let label = if label_taken(list, &theme.label) {
            owned_label(&theme.label, owner, list)
        } else {
            theme.label.clone()
        };
        list.push(CustomTheme {
            id: id.clone(),
            label,
            ..theme.clone()
        });
        landed.push(id);
    }
    landed
}

/// True when every theme in `held` sits in `shared` under the id
/// `landed` gives it, with the same colors.
fn holds_every_theme(shared: &[CustomTheme], held: &[CustomTheme], landed: &[String]) -> bool {
    held.len() == landed.len()
        && held
            .iter()
            .zip(landed)
            .all(|(theme, id)| shared.iter().any(|s| s.id == *id && same_colors(s, theme)))
}

/// Point `ui`'s theme and the themes of its light, dark, day and night
/// slots at the ids its own
/// custom themes `held` landed under, so a theme that moved to a fresh id
/// stays the one that profile shows.
pub(crate) fn follow_moved_ids(ui: &mut UiConfig, held: &[CustomTheme], landed: &[String]) {
    let mut moved: BTreeMap<&str, &str> = BTreeMap::new();
    for (theme, id) in held.iter().zip(landed) {
        moved.entry(theme.id.as_str()).or_insert(id.as_str());
    }
    for field in [
        &mut ui.theme,
        &mut ui.light_theme,
        &mut ui.dark_theme,
        &mut ui.day_theme,
        &mut ui.night_theme,
    ] {
        if let Some(id) = moved.get(field.as_str()) {
            if *id != field.as_str() {
                *field = (*id).to_string();
            }
        }
    }
}

/// Profile files that still hold their own custom themes while the
/// `theme` scope category is global. Files written before custom themes
/// joined that category carry a list, and so does every profile saved
/// while the category was per profile. Each list moves into global.toml
/// once and then leaves its file, so global.toml holds the one list and
/// a theme you delete stays deleted.
pub(crate) struct HeldCustomThemes {
    files: Vec<HeldFile>,
}

struct HeldFile {
    name: String,
    path: PathBuf,
    config: ProfileConfig,
    /// The id each held theme holds in the shared list, set by `add_to`.
    landed: Vec<String>,
}

impl HeldCustomThemes {
    /// Keep the profile files in `set` that hold custom themes, leaving
    /// out the file of `skip`. The active profile comes first and the
    /// rest follow in index order, so when two files hold one id the
    /// themes you see now keep it. A file Vosh cannot read stays as it is.
    pub(crate) fn find(set: &ProfileSet, skip: Option<&str>) -> Self {
        let mut files = Vec::new();
        for stored in set.read_all() {
            if skip == Some(stored.name) {
                continue;
            }
            match stored.file {
                Some(Ok(file)) if !file.config.ui.custom_themes.is_empty() => {
                    files.push(HeldFile {
                        name: stored.name.to_string(),
                        path: stored.path,
                        config: file.config,
                        landed: Vec::new(),
                    });
                }
                Some(Ok(_)) | None => {}
                Some(Err(e)) => {
                    tracing::warn!(
                        error = %e,
                        path = %stored.path.display(),
                        "profile file unreadable",
                    );
                }
            }
        }
        // A stable sort, so the rest keep index order.
        let active = set.active_name();
        files.sort_by_key(|file| file.name != active);
        Self { files }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Add every held theme to `shared` through `merge_custom_themes`,
    /// so a theme under an id `shared` already holds for a different
    /// theme joins under a fresh id instead of being dropped.
    fn add_to(&mut self, shared: &mut Vec<CustomTheme>) {
        for file in &mut self.files {
            let owner = crate::profile::set::display_name(&file.name);
            file.landed = merge_custom_themes(shared, &file.config.ui.custom_themes, &owner);
        }
    }

    /// Clear the list from each file whose every theme now sits in
    /// `shared`, and point its theme references at the ids its themes
    /// landed under. Call only after global.toml holds `shared`. A file
    /// that fails to save keeps its list, and the next launch moves it
    /// again. Returns how many files it cleared.
    fn strip(self, shared: &[CustomTheme]) -> usize {
        let mut cleared = 0;
        for file in self.files {
            let HeldFile {
                path,
                mut config,
                landed,
                ..
            } = file;
            let held = std::mem::take(&mut config.ui.custom_themes);
            if !holds_every_theme(shared, &held, &landed) {
                tracing::warn!(path = %path.display(), "profile file kept custom themes the shared list lacks");
                continue;
            }
            follow_moved_ids(&mut config.ui, &held, &landed);
            match config.save(&path) {
                Ok(()) => cleared += 1,
                Err(e) => {
                    tracing::warn!(error = %e, path = %path.display(), "profile file kept its custom themes");
                }
            }
        }
        cleared
    }
}

/// Move the custom themes that profile files still hold into
/// global.toml, then clear them from those files. Vosh runs this at
/// startup before it loads the active profile, and after the first run
/// no file holds a list, so it finds nothing to do. It leaves every file
/// alone while the `theme` scope category is per profile, since each
/// file then owns its list. Returns how many files it cleared.
pub(crate) fn migrate_custom_themes(set: &ProfileSet) -> Result<usize, ConfigError> {
    if !matches!(set.scope().theme, crate::profile::shared::Scope::Global) {
        return Ok(0);
    }
    let mut held = HeldCustomThemes::find(set, None);
    if held.is_empty() {
        return Ok(0);
    }
    let path = set.global_path();
    let mut global = if path.exists() {
        GlobalConfig::load(&path)?
    } else {
        GlobalConfig::default()
    };
    let mut shared = global.custom_themes.take().unwrap_or_default();
    held.add_to(&mut shared);
    global.custom_themes = Some(shared.clone());
    // global.toml first, so a failure between the writes leaves every
    // theme on disk for the next launch to finish.
    global.save(&path)?;
    Ok(held.strip(&shared))
}

/// Fold the custom themes that the other profile files hold into the
/// live profile when the `theme` scope category turns global, save the
/// shared list to global.toml, and clear those files. Without this a
/// switch to one of those profiles lays the shared list over its own
/// and its themes are gone. `held` comes from `HeldCustomThemes::find`
/// with the active profile skipped, since the live profile holds its
/// list. Call with the new global `scope` and the persist lock held.
/// Returns true when the live list gained a theme.
pub(crate) fn share_custom_themes(
    mut held: HeldCustomThemes,
    scope: &ScopeConfig,
    global_path: &Path,
    live: &mut Profile,
) -> Result<bool, ConfigError> {
    if held.is_empty() {
        return Ok(false);
    }
    let mut shared = live.ui.custom_themes.clone();
    held.add_to(&mut shared);
    let gained = shared.len() > live.ui.custom_themes.len();
    let mut global = GlobalConfig::from_profile(live, scope);
    global.custom_themes = Some(shared.clone());
    global.save(global_path)?;
    held.strip(&shared);
    live.ui.custom_themes = shared;
    Ok(gained)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::shared::strip_global_fields;
    use crate::profile::tests::{persist_live, styled_profile, theme, theme_ids};

    fn background(theme: &CustomTheme) -> &str {
        theme.xterm.get("background").map_or("", String::as_str)
    }

    /// Write a profile file that holds its own custom themes, the way a
    /// build before custom themes joined the theme scope saved it.
    fn write_profile_themes(set: &ProfileSet, name: &str, themes: Vec<CustomTheme>) {
        let mut config = ProfileConfig::default();
        config.ui.custom_themes = themes;
        config.save(&set.profile_path(name)).unwrap();
    }

    /// Mirror a launch or a switch. The active profile file loads first,
    /// then global.toml over it.
    fn load_live(set: &ProfileSet) -> Profile {
        let mut profile = Profile::default();
        let path = set.active_path();
        if path.exists() {
            ProfileConfig::load(&path).unwrap().apply_to(&mut profile);
        }
        if let Some(global) = GlobalConfig::load_shared(&set.global_path(), set.scope()).unwrap() {
            global.apply_to(&mut profile);
        }
        profile
    }

    #[test]
    fn the_shared_custom_themes_replace_the_profile_list() {
        // A list left in a profile file never comes back through a
        // switch. The shared list is the whole list.
        let global = GlobalConfig {
            custom_themes: Some(vec![theme("shared", "#101010")]),
            ..GlobalConfig::default()
        };
        let mut profile = Profile::default();
        profile.ui.custom_themes = vec![theme("shared", "#ffffff"), theme("deleted", "#202020")];
        global.apply_to(&mut profile);
        assert_eq!(theme_ids(&profile.ui.custom_themes), ["shared"]);
        assert_eq!(background(&profile.ui.custom_themes[0]), "#101010");
    }

    #[test]
    fn startup_moves_older_profile_themes_into_global_toml() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("alt").unwrap();
        GlobalConfig {
            theme: Some("nord".into()),
            custom_themes: Some(vec![theme("shared", "#101010")]),
            ..GlobalConfig::default()
        }
        .save(&set.global_path())
        .unwrap();
        write_profile_themes(
            &set,
            "default",
            vec![theme("shared", "#ffffff"), theme("mine", "#202020")],
        );
        write_profile_themes(
            &set,
            "alt",
            vec![theme("mine", "#303030"), theme("alts", "#404040")],
        );

        assert_eq!(migrate_custom_themes(&set).unwrap(), 2);

        let global = GlobalConfig::load(&set.global_path()).unwrap();
        assert_eq!(global.theme.as_deref(), Some("nord"));
        let shared = global.custom_themes.unwrap();
        // A different theme under a taken id joins under a fresh id, and
        // its label names its profile when the label is taken too.
        assert_eq!(
            theme_ids(&shared),
            ["shared", "shared-2", "mine", "mine-2", "alts"]
        );
        let backgrounds: Vec<&str> = shared.iter().map(background).collect();
        assert_eq!(
            backgrounds,
            ["#101010", "#ffffff", "#202020", "#303030", "#404040"]
        );
        assert_eq!(shared[1].label, "shared (Default)");
        assert_eq!(shared[3].label, "mine (alt)");
        for name in ["default", "alt"] {
            let file = ProfileConfig::load(&set.profile_path(name)).unwrap();
            assert!(file.ui.custom_themes.is_empty(), "{name} kept its list");
        }
        // The next launch finds nothing left to move.
        assert_eq!(migrate_custom_themes(&set).unwrap(), 0);
    }

    #[test]
    fn a_deleted_custom_theme_stays_deleted_after_a_switch() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("alt").unwrap();
        // Both files predate the shared list.
        write_profile_themes(&set, "default", vec![theme("keep", "#101010")]);
        write_profile_themes(&set, "alt", vec![theme("gone", "#202020")]);
        migrate_custom_themes(&set).unwrap();

        let mut live = load_live(&set);
        assert_eq!(theme_ids(&live.ui.custom_themes), ["keep", "gone"]);
        // You delete a theme on the default profile.
        live.ui.custom_themes.retain(|t| t.id != "gone");
        persist_live(&set, &live);

        set.switch("alt").unwrap();
        assert_eq!(theme_ids(&load_live(&set).ui.custom_themes), ["keep"]);
        // The next launch does not bring it back either.
        assert_eq!(migrate_custom_themes(&set).unwrap(), 0);
        assert_eq!(theme_ids(&load_live(&set).ui.custom_themes), ["keep"]);
    }

    #[test]
    fn profile_scoped_custom_themes_stay_in_their_files() {
        use crate::profile::shared::Scope as Kind;
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.set_scope(ScopeConfig {
            theme: Kind::Profile,
            ..ScopeConfig::default()
        })
        .unwrap();
        write_profile_themes(&set, "default", vec![theme("mine", "#101010")]);
        let global_before = std::fs::read_to_string(set.global_path()).unwrap();

        assert_eq!(migrate_custom_themes(&set).unwrap(), 0);
        let file = ProfileConfig::load(&set.profile_path("default")).unwrap();
        assert_eq!(theme_ids(&file.ui.custom_themes), ["mine"]);
        // The new install wrote global.toml, and the move leaves it alone.
        assert_eq!(
            std::fs::read_to_string(set.global_path()).unwrap(),
            global_before
        );
    }

    #[test]
    fn turning_the_theme_scope_global_keeps_every_profile_theme() {
        use crate::profile::shared::Scope as Kind;
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("alt").unwrap();
        set.set_scope(ScopeConfig {
            theme: Kind::Profile,
            ..ScopeConfig::default()
        })
        .unwrap();
        // Each profile saved its own list while the theme was per profile.
        let mut live = Profile::default();
        live.ui.custom_themes = vec![theme("mine", "#101010")];
        persist_live(&set, &live);
        write_profile_themes(
            &set,
            "alt",
            vec![theme("mine", "#ffffff"), theme("alts", "#202020")],
        );

        set.set_scope(ScopeConfig::default()).unwrap();
        let share = |set: &ProfileSet, live: &mut Profile| {
            let held = HeldCustomThemes::find(set, Some(set.active_name()));
            share_custom_themes(held, set.scope(), &set.global_path(), live).unwrap()
        };
        assert!(share(&set, &mut live));
        assert_eq!(
            theme_ids(&live.ui.custom_themes),
            ["mine", "mine-2", "alts"]
        );
        assert_eq!(background(&live.ui.custom_themes[0]), "#101010");
        assert_eq!(background(&live.ui.custom_themes[1]), "#ffffff");
        let global = GlobalConfig::load(&set.global_path()).unwrap();
        assert_eq!(
            theme_ids(&global.custom_themes.unwrap()),
            ["mine", "mine-2", "alts"]
        );
        let alt = ProfileConfig::load(&set.profile_path("alt")).unwrap();
        let leftover = &alt.ui.custom_themes;
        assert!(leftover.is_empty(), "{leftover:?}");

        // The persist that follows the scope change clears the active
        // file, and the other profile sees every theme.
        persist_live(&set, &live);
        set.switch("alt").unwrap();
        assert_eq!(
            theme_ids(&load_live(&set).ui.custom_themes),
            ["mine", "mine-2", "alts"]
        );
        // Nothing is left for a second pass.
        assert!(!share(&set, &mut live));
    }

    fn labeled(id: &str, label: &str, background: &str) -> CustomTheme {
        CustomTheme {
            label: label.into(),
            ..theme(id, background)
        }
    }

    /// A profile file that picked `id` as its theme, its dark theme and
    /// its day and night themes while it held `themes`.
    fn write_profile_pick(set: &ProfileSet, name: &str, id: &str, themes: Vec<CustomTheme>) {
        let mut config = ProfileConfig::default();
        config.ui.theme = id.into();
        config.ui.dark_theme = id.into();
        config.ui.day_theme = id.into();
        config.ui.night_theme = id.into();
        config.ui.custom_themes = themes;
        config.save(&set.profile_path(name)).unwrap();
    }

    #[test]
    fn startup_keeps_different_themes_that_two_profiles_saved_as_custom() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Healer").unwrap();
        set.create("Bard").unwrap();
        let ember = labeled("custom", "Ember variant", "#101010");
        let healer = labeled("custom", "Ember variant", "#202020");
        let bard = labeled("custom", "Night blue", "#303030");
        write_profile_pick(&set, "default", "custom", vec![ember.clone()]);
        write_profile_pick(&set, "Healer", "custom", vec![healer]);
        write_profile_pick(&set, "Bard", "custom", vec![bard]);

        assert_eq!(migrate_custom_themes(&set).unwrap(), 3);

        let shared = GlobalConfig::load(&set.global_path())
            .unwrap()
            .custom_themes
            .unwrap();
        assert_eq!(theme_ids(&shared), ["custom", "custom-2", "custom-3"]);
        assert_eq!(shared[0], ember);
        assert_eq!(background(&shared[1]), "#202020");
        assert_eq!(shared[1].label, "Ember variant (Healer)");
        // A label nobody else holds stays as it is.
        assert_eq!(background(&shared[2]), "#303030");
        assert_eq!(shared[2].label, "Night blue");

        // Each file now points at the id its own theme landed under.
        for (name, id) in [
            ("default", "custom"),
            ("Healer", "custom-2"),
            ("Bard", "custom-3"),
        ] {
            let file = ProfileConfig::load(&set.profile_path(name)).unwrap();
            assert!(file.ui.custom_themes.is_empty(), "{name} kept its list");
            assert_eq!(file.ui.theme, id, "{name}");
            assert_eq!(file.ui.dark_theme, id, "{name}");
            assert_eq!(file.ui.day_theme, id, "{name}");
            assert_eq!(file.ui.night_theme, id, "{name}");
        }
    }

    #[test]
    fn startup_merges_identical_themes_into_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Healer").unwrap();
        let ember = labeled("custom", "Ember variant", "#101010");
        write_profile_pick(&set, "default", "custom", vec![ember.clone()]);
        write_profile_pick(&set, "Healer", "custom", vec![ember.clone()]);

        assert_eq!(migrate_custom_themes(&set).unwrap(), 2);

        let shared = GlobalConfig::load(&set.global_path())
            .unwrap()
            .custom_themes
            .unwrap();
        assert_eq!(shared, [ember]);
        let healer = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
        assert_eq!(healer.ui.theme, "custom");
    }

    #[test]
    fn a_move_that_runs_again_adds_no_second_copy() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Healer").unwrap();
        let healer = labeled("custom", "Ember variant", "#202020");
        write_profile_pick(
            &set,
            "default",
            "custom",
            vec![labeled("custom", "Ember variant", "#101010")],
        );
        write_profile_pick(&set, "Healer", "custom", vec![healer.clone()]);
        migrate_custom_themes(&set).unwrap();
        let first = GlobalConfig::load(&set.global_path())
            .unwrap()
            .custom_themes
            .unwrap();

        // The Healer file failed to save last time and still holds its list.
        write_profile_pick(&set, "Healer", "custom", vec![healer]);
        assert_eq!(migrate_custom_themes(&set).unwrap(), 1);

        let again = GlobalConfig::load(&set.global_path())
            .unwrap()
            .custom_themes
            .unwrap();
        assert_eq!(again, first);
        let file = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
        assert_eq!(file.ui.theme, "custom-2");
    }

    #[test]
    fn a_list_the_shared_themes_lack_is_not_cleared() {
        let held = vec![theme("custom", "#202020")];
        let shared = vec![theme("custom", "#101010")];
        assert!(!holds_every_theme(&shared, &held, &["custom".into()]));
        assert!(!holds_every_theme(&shared, &held, &[]));
        let mut merged = shared.clone();
        let landed = merge_custom_themes(&mut merged, &held, "Healer");
        assert!(holds_every_theme(&merged, &held, &landed));
    }

    #[test]
    fn an_imported_theme_picked_globally_survives_a_profile_switch() {
        use crate::profile::set::ProfileSet;

        fn persist(set: &ProfileSet, profile: &Profile) {
            let mut snapshot = ProfileConfig::from_profile(profile);
            strip_global_fields(&mut snapshot, set.scope());
            snapshot.save(&set.active_path()).unwrap();
            GlobalConfig::from_profile(profile, set.scope())
                .save(&set.global_path())
                .unwrap();
        }
        fn load(set: &ProfileSet) -> Profile {
            let mut profile = Profile::default();
            let path = set.active_path();
            if path.exists() {
                ProfileConfig::load(&path).unwrap().apply_to(&mut profile);
            }
            GlobalConfig::load(&set.global_path())
                .unwrap()
                .apply_to(&mut profile);
            profile
        }

        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        persist(&set, &styled_profile());

        set.create("alt").unwrap();
        set.switch("alt").unwrap();
        let alt = load(&set);
        assert_eq!(alt.ui.theme, "night-ink");
        assert_eq!(alt.ui.custom_themes.len(), 1);
        assert_eq!(alt.ui.custom_themes[0].id, "night-ink");
        assert!(alt.ui.follow_system_appearance);
        assert_eq!(alt.ui.terminal_line_height, "loose");
    }
}
