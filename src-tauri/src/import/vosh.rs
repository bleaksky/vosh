//! A Vosh profile export, read for the import under Characters.
//! [`preview`] says what the file holds and where it
//! would go, and changes nothing. [`plan`] works out what the import
//! writes before anything is written, and [`apply`] writes it. Files from
//! other clients go through the importers beside this one, under
//! Automation.

pub(crate) mod apply;

use serde::Serialize;
use tracing::warn;
use vosh_automation::alias::Alias;
use vosh_automation::trigger::{Trigger, TriggerAction};

use crate::loadouts::catalog::GlobalCatalog;
use crate::loadouts::presets::PRESETS_OFF;
use crate::profile::export;
use crate::profile::file::ProfileConfig;
use crate::profile::live::Macro;
use crate::profile::login_match::AutoMatch;
use crate::profile::panes::leaf_panes;
use crate::profile::set::{sanitize_name, ProfileSet};
use crate::profile::shared::{strip_global_fields, ScopeConfig};
use crate::profile::worlds::world_name;
use crate::tick::TickConfig;

/// What an export holds and where it would go.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ImportPreview {
    /// The name the file name gives, or None when that name breaks the
    /// profile name rule, so you type one.
    pub name: Option<String>,
    /// Every trigger, room triggers included.
    pub triggers: usize,
    pub aliases: usize,
    pub macros: usize,
    pub timers: usize,
    /// Whether the `[tick]` table differs from the default.
    pub tick: bool,
    pub variables: usize,
    /// The pane types in tree order.
    pub panes: Vec<String>,
    /// The triggers with a script action, then the aliases with a script,
    /// which the import names under the warning Install gives.
    pub runs_lua: Vec<LuaItem>,
    /// The plugins the file turns on. An import brings each in off.
    pub plugins: Vec<String>,
    /// The world the `[vosh_export]` table names, if any.
    pub world: Option<ImportWorld>,
    /// The characters the table names. A character logs in only on a
    /// world, so a file with no world lists none.
    pub characters: Vec<ImportCharacter>,
    /// In loadout mode, whether the file holds presets, a list of them,
    /// their triggers or macros, or your edits to them, which stay out as
    /// the catalog's presets serve every character. The
    /// sheet then says the presets stay as the catalog has them.
    pub presets_stay: bool,
}

