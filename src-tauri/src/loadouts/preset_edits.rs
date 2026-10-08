//! Your edits to the presets, the `[preset_edits]` table. Each row you
//! changed keeps your value and the preset's
//! value you changed it from, so a fix to the preset still reaches every
//! row you left alone, and Vosh can tell when a fix lands on a row you
//! changed. The table sits beside the list of presets that are on, in the
//! profile file in per profile mode and in catalog.toml in loadout mode,
//! as the `[alerts]` table does.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Your edits, by preset id. A preset with none has no entry.
pub(crate) type PresetEdits = BTreeMap<String, PresetEdit>;

/// Your edits to one preset: its swatches by color key, and its triggers'
/// rows by trigger name, then by row key. A row in a list keys by what
/// the preset holds there, a pattern by its text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct PresetEdit {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) colors: BTreeMap<String, EditRow>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) triggers: BTreeMap<String, BTreeMap<String, EditRow>>,
}

impl PresetEdit {
    pub(crate) fn is_empty(&self) -> bool {
        self.colors.is_empty() && self.triggers.is_empty()
    }
}

/// One row you changed: your value and the preset's value you changed it
/// from, as TOML values so a swatch, a switch, a number, a text and a
/// list all fit. `seen` is the preset's value a launch notice already
/// named when a fix changed this row, so the notice tells each fix once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct EditRow {
    pub(crate) value: toml::Value,
    pub(crate) was: toml::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) seen: Option<toml::Value>,
}

impl EditRow {
    /// Whether the row says the preset's value and so is no edit.
    fn folds(&self) -> bool {
        self.value == self.was
    }
}

/// Merge `edits`, what the page saved for the preset `id`, into `table`
/// row by row. A row the page sent keeps the `was` of the row already
/// there, so an edit you change again still knows the preset value you
/// started from, and keeps its `seen` while the page sends one, so a
/// flagged row stays flagged until you choose. A row the page sends with
/// no `seen` over one that has it is that choice: it takes the page's
/// `was`, the preset's value now, and the flag clears. A row whose value
/// is the preset's, before or now, folds away, and so does a preset left
/// with no rows. A row the page did not send stays as it stood.
pub(crate) fn merge(table: &mut PresetEdits, id: &str, edits: PresetEdit) {
    let mut entry = table.remove(id).unwrap_or_default();
    merge_rows(&mut entry.colors, edits.colors);
    for (name, rows) in edits.triggers {
        let mut held = entry.triggers.remove(&name).unwrap_or_default();
        merge_rows(&mut held, rows);
        if !held.is_empty() {
            entry.triggers.insert(name, held);
        }
    }
    if !entry.is_empty() {
        table.insert(id.to_string(), entry);
    }
}

fn merge_rows(held: &mut BTreeMap<String, EditRow>, sent: BTreeMap<String, EditRow>) {
    for (key, row) in sent {
        let now = row.was.clone();
        let merged = match held.remove(&key) {
            Some(old) if row.seen.is_some() || old.seen.is_none() => EditRow {
                value: row.value,
                was: old.was,
                seen: row.seen.or(old.seen),
            },
            _ => row,
        };
        if !merged.folds() && merged.value != now {
            held.insert(key, merged);
        }
    }
}

