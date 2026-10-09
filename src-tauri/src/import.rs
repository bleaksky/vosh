//! Multi-format MUD client config importer.
//!
//! Reads config files from other MUD clients and converts the bits
//! we can model into vosh's Alias / Trigger / Macro / variable
//! types. Anything we recognize but cannot represent goes into the
//! `unsupported` bucket; lines we do not understand at all go into
//! `unparsed` so the user can port them by hand.
//!
//! Supported formats, one file each:
//!   - **`MUSHclient`** world files (`.mcl`, XML rooted at `<muclient>`)
//!   - **Mudlet** package exports (`.xml`, rooted at `<MudletPackage>`)
//!   - **GMUD** plain-text config (`gmud.cfg`-style line directives)
//!   - **CMUD** exports (XML rooted at `<cmud>`)
//!   - **`TinTin`++** scripts (`.tin`), which `#import-tintin` reads
//!
//! `vosh.rs` reads a Vosh profile export for the import under Characters,
//! which takes a whole profile rather than this report.

mod cmud;
mod gmud;
mod mudlet;
mod mushclient;
pub(crate) mod tintin;
pub(crate) mod vosh;

use quick_xml::events::BytesStart;
use quick_xml::name::QName;
use vosh_automation::alias::Alias;
use vosh_automation::trigger::{Trigger, TriggerStore};

use crate::profile::live::Macro;
use vosh::{Clash, ClashKind};

use cmud::parse_cmud;
use gmud::parse_gmud;
use mudlet::parse_mudlet;
use mushclient::parse_mushclient;

#[derive(Debug, Default, PartialEq)]
pub(crate) struct ImportReport {
    pub aliases: Vec<Alias>,
    pub triggers: Vec<Trigger>,
    pub macros: Vec<Macro>,
    pub vars: Vec<(String, String)>,
    /// Lines or elements we recognized but cannot model (Lua
    /// scripts, plugin code, color triggers, etc). Each entry is
    /// `(kind, descriptor)` so the UI can summarize by kind.
    pub unsupported: Vec<(String, String)>,
    /// Things that did not match any expected shape — usually
    /// hints at a typo or an unfamiliar dialect.
    pub unparsed: Vec<String>,
}

/// The page names a format in lowercase, so serde reads and writes
/// these names for the import commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ImportFormat {
    Mushclient,
    Mudlet,
    Gmud,
    Cmud,
}

/// Sniff the file content to guess which client it came from. The
/// frontend ships its own extension check first; this is the
/// fallback when extension is missing or ambiguous.
pub(crate) fn detect_format(text: &str) -> Option<ImportFormat> {
    let head = text.trim_start();
    if head.starts_with("<?xml") || head.starts_with('<') {
        if head.contains("<MudletPackage") {
            return Some(ImportFormat::Mudlet);
        }
        if head.contains("<muclient") || head.contains("<plugin") {
            return Some(ImportFormat::Mushclient);
        }
        if head.contains("<cmud") {
            return Some(ImportFormat::Cmud);
        }
        return None;
    }
    // Plain-text formats. GMUD lines look like
    //   alias [name] [command]
    //   macro [F1] [command]
    for raw in head.lines().take(20) {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
            continue;
        }
        if t.starts_with("alias ") || t.starts_with("macro ") || t.starts_with("variable ") {
            return Some(ImportFormat::Gmud);
        }
    }
    None
}

pub(crate) fn parse(format: ImportFormat, text: &str) -> ImportReport {
    match format {
        ImportFormat::Mushclient => parse_mushclient(text),
        ImportFormat::Mudlet => parse_mudlet(text),
        ImportFormat::Gmud => parse_gmud(text),
        ImportFormat::Cmud => parse_cmud(text),
    }
}

/// What the triggers of a report did in a profile: each one the store
/// refused, as the line the summary shows, and each one that takes the
/// name of a preset trigger.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct TriggersMerged {
    pub rejected: Vec<String>,
    pub clashes: Vec<Clash>,
}

/// Merge `triggers` into `store`, each replacing yours of the same name
/// in the same group. The other clients' importers set no group, so one
/// replaces yours of its name in no group, and yours in a group stay.
/// One that takes the name of a preset trigger, any name in `presets`,
/// the library's, or one the store holds for a preset in any group,
/// joins the clash list and leaves the preset's in place, its preset on
/// or off, since the next install would put the preset's back.
pub(crate) fn merge_triggers(
    store: &mut TriggerStore,
    triggers: &[Trigger],
    presets: &[String],
) -> TriggersMerged {
    let mut merged = TriggersMerged::default();
    for trigger in triggers {
        let name = &trigger.name;
        let preset =
            presets.contains(name) || store.named(name).iter().any(|held| held.preset.is_some());
        if preset {
            merged.clashes.push(Clash {
                kind: ClashKind::Trigger,
                name: name.clone(),
            });
        } else if let Err(e) = store.set(trigger.clone()) {
            merged
                .rejected
                .push(format!("trigger `{name}` rejected: {e}"));
        }
    }
    merged
}

fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().with_checks(false).find_map(|a| {
        let a = a.ok()?;
        if a.key == QName(name) {
            Some(String::from_utf8_lossy(&a.value).into_owned())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mudlet_by_root() {
        let text = r#"<?xml version="1.0"?><MudletPackage version="1.001"></MudletPackage>"#;
        assert_eq!(detect_format(text), Some(ImportFormat::Mudlet));
    }

    #[test]
    fn detects_mushclient_by_root() {
        let text = r#"<?xml version="1.0"?><muclient><world></world></muclient>"#;
        assert_eq!(detect_format(text), Some(ImportFormat::Mushclient));
    }

    #[test]
    fn detects_gmud_by_directives() {
        let text = "alias [g] [get $1.gold]\nmacro [F1] [say hi]\n";
        assert_eq!(detect_format(text), Some(ImportFormat::Gmud));
    }

    #[test]
    fn detect_format_cmud() {
        let text = "<?xml version=\"1.0\"?>\n<cmud>\n<window/>\n</cmud>\n";
        assert_eq!(detect_format(text), Some(ImportFormat::Cmud));
    }
}
