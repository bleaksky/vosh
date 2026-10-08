//! The `[vosh_export]` table Export to Downloads writes after a profile's
//! settings. It names the world the profile plays and the
//! characters you tick, so an import can take them, and its presence
//! tells an export from any other TOML file. A profile file skips keys it
//! does not know, so this build and every older one read an export as
//! the profile it holds.

use serde::{Deserialize, Serialize};

use crate::profile::login_match::AutoMatch;

/// The table. Each key is left out while it says nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct VoshExport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<String>,
}

#[derive(Serialize)]
struct Written<'a> {
    vosh_export: &'a VoshExport,
}

#[derive(Deserialize)]
struct Read {
    vosh_export: Option<VoshExport>,
}

impl VoshExport {
    /// The table for a profile whose login claim is `claim`: its world,
    /// and the characters it lists that you ticked, in its order. A
    /// character logs in only on a world, so a claim with no host writes
    /// an empty table.
    pub(crate) fn new(claim: Option<&AutoMatch>, ticked: &[String]) -> Self {
        let Some(am) = claim.filter(|am| am.host.as_deref().is_some_and(|h| !h.trim().is_empty()))
        else {
            return Self::default();
        };
        let is_ticked = |name: &&String| {
            ticked
                .iter()
                .any(|t| t.trim().eq_ignore_ascii_case(name.trim()))
        };
        Self {
            host: am.host.clone(),
            port: am.port,
            characters: am.characters.iter().filter(is_ticked).cloned().collect(),
        }
    }

    /// `profile`, a profile file as `ProfileConfig::to_toml` writes it,
    /// with the table after it.
    pub(crate) fn write(&self, profile: &str) -> Result<String, toml::ser::Error> {
        let table = toml::to_string_pretty(&Written { vosh_export: self })?;
        Ok(format!("{profile}\n{table}"))
    }
}

/// The table in `text`, or None when the file has none. Fails when `text`
/// is not TOML or its table is not one Vosh writes.
pub(crate) fn read(text: &str) -> Result<Option<VoshExport>, toml::de::Error> {
    Ok(toml::from_str::<Read>(text)?.vosh_export)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::file::ProfileConfig;
    use crate::profile::tests::claim;

    const WORLD: &str = "play.theforsakenlands.com";

    fn ticked(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn the_table_names_the_ticked_characters_the_profile_claims_in_its_order() {
        let am = claim(WORLD, Some(1848), &["Maren", "Orla"]);
        let table = VoshExport::new(Some(&am), &ticked(&["orla", "Tolliver", "Maren"]));
        assert_eq!(
            table,
            VoshExport {
                host: Some(WORLD.into()),
                port: Some(1848),
                characters: ticked(&["Maren", "Orla"]),
            }
        );
        // Nothing ticked names the world alone.
        let table = VoshExport::new(Some(&am), &[]);
        assert_eq!(table.characters, Vec::<String>::new());
        assert_eq!(table.host.as_deref(), Some(WORLD));
    }

    #[test]
    fn a_profile_with_no_world_writes_an_empty_table() {
        let mut am = claim(WORLD, Some(1848), &["Orla"]);
        am.host = None;
        assert_eq!(
            VoshExport::new(Some(&am), &ticked(&["Orla"])),
            VoshExport::default()
        );
        assert_eq!(
            VoshExport::new(None, &ticked(&["Orla"])),
            VoshExport::default()
        );

        let profile = ProfileConfig::default().to_toml().unwrap();
        let text = VoshExport::default().write(&profile).unwrap();
        assert_eq!(text, format!("{profile}\n[vosh_export]\n"));
        assert_eq!(read(&text).unwrap(), Some(VoshExport::default()));
    }

    #[test]
    fn a_file_without_the_table_reads_as_none() {
        let profile = ProfileConfig::default().to_toml().unwrap();
        assert_eq!(read(&profile).unwrap(), None);
        assert!(read("not = [toml").is_err());
        assert!(read("[vosh_export]\nport = \"west\"\n").is_err());
    }
}