/// Orla's lilac line in Disarms and fading buffs, the color row of its
/// card, for the tests that carry a table from file to file.
#[cfg(test)]
pub(crate) fn lilac_line() -> PresetEdits {
    let line = EditRow {
        value: "#c3a6ff".into(),
        was: "fg:178".into(),
        seen: None,
    };
    PresetEdits::from([(
        "disarm_buff_fade".into(),
        PresetEdit {
            colors: BTreeMap::from([("line".into(), line)]),
            ..PresetEdit::default()
        },
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(value: &str, was: &str, seen: Option<&str>) -> EditRow {
        EditRow {
            value: value.into(),
            was: was.into(),
            seen: seen.map(Into::into),
        }
    }

    /// The edit of `trigger`'s `key` row that preset `disarm_buff_fade`
    /// sends.
    fn send(trigger: &str, key: &str, sent: EditRow) -> PresetEdit {
        PresetEdit {
            triggers: BTreeMap::from([(trigger.into(), BTreeMap::from([(key.into(), sent)]))]),
            ..PresetEdit::default()
        }
    }

    fn held<'a>(table: &'a PresetEdits, trigger: &str, key: &str) -> Option<&'a EditRow> {
        table
            .get("disarm_buff_fade")?
            .triggers
            .get(trigger)?
            .get(key)
    }

    const ID: &str = "disarm_buff_fade";
    const SECONDARY: &str = "disarm.secondary";

    #[test]
    fn a_second_set_keeps_the_first_was() {
        let mut table = PresetEdits::new();
        merge(
            &mut table,
            ID,
            send(SECONDARY, "send", row("get 1.", "get 1.;wield 1.", None)),
        );
        merge(
            &mut table,
            ID,
            send(SECONDARY, "send", row("", "get 1.;dual 1.", None)),
        );
        assert_eq!(
            held(&table, SECONDARY, "send"),
            Some(&row("", "get 1.;wield 1.", None))
        );
    }

    #[test]
    fn a_row_set_back_to_the_preset_folds_and_takes_its_preset_with_it() {
        let mut table = PresetEdits::new();
        let line = PresetEdit {
            colors: BTreeMap::from([("line".into(), row("#c3a6ff", "fg:178", None))]),
            ..PresetEdit::default()
        };
        merge(&mut table, ID, line);
        merge(
            &mut table,
            ID,
            send(SECONDARY, "send", row("", "get 1.;wield 1.", None)),
        );
        merge(
            &mut table,
            ID,
            send(
                SECONDARY,
                "send",
                row("get 1.;wield 1.", "get 1.;wield 1.", None),
            ),
        );
        assert!(!table[ID].triggers.contains_key(SECONDARY), "{table:?}");
        let colors = PresetEdit {
            colors: BTreeMap::from([("line".into(), row("fg:178", "fg:178", None))]),
            ..PresetEdit::default()
        };
        merge(&mut table, ID, colors);
        assert!(table.is_empty(), "{table:?}");
    }

    #[test]
    fn a_flagged_row_keeps_seen_until_a_choice_clears_it() {
        let mut table = PresetEdits::new();
        merge(
            &mut table,
            ID,
            send(SECONDARY, "send", row("", "get 1.;wield 1.", None)),
        );
        // The launch names the fix once.
        let flagged = row("", "get 1.;wield 1.", Some("get 1.;dual 1."));
        merge(&mut table, ID, send(SECONDARY, "send", flagged.clone()));
        assert_eq!(held(&table, SECONDARY, "send"), Some(&flagged));
        // An edit of the flagged row keeps the flag.
        merge(
            &mut table,
            ID,
            send(
                SECONDARY,
                "send",
                row("get 1.", "get 1.;dual 1.", Some("get 1.;dual 1.")),
            ),
        );
        assert_eq!(
            held(&table, SECONDARY, "send"),
            Some(&row("get 1.", "get 1.;wield 1.", Some("get 1.;dual 1.")))
        );
        // Keep mine moves was to the preset's new value and clears seen.
        merge(
            &mut table,
            ID,
            send(SECONDARY, "send", row("get 1.", "get 1.;dual 1.", None)),
        );
        assert_eq!(
            held(&table, SECONDARY, "send"),
            Some(&row("get 1.", "get 1.;dual 1.", None))
        );
    }

    #[test]
    fn taking_the_fix_drops_the_flagged_row() {
        let mut table = PresetEdits::new();
        let flagged = row("", "get 1.;wield 1.", Some("get 1.;dual 1."));
        merge(&mut table, ID, send(SECONDARY, "send", flagged));
        merge(
            &mut table,
            ID,
            send(
                SECONDARY,
                "send",
                row("get 1.;dual 1.", "get 1.;dual 1.", None),
            ),
        );
        assert!(table.is_empty(), "{table:?}");
    }

    #[test]
    fn rows_the_page_did_not_send_stay() {
        let mut table = PresetEdits::new();
        let off = row_bool(false, true);
        merge(
            &mut table,
            ID,
            send("buff.sanctuary", "enabled", off.clone()),
        );
        merge(
            &mut table,
            ID,
            send(SECONDARY, "send", row("", "get 1.;wield 1.", None)),
        );
        assert_eq!(held(&table, "buff.sanctuary", "enabled"), Some(&off));
        assert!(held(&table, SECONDARY, "send").is_some());
    }

    fn row_bool(value: bool, was: bool) -> EditRow {
        EditRow {
            value: value.into(),
            was: was.into(),
            seen: None,
        }
    }
}