/// A trigger or an alias that runs Lua.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LuaItem {
    pub kind: LuaItemKind,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LuaItemKind {
    Trigger,
    Alias,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportWorld {
    pub host: String,
    pub port: Option<u16>,
    /// The name Vosh shows for the host, like The Forsaken Lands.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportCharacter {
    pub name: String,
    /// The profile that keeps the character on that world, or None while
    /// no profile lists it. See [`ProfileSet::claimant`].
    pub claimed_by: Option<String>,
}

/// What an import writes, planned from the export before anything is
/// written.
#[derive(Debug)]
pub(crate) struct ImportPlan {
    /// The profile file to write. Its `[plugins]` list is empty, since an
    /// import brings each plugin in off, so no code from the file runs
    /// before you turn it on, and the settings your scope
    /// shares sit at their defaults, so your shared theme and font stay.
    /// In loadout mode it holds no triggers, aliases, macros, alert
    /// presets, list of presets that are on or edits to them, which
    /// belong to the catalog.
    pub file: ProfileConfig,
    /// The world the `[vosh_export]` table names, with the characters it
    /// lists, or None when it names no world.
    pub claim: Option<AutoMatch>,
    /// What the catalog takes in loadout mode. None in per profile mode.
    pub catalog: Option<CatalogJoin>,
}

/// The triggers, aliases and macros of an export as the catalog takes
/// them in loadout mode. A profile file's own items lay over the
/// catalog at every launch, so they go to the catalog instead.
#[derive(Debug, PartialEq)]
pub(crate) struct CatalogJoin {
    /// The group each item joins, named for the file, like
    /// `Healer profile`.
    pub group: String,
    /// Your triggers of the file. Its preset triggers stay out, since a
    /// launch installs the catalog's own from its `enabled_presets`.
    pub triggers: Vec<Trigger>,
    pub aliases: Vec<Alias>,
    /// Your macros of the file. Its preset macros stay out, since a
    /// launch installs the catalog's own from its `enabled_presets`.
    pub macros: Vec<Macro>,
    /// The file's items the catalog already has, which it keeps.
    pub clashes: Vec<Clash>,
}

/// An item of the file that the catalog already holds, a trigger or an
/// alias by its name in the file's group, and a macro of yours by its
/// key. Yours stays and the file's is left out. A trigger or an alias
/// whose name the file holds in more than one group clashes too, past
/// the one its Settings listed first, since they all join one group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Clash {
    pub kind: ClashKind,
    /// The name, or the key of a macro.
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ClashKind {
    Trigger,
    Alias,
    Macro,
}

/// An export as the import reads it.
struct Export {
    /// The name the file name gives, see [`ImportPreview::name`].
    name: Option<String>,
    /// The `[vosh_export]` table as a login claim, cleaned.
    claim: Option<AutoMatch>,
    config: ProfileConfig,
}

/// Read `text`, the file you picked as `file_name`, as a Vosh profile
/// export. Every field of a profile file has a default, so any TOML file
/// reads as a profile. Only the `[vosh_export]` table, or the file name
/// Export to Downloads gave before it wrote the table, marks an export,
/// so catalog.toml or a plugin's manifest.toml is refused.
fn read_export(file_name: &str, text: &str) -> Result<Export, String> {
    let (name, export_name) = name_from_file(file_name);
    let unreadable = |e: &dyn std::fmt::Display| {
        warn!(error = %e, file = file_name, "profile import read failed");
        format!("Vosh could not read {file_name}.")
    };
    let table = match export::read(text) {
        Ok(Some(table)) => Some(table),
        Ok(None) if export_name => None,
        Err(e) if export_name => return Err(unreadable(&e)),
        _ => {
            return Err(format!(
                "{file_name} is not a Vosh profile export. Import other clients under Automation."
            ))
        }
    };
    let config = ProfileConfig::from_toml(text).map_err(|e| unreadable(&e))?;

    // The claim cleanup trims the host and drops blank or repeated names
    // a hand edit may leave in the table.
    let claim = table.map(|t| {
        AutoMatch {
            host: t.host,
            port: t.port,
            characters: t.characters,
            enabled: true,
        }
        .cleaned()
    });
    Ok(Export {
        name,
        claim,
        config,
    })
}

/// Say what the export `text`, the file you picked as `file_name`, holds
/// and where it would go, in loadout mode when `loadout_mode`. A file
/// that is no export is refused, see [`read_export`].
pub(crate) fn preview(
    set: &ProfileSet,
    file_name: &str,
    text: &str,
    loadout_mode: bool,
) -> Result<ImportPreview, String> {
    let Export {
        name,
        claim,
        config,
    } = read_export(file_name, text)?;
    let world = claim.as_ref().and_then(|am| {
        let host = am.host.clone()?;
        Some(ImportWorld {
            name: world_name(&host),
            host,
            port: am.port,
        })
    });
    let characters = match (&world, claim) {
        (Some(world), Some(am)) => am
            .characters
            .into_iter()
            .map(|name| ImportCharacter {
                claimed_by: set
                    .claimant(&world.host, world.port, &name)
                    .map(str::to_string),
                name,
            })
            .collect(),
        _ => Vec::new(),
    };

    let lua_triggers = config
        .triggers
        .iter()
        .filter(|t| {
            t.actions
                .iter()
                .any(|a| matches!(a, TriggerAction::Script { .. }))
        })
        .map(|t| LuaItem {
            kind: LuaItemKind::Trigger,
            name: t.name.clone(),
        });
    let lua_aliases = config
        .aliases
        .iter()
        .filter(|a| a.script.is_some())
        .map(|a| LuaItem {
            kind: LuaItemKind::Alias,
            name: a.name.clone(),
        });
    let runs_lua = lua_triggers.chain(lua_aliases).collect();
    let presets_stay = loadout_mode && holds_presets(&config);

    Ok(ImportPreview {
        name,
        triggers: config.triggers.len(),
        aliases: config.aliases.len(),
        macros: config.macros.len(),
        timers: config.timers.len(),
        tick: config.tick != TickConfig::default(),
        variables: config.profile_vars.len(),
        panes: leaf_panes(&config.ui.pane_layout().root),
        runs_lua,
        plugins: config.plugins.enabled,
        world,
        characters,
        presets_stay,
    })
}

/// Whether `file` turns a preset on, an empty list turning the defaults
/// on, or holds a preset trigger, a preset macro or an edit to a preset.
fn holds_presets(file: &ProfileConfig) -> bool {
    !file.ui.enabled_presets.iter().any(|id| id == PRESETS_OFF)
        || !file.preset_edits.is_empty()
        || file.triggers.iter().any(|t| t.preset.is_some())
        || file.macros.iter().any(|m| m.preset.is_some())
}

/// Plan the import of the export `text`, the file you picked as
/// `file_name`, before anything is written. `scope` is what your profiles
/// share, and `catalog` the catalog as it stands in loadout mode, None in
/// per profile mode. A file that is no export is refused, see
/// [`read_export`].
///
/// The `[plugins]` list empties. The settings `scope` shares go back to
/// their defaults, as every save writes them, so your shared theme and
/// font stay. A scope that shares nothing keeps the file's own. In
/// loadout mode the file's triggers, aliases and macros move to the
/// catalog in a group named for the file, and the presets the catalog
/// keeps stay as they are: its list, your edits to them and the alert
/// presets, since they serve every character.
pub(crate) fn plan(
    file_name: &str,
    text: &str,
    scope: &ScopeConfig,
    catalog: Option<&GlobalCatalog>,
) -> Result<ImportPlan, String> {
    let Export {
        claim, mut config, ..
    } = read_export(file_name, text)?;
    config.plugins.enabled.clear();
    strip_global_fields(&mut config, scope);
    let catalog = catalog.map(|catalog| {
        let group = file_name.strip_suffix(".toml").unwrap_or(file_name);
        let join = join_catalog(&config, catalog, group);
        config.clear_catalog_items();
        // A list that turns no preset on, which a catalog that has yet to
        // take a list reads as nothing to add.
        config.ui.enabled_presets = vec![PRESETS_OFF.to_string()];
        join
    });
    Ok(ImportPlan {
        file: config,
        claim: claim.filter(|am| am.host.is_some()),
        catalog,
    })
}

/// The triggers, aliases and macros of `file` that `catalog` lacks, each
/// moved into `group`, with a clash for each one it has.
fn join_catalog(file: &ProfileConfig, catalog: &GlobalCatalog, group: &str) -> CatalogJoin {
    let mut clashes = Vec::new();
    // The catalog's preset triggers serve every character, so the file's
    // stay out, or the catalog would file them as yours.
    let yours: Vec<Trigger> = file
        .triggers
        .iter()
        .filter(|t| t.preset.is_none())
        .cloned()
        .collect();
    let triggers = join_grouped(&yours, &catalog.triggers, group, &mut clashes);
    let aliases = join_grouped(&file.aliases, &catalog.aliases, group, &mut clashes);
    // A launch installs the catalog's own preset macros from its
    // enabled_presets, so the file's stay out. The file's macros meet only
    // yours in the catalog. A preset macro there is held off by one of
    // yours on its key, so it is no clash, and a catalog with Numpad movement on
    // lists none on its keys.
    let yours = |macros: &[Macro]| -> Vec<Macro> {
        macros
            .iter()
            .filter(|m| m.preset.is_none())
            .cloned()
            .collect()
    };
    let macros = join(
        ClashKind::Macro,
        &yours(&file.macros),
        &yours(&catalog.macros),
        |m| &m.key,
        |m| m.group = Some(group.to_string()),
        &mut clashes,
    );
    CatalogJoin {
        group: group.to_string(),
        triggers,
        aliases,
        macros,
        clashes,
    }
}

/// The items of `file` whose `key` no item of `kept` has, each changed by
/// `regroup`. Each one `kept` has adds a clash of `kind` to `clashes`.
fn join<T: Clone>(
    kind: ClashKind,
    file: &[T],
    kept: &[T],
    key: impl Fn(&T) -> &String,
    regroup: impl Fn(&mut T),
    clashes: &mut Vec<Clash>,
) -> Vec<T> {
    let mut joined = Vec::new();
    for item in file {
        let name = key(item);
        if kept.iter().any(|mine| key(mine) == name) {
            clashes.push(Clash {
                kind,
                name: name.clone(),
            });
            continue;
        }
        let mut item = item.clone();
        regroup(&mut item);
        joined.push(item);
    }
    joined
}

/// A trigger or an alias, which the stores know by its group and its
/// name.
trait Grouped: Clone {
    const KIND: ClashKind;
    fn id(&self) -> (Option<&str>, &str);
    /// Give the item `name` and put it in `group`.
    fn regroup(&mut self, name: &str, group: &str);
}

impl Grouped for Alias {
    const KIND: ClashKind = ClashKind::Alias;
    fn id(&self) -> (Option<&str>, &str) {
        Alias::id(self)
    }
    fn regroup(&mut self, name: &str, group: &str) {
        self.name = name.to_string();
        self.group = Some(group.to_string());
    }
}

impl Grouped for Trigger {
    const KIND: ClashKind = ClashKind::Trigger;
    fn id(&self) -> (Option<&str>, &str) {
        Trigger::id(self)
    }
    fn regroup(&mut self, name: &str, group: &str) {
        self.name = name.to_string();
        self.group = Some(group.to_string());
    }
}

/// The triggers or aliases of `file`, each moved into `group`, that
/// `kept` lacks there. Each is known by its group and its name, so one
/// of a name the catalog keeps in another group joins beside it. When
/// the file holds a name in more than one group, the one its Settings
/// listed first joins, since they would all land in `group`, and each
/// other adds a clash, as does each the catalog already has in `group`.
/// For an alias that is also the one that fired.
fn join_grouped<T: Grouped>(
    file: &[T],
    kept: &[T],
    group: &str,
    clashes: &mut Vec<Clash>,
) -> Vec<T> {
    let mut joined: Vec<T> = Vec::new();
    for (at, item) in file.iter().enumerate() {
        let (group_here, name) = item.id();
        // Another of the name the file lists first, in Settings order.
        let listed_before = file.iter().enumerate().any(|(other, b)| {
            other != at
                && b.id().1 == name
                && vosh_automation::compare_groups(b.id().0, group_here)
                    .then(other.cmp(&at))
                    .is_lt()
        });
        let kept_here = kept.iter().any(|mine| mine.id() == (Some(group), name));
        if listed_before || kept_here {
            clashes.push(Clash {
                kind: T::KIND,
                name: name.to_string(),
            });
            continue;
        }
        let mut item = item.clone();
        item.regroup(name, group);
        joined.push(item);
    }
    joined
}

/// The profile name a file name gives, and whether the file name is one
/// Export to Downloads gives. The name is the text before ` profile`,
/// with the ` (n)` a second export to the same folder adds dropped, so
/// `Healer profile (2).toml` reads as Healer.
fn name_from_file(file_name: &str) -> (Option<String>, bool) {
    let stem = file_name.strip_suffix(".toml");
    let base = crate::disk::paths::drop_copy_number(stem.unwrap_or(file_name));
    let (base, profile) = match base.strip_suffix(" profile") {
        Some(base) => (base, true),
        None => (base, false),
    };
    (sanitize_name(base).ok(), stem.is_some() && profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::tests::{claim, set_with_profiles};
    use crate::profile::text_size::TextPx;

    const WORLD: &str = "play.theforsakenlands.com";
    const FULL: &str = include_str!("../../../fixtures/config/profile.full.toml");
    const EXPORT: &str = include_str!("../../../fixtures/config/export.full.toml");

    fn lua(kind: LuaItemKind, name: &str) -> LuaItem {
        LuaItem {
            kind,
            name: name.into(),
        }
    }

    #[test]
    fn the_full_profile_previews_what_it_holds() {
        let set = set_with_profiles(vec![]);
        let view = preview(&set, "Healer profile.toml", FULL, false).unwrap();
        assert_eq!(
            view,
            ImportPreview {
                name: Some("Healer".into()),
                triggers: 3,
                aliases: 2,
                macros: 5,
                timers: 1,
                tick: true,
                variables: 2,
                panes: vec!["map".into(), "chat".into(), "group".into()],
                runs_lua: vec![
                    lua(LuaItemKind::Trigger, "tells"),
                    lua(LuaItemKind::Alias, "heal")
                ],
                plugins: vec!["vitals_alert".into()],
                world: None,
                characters: Vec::new(),
                presets_stay: false,
            }
        );
    }

    /// In loadout mode the sheet says the presets stay as the catalog has
    /// them, when the file holds any.
    #[test]
    fn a_loadout_preview_flags_the_presets_that_stay_out() {
        let set = set_with_profiles(vec![]);
        let view = preview(&set, "Healer profile.toml", EXPORT, true).unwrap();
        assert!(view.presets_stay);
        let view = preview(&set, "Healer profile.toml", EXPORT, false).unwrap();
        assert!(!view.presets_stay);
        // A file with every preset off has nothing to leave out.
        let mut off = ProfileConfig::default();
        off.ui.enabled_presets = vec![PRESETS_OFF.into()];
        let off = off.to_toml().unwrap();
        let view = preview(&set, "Default profile.toml", &off, true).unwrap();
        assert!(!view.presets_stay);
    }

    #[test]
    fn a_character_another_profile_lists_reads_as_claimed_by_it() {
        let set = set_with_profiles(vec![("Healer", claim(WORLD, Some(1848), &["Orla"]))]);
        let view = preview(&set, "Healer profile.toml", EXPORT, false).unwrap();
        assert_eq!(
            view.world,
            Some(ImportWorld {
                host: WORLD.into(),
                port: Some(1848),
                name: "The Forsaken Lands".into(),
            })
        );
        assert_eq!(
            view.characters,
            [ImportCharacter {
                name: "Orla".into(),
                claimed_by: Some("Healer".into()),
            }]
        );

        // Maren is on no list, and a blank or repeated name drops out.
        let table = "\n[vosh_export]\nhost = \" play.theforsakenlands.com \"\n\
                     characters = [\"Orla\", \" \", \"Maren\", \"orla\"]\n";
        let view = preview(&set, "shared.toml", &format!("{FULL}{table}"), false).unwrap();
        assert_eq!(view.name.as_deref(), Some("shared"));
        assert_eq!(
            view.world.as_ref().map(|w| (w.host.as_str(), w.port)),
            Some((WORLD, None))
        );
        let claims: Vec<_> = view
            .characters
            .iter()
            .map(|c| (c.name.as_str(), c.claimed_by.as_deref()))
            .collect();
        assert_eq!(claims, [("Orla", Some("Healer")), ("Maren", None)]);
    }

    #[test]
    fn an_export_without_the_table_reads_by_its_file_name() {
        let set = set_with_profiles(vec![]);
        let view = preview(&set, "Healer profile (2).toml", FULL, false).unwrap();
        assert_eq!(view.name.as_deref(), Some("Healer"));
        assert_eq!(view.triggers, 3);
        assert_eq!(view.world, None);

        // A profile at its defaults keeps the default tick and panes.
        let blank = ProfileConfig::default().to_toml().unwrap();
        let view = preview(&set, "Default profile.toml", &blank, false).unwrap();
        assert_eq!(view.name.as_deref(), Some("Default"));
        assert!(!view.tick);
        assert_eq!(view.panes, ["map", "affects"]);
        assert!(view.runs_lua.is_empty() && view.plugins.is_empty());

        // A name the profile rule refuses leaves the name for you to type.
        let view = preview(&set, "Healer (old) profile.toml", FULL, false).unwrap();
        assert_eq!(view.name, None);
    }

    #[test]
    fn a_file_that_is_not_an_export_is_refused() {
        let set = set_with_profiles(vec![]);
        let catalog = include_str!("../../../fixtures/config/catalog.full.toml");
        let manifest = include_str!("../../../plugins/vitals_alert/manifest.toml");
        assert_eq!(
            preview(&set, "catalog.toml", catalog, false),
            Err(
                "catalog.toml is not a Vosh profile export. Import other clients under Automation."
                    .into()
            )
        );
        for (file, text) in [
            ("manifest.toml", manifest),
            ("profile.toml", FULL),
            ("Healer profile.txt", FULL),
            ("notes.toml", "not = [toml"),
        ] {
            assert_eq!(
                preview(&set, file, text, false),
                Err(format!(
                    "{file} is not a Vosh profile export. Import other clients under Automation."
                ))
            );
        }
        // An export name over a file Vosh cannot read says so.
        assert_eq!(
            preview(&set, "Healer profile.toml", "not = [toml", false),
            Err("Vosh could not read Healer profile.toml.".into())
        );
    }

    #[test]
    fn the_name_drops_the_copy_number_and_the_word_profile() {
        assert_eq!(
            name_from_file("Healer profile (2).toml"),
            (Some("Healer".into()), true)
        );
        assert_eq!(
            name_from_file("Default profile.toml"),
            (Some("Default".into()), true)
        );
        assert_eq!(
            name_from_file("Tank 2 profile (12).toml"),
            (Some("Tank 2".into()), true)
        );
        assert_eq!(
            name_from_file("Healer (2).toml"),
            (Some("Healer".into()), false)
        );
        assert_eq!(name_from_file("Healer (b).toml"), (None, false));
        assert_eq!(
            name_from_file("profile.toml"),
            (Some("profile".into()), false)
        );
        assert_eq!(name_from_file(" profile.toml"), (None, true));
    }

    /// A scope that keeps every category per profile, so it shares
    /// nothing.
    fn shares_nothing() -> ScopeConfig {
        use crate::profile::shared::Scope;
        ScopeConfig {
            theme: Scope::Profile,
            font: Scope::Profile,
            dock_layout: Scope::Profile,
            keep_last_command: Scope::Profile,
            auto_update: Scope::Profile,
        }
    }

    fn names<T>(items: &[T], key: impl Fn(&T) -> &String) -> Vec<&str> {
        items.iter().map(|item| key(item).as_str()).collect()
    }

    #[test]
    fn a_plan_turns_every_plugin_off_and_keeps_the_rest_in_per_profile_mode() {
        let plan = plan("Healer profile.toml", EXPORT, &ScopeConfig::default(), None).unwrap();
        let plugins = &plan.file.plugins.enabled;
        assert!(plugins.is_empty(), "{plugins:?}");
        assert!(plan.catalog.is_none());
        assert_eq!(
            names(&plan.file.triggers, |t| &t.name),
            ["tells", "spam", "room-items"]
        );
        assert_eq!(names(&plan.file.aliases, |a| &a.name), ["kk", "heal"]);
        assert_eq!(
            names(&plan.file.macros, |m| &m.key),
            ["F1", "F2", "Numpad3", "Numpad8", "Numpad3"]
        );
        assert_eq!(plan.file.timers.len(), 1);
        assert_eq!(plan.file.alerts.len(), 2);
        // The list of presets that are on and your edits come whole.
        let exported = ProfileConfig::from_toml(EXPORT).unwrap();
        assert!(!exported.preset_edits.is_empty());
        assert_eq!(plan.file.ui.enabled_presets, exported.ui.enabled_presets);
        assert_eq!(plan.file.preset_edits, exported.preset_edits);
        let am = plan.claim.unwrap();
        assert_eq!(
            (am.host.as_deref(), am.port, am.characters, am.enabled),
            (Some(WORLD), Some(1848), vec!["Orla".to_string()], true)
        );
        // A file with no table names no world.
        let plan = plan_of("Healer profile.toml", FULL);
        assert!(plan.claim.is_none());
    }

    fn plan_of(file_name: &str, text: &str) -> ImportPlan {
        plan(file_name, text, &ScopeConfig::default(), None).unwrap()
    }

    #[test]
    fn your_shared_theme_and_font_stay_unless_your_profiles_share_nothing() {
        use crate::profile::ui::UiConfig;
        let defaults = UiConfig::default();
        let shared = plan_of("Healer profile.toml", FULL).file.ui;
        assert_eq!(shared.theme, defaults.theme);
        assert_eq!(shared.font_family, defaults.font_family);
        assert_eq!(shared.font_size, defaults.font_size);
        assert!(
            shared.custom_themes.is_empty(),
            "{:?}",
            shared.custom_themes
        );
        // What no scope shares comes with the file.
        assert_eq!(shared.tracked_affects.len(), 2);

        let own = plan("Healer profile.toml", FULL, &shares_nothing(), None)
            .unwrap()
            .file
            .ui;
        assert_eq!(own.theme, "custom-dusk");
        assert_eq!(own.font_size, TextPx::whole(16));
        assert_eq!(own.custom_themes.len(), 1);
    }

    #[test]
    fn in_loadout_mode_the_items_join_the_catalog_in_a_group_named_for_the_file() {
        let mut catalog = GlobalCatalog::default();
        catalog.aliases.push(Alias::new("kk", "kick"));
        catalog
            .triggers
            .push(Trigger::new("spam", "^spam$", TriggerAction::Gag));
        catalog.macros.push(Macro {
            key: "F2".into(),
            command: "rest".into(),
            group: None,
            enabled: true,
            preset: None,
        });
        let plan = plan(
            "Healer profile (2).toml",
            EXPORT,
            &ScopeConfig::default(),
            Some(&catalog),
        )
        .unwrap();

        // The file keeps everything else, and never the catalog's items.
        let file = &plan.file;
        assert!(file.triggers.is_empty() && file.aliases.is_empty() && file.macros.is_empty());
        assert!(file.alerts.is_empty(), "{:?}", file.alerts);
        // The presets stay as the catalog has them.
        assert_eq!(file.ui.enabled_presets, [PRESETS_OFF]);
        assert!(file.preset_edits.is_empty(), "{:?}", file.preset_edits);
        assert_eq!(file.timers.len(), 1);
        assert_eq!(file.profile_vars.len(), 2);

        let join = plan.catalog.unwrap();
        assert_eq!(join.group, "Healer profile (2)");
        // The preset trigger tells stays out with its preset. A trigger is
        // known by its group and its name, so the file's spam joins beside
        // the catalog's spam in no group.
        assert_eq!(names(&join.triggers, |t| &t.name), ["spam", "room-items"]);
        // An alias is known by its group and its name, so the file's kk
        // joins beside the catalog's kk in no group.
        assert_eq!(names(&join.aliases, |a| &a.name), ["kk", "heal"]);
        // The file's preset macros stay out.
        assert_eq!(names(&join.macros, |m| &m.key), ["F1", "Numpad3"]);
        let groups: Vec<_> = join
            .triggers
            .iter()
            .map(|t| t.group.as_deref())
            .chain(join.aliases.iter().map(|a| a.group.as_deref()))
            .chain(join.macros.iter().map(|m| m.group.as_deref()))
            .collect();
        assert_eq!(groups, [Some("Healer profile (2)"); 6]);
        // A clash keeps yours, here a macro by key.
        let clash = |kind, name: &str| Clash {
            kind,
            name: name.into(),
        };
        assert_eq!(join.clashes, [clash(ClashKind::Macro, "F2")]);
        // A trigger the catalog has in that group clashes.
        catalog.triggers[0].group = Some("Healer profile (2)".into());
        let again = super::plan(
            "Healer profile (2).toml",
            EXPORT,
            &ScopeConfig::default(),
            Some(&catalog),
        )
        .unwrap();
        let join = again.catalog.unwrap();
        assert_eq!(names(&join.triggers, |t| &t.name), ["room-items"]);
        assert_eq!(join.clashes[0], clash(ClashKind::Trigger, "spam"));
    }

    #[test]
    fn a_trigger_name_two_groups_share_joins_once_and_a_per_profile_file_keeps_both() {
        let in_group = |name: &str, command: &str, group: &str| Trigger {
            group: Some(group.into()),
            ..Trigger::new(
                name,
                "^Orla arrives",
                TriggerAction::Send {
                    template: command.into(),
                },
            )
        };
        let file = ProfileConfig {
            triggers: vec![
                in_group("greet", "bow orla", "Tolliver"),
                in_group("greet", "wave orla", "Maren"),
                in_group("flee", "flee", "Tolliver"),
                in_group("hp", "quaff", "Maren"),
            ],
            ..ProfileConfig::default()
        };
        // The catalog has hp in the group the file joins, and greet in
        // another.
        let catalog = GlobalCatalog {
            triggers: vec![
                in_group("hp", "quaff red", "Orla"),
                in_group("greet", "nod", "Tolliver"),
            ],
            ..GlobalCatalog::default()
        };
        let mut clashes = Vec::new();
        let joined = join_grouped(&file.triggers, &catalog.triggers, "Orla", &mut clashes);
        // Maren's greet, which Settings listed first, joins.
        let got: Vec<(&str, Option<&str>)> = joined
            .iter()
            .map(|t| (t.name.as_str(), t.group.as_deref()))
            .collect();
        assert_eq!(got, [("greet", Some("Orla")), ("flee", Some("Orla"))]);
        assert!(matches!(
            &joined[0].actions[0],
            TriggerAction::Send { template } if template == "wave orla"
        ));
        let clash = |name: &str| Clash {
            kind: ClashKind::Trigger,
            name: name.into(),
        };
        assert_eq!(clashes, [clash("greet"), clash("hp")]);

        // In per profile mode the file loads as it is, both kept.
        let mut p = crate::profile::live::Profile::default();
        file.apply_to(&mut p);
        assert_eq!(p.triggers.named("greet").len(), 2);
        assert!(p.triggers.get_in(Some("Tolliver"), "greet").is_some());
    }

    #[test]
    fn an_alias_name_two_groups_share_joins_once_and_a_per_profile_file_keeps_both() {
        let in_group = |name: &str, expansion: &str, group: &str| {
            let mut alias = Alias::new(name, expansion);
            alias.group = Some(group.into());
            alias
        };
        let file = ProfileConfig {
            aliases: vec![
                in_group("ds", "cast 'detect scry' tolliver", "Tolliver"),
                in_group("ds", "cast 'detect scry' maren", "Maren"),
                in_group("res", "cast resurrect", "Tolliver"),
                in_group("hl", "cast heal", "Maren"),
            ],
            ..ProfileConfig::default()
        };
        // The catalog has hl in the group the file joins, and ds in another.
        let catalog = GlobalCatalog {
            aliases: vec![
                in_group("hl", "cast 'cure light'", "Orla"),
                in_group("ds", "cast 'detect scry'", "Tolliver"),
            ],
            ..GlobalCatalog::default()
        };
        let mut clashes = Vec::new();
        let joined = join_grouped(&file.aliases, &catalog.aliases, "Orla", &mut clashes);
        // Maren's ds, which Settings listed first and so fired, joins.
        let got: Vec<(&str, &str, Option<&str>)> = joined
            .iter()
            .map(|a| (a.name.as_str(), a.expansion.as_str(), a.group.as_deref()))
            .collect();
        assert_eq!(
            got,
            [
                ("ds", "cast 'detect scry' maren", Some("Orla")),
                ("res", "cast resurrect", Some("Orla")),
            ]
        );
        let clash = |name: &str| Clash {
            kind: ClashKind::Alias,
            name: name.into(),
        };
        assert_eq!(clashes, [clash("ds"), clash("hl")]);

        // In per profile mode the file loads as it is, both kept.
        let mut p = crate::profile::live::Profile::default();
        file.apply_to(&mut p);
        assert_eq!(p.aliases.named("ds").len(), 2);
        assert_eq!(
            p.aliases.get_in(Some("Tolliver"), "ds").unwrap().expansion,
            "cast 'detect scry' tolliver"
        );
    }

    #[test]
    fn in_loadout_mode_the_macros_of_the_file_meet_only_yours() {
        // The catalog has your F2, and Numpad movement on, two of its
        // macros for short.
        let bind = |key: &str, command: &str, preset: Option<&str>| Macro {
            key: key.into(),
            command: command.into(),
            group: None,
            enabled: true,
            preset: preset.map(String::from),
        };
        let numpad = Some("numpad_movement");
        let catalog = GlobalCatalog {
            macros: vec![
                bind("F2", "rest", None),
                bind("Numpad8", "n", numpad),
                bind("Numpad3", "d", numpad),
            ],
            enabled_presets: Some(vec!["numpad_movement".into()]),
            ..GlobalCatalog::default()
        };
        let join = plan(
            "Healer profile.toml",
            EXPORT,
            &ScopeConfig::default(),
            Some(&catalog),
        )
        .unwrap()
        .catalog
        .unwrap();
        // Your Numpad3 joins beside the preset's d, and the file's own
        // preset macros stay out, as a launch installs the catalog's.
        assert_eq!(names(&join.macros, |m| &m.key), ["F1", "Numpad3"]);
        assert!(join.macros.iter().all(|m| m.preset.is_none()));
        // Only your F2 clashes. Each key of the preset used to as well.
        assert_eq!(
            join.clashes,
            [Clash {
                kind: ClashKind::Macro,
                name: "F2".into(),
            }]
        );
    }

    #[test]
    fn a_plan_refuses_what_the_preview_refuses() {
        let catalog = include_str!("../../../fixtures/config/catalog.full.toml");
        assert_eq!(
            plan("catalog.toml", catalog, &ScopeConfig::default(), None).unwrap_err(),
            "catalog.toml is not a Vosh profile export. Import other clients under Automation."
        );
        assert_eq!(
            plan(
                "Healer profile.toml",
                "not = [toml",
                &ScopeConfig::default(),
                None
            )
            .unwrap_err(),
            "Vosh could not read Healer profile.toml."
        );
    }
}
