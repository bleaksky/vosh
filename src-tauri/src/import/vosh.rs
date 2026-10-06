//! A Vosh profile export, read for the import under Characters (Scripts
//! Q9 and Q10). [`preview`] says what the file holds and where it would
//! go, and changes nothing. Files from other clients go through the
//! importers beside this one, under Automation.

use serde::Serialize;
use tracing::warn;
use vosh_automation::trigger::TriggerAction;

use crate::profile::export;
use crate::profile::file::ProfileConfig;
use crate::profile::login_match::AutoMatch;
use crate::profile::panes::leaf_panes;
use crate::profile::set::{sanitize_name, ProfileSet};
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

/// Read `text`, the file you picked as `file_name`, as a Vosh profile
/// export. Every field of a profile file has a default, so any TOML file
/// reads as a profile. Only the `[vosh_export]` table, or the file name
/// Export to Downloads gave before it wrote the table, marks an export,
/// so catalog.toml or a plugin's manifest.toml is refused.
pub(crate) fn preview(
    set: &ProfileSet,
    file_name: &str,
    text: &str,
) -> Result<ImportPreview, String> {
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
    })
}

/// The profile name a file name gives, and whether the file name is one
/// Export to Downloads gives. The name is the text before ` profile`,
/// with the ` (n)` a second export to the same folder adds dropped, so
/// `Healer profile (2).toml` reads as Healer.
fn name_from_file(file_name: &str) -> (Option<String>, bool) {
    let stem = file_name.strip_suffix(".toml");
    let base = drop_copy_number(stem.unwrap_or(file_name));
    let (base, profile) = match base.strip_suffix(" profile") {
        Some(base) => (base, true),
        None => (base, false),
    };
    (sanitize_name(base).ok(), stem.is_some() && profile)
}

/// `stem` without a trailing ` (n)`.
fn drop_copy_number(stem: &str) -> &str {
    let Some(inner) = stem.strip_suffix(')') else {
        return stem;
    };
    let Some(at) = inner.rfind(" (") else {
        return stem;
    };
    let digits = &inner[at + 2..];
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        &stem[..at]
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::tests::{claim, set_with_profiles};

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
        let view = preview(&set, "Healer profile.toml", FULL).unwrap();
        assert_eq!(
            view,
            ImportPreview {
                name: Some("Healer".into()),
                triggers: 3,
                aliases: 2,
                macros: 2,
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
            }
        );
    }

    #[test]
    fn a_character_another_profile_lists_reads_as_claimed_by_it() {
        let set = set_with_profiles(vec![("Healer", claim(WORLD, Some(1848), &["Orla"]))]);
        let view = preview(&set, "Healer profile.toml", EXPORT).unwrap();
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
        let view = preview(&set, "shared.toml", &format!("{FULL}{table}")).unwrap();
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
        let view = preview(&set, "Healer profile (2).toml", FULL).unwrap();
        assert_eq!(view.name.as_deref(), Some("Healer"));
        assert_eq!(view.triggers, 3);
        assert_eq!(view.world, None);

        // A profile at its defaults keeps the default tick and panes.
        let blank = ProfileConfig::default().to_toml().unwrap();
        let view = preview(&set, "Default profile.toml", &blank).unwrap();
        assert_eq!(view.name.as_deref(), Some("Default"));
        assert!(!view.tick);
        assert_eq!(view.panes, ["map", "affects"]);
        assert!(view.runs_lua.is_empty() && view.plugins.is_empty());

        // A name the profile rule refuses leaves the name for you to type.
        let view = preview(&set, "Healer (old) profile.toml", FULL).unwrap();
        assert_eq!(view.name, None);
    }

    #[test]
    fn a_file_that_is_not_an_export_is_refused() {
        let set = set_with_profiles(vec![]);
        let catalog = include_str!("../../../fixtures/config/catalog.full.toml");
        let manifest = include_str!("../../../plugins/vitals_alert/manifest.toml");
        assert_eq!(
            preview(&set, "catalog.toml", catalog),
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
                preview(&set, file, text),
                Err(format!(
                    "{file} is not a Vosh profile export. Import other clients under Automation."
                ))
            );
        }
        // An export name over a file Vosh cannot read says so.
        assert_eq!(
            preview(&set, "Healer profile.toml", "not = [toml"),
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
}
