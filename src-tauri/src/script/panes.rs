//! The panes the plugins of a session draw with `mud.pane`, each kept by
//! its plugin and id with its title, the words beside its name and its
//! blocks. A step marks each pane it changes or removes, and the session
//! sends what changed once per flush on `session://lua-panes`, so the
//! Room.Weather and Char.State that come with every prompt send one
//! event a read. The main window reads every pane as it opens through
//! `lua_panes_get`.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vosh_script::PaneBlock;

/// One block of a pane as the page reads it, told apart by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum Block {
    Row { label: String, value: String },
    Gauge { label: String, value: f64, max: f64 },
    Line { text: String },
    Rule,
}

impl From<PaneBlock> for Block {
    fn from(block: PaneBlock) -> Self {
        match block {
            PaneBlock::Row { label, value } => Block::Row { label, value },
            PaneBlock::Gauge { label, value, max } => Block::Gauge { label, value, max },
            PaneBlock::Line(text) => Block::Line { text },
            PaneBlock::Rule => Block::Rule,
        }
    }
}

/// What a pane shows.
#[derive(Debug, Clone, Default, PartialEq)]
struct Pane {
    title: String,
    /// The words beside the title, empty for none.
    meta: String,
    blocks: Vec<Block>,
}

/// A pane as the page reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct LuaPane {
    pub(crate) plugin: String,
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) meta: String,
    pub(crate) blocks: Vec<Block>,
}

/// A pane that went, by its plugin and id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PaneId {
    pub(crate) plugin: String,
    pub(crate) id: String,
}

/// What `session://lua-panes` carries beside the session: each pane that
/// changed since the last send, whole, and each one that went.
#[derive(Debug, Default, PartialEq, Serialize)]
pub(crate) struct LuaPanesPayload {
    pub(crate) panes: Vec<LuaPane>,
    pub(crate) removed: Vec<PaneId>,
}

/// The panes of one session's plugins, and which changed since the last
/// send.
#[derive(Debug, Default)]
pub(crate) struct LuaPanes {
    panes: BTreeMap<(String, String), Pane>,
    /// Each pane changed or removed since the last send.
    dirty: BTreeSet<(String, String)>,
}

impl LuaPanes {
    /// The plugin `plugin` draws the pane `id` under `title`. A pane it
    /// draws again keeps what it shows under the new title.
    pub(crate) fn draw(&mut self, plugin: String, id: String, title: String) {
        let key = (plugin, id);
        self.panes.entry(key.clone()).or_default().title = title;
        self.dirty.insert(key);
    }

    /// Replace what the pane `id` of `plugin` shows. Returns false when
    /// the plugin draws no such pane.
    pub(crate) fn set(&mut self, plugin: String, id: String, blocks: Vec<PaneBlock>) -> bool {
        self.change((plugin, id), |pane| {
            pane.blocks = blocks.into_iter().map(Block::from).collect();
        })
    }

    /// Put `text` beside the name of the pane `id` of `plugin`. Returns
    /// false when the plugin draws no such pane.
    pub(crate) fn meta(&mut self, plugin: String, id: String, text: String) -> bool {
        self.change((plugin, id), |pane| pane.meta = text)
    }

    fn change(&mut self, key: (String, String), edit: impl FnOnce(&mut Pane)) -> bool {
        let Some(pane) = self.panes.get_mut(&key) else {
            return false;
        };
        edit(pane);
        self.dirty.insert(key);
        true
    }

    /// Remove every pane of `plugin`, which turned off, stopped or loads
    /// again. Returns whether it drew any.
    pub(crate) fn drop_plugin(&mut self, plugin: &str) -> bool {
        let gone: Vec<(String, String)> = self
            .panes
            .keys()
            .filter(|(owner, _)| owner == plugin)
            .cloned()
            .collect();
        for key in &gone {
            self.panes.remove(key);
        }
        let any = !gone.is_empty();
        self.dirty.extend(gone);
        any
    }

    /// What changed since the last send, which counts as sent now. None
    /// when nothing did.
    pub(crate) fn take_changes(&mut self) -> Option<LuaPanesPayload> {
        if self.dirty.is_empty() {
            return None;
        }
        let mut payload = LuaPanesPayload::default();
        for (plugin, id) in std::mem::take(&mut self.dirty) {
            match self.panes.get(&(plugin.clone(), id.clone())) {
                Some(pane) => payload.panes.push(view(plugin, id, pane)),
                None => payload.removed.push(PaneId { plugin, id }),
            }
        }
        Some(payload)
    }

    /// Every pane, by plugin and then id.
    pub(crate) fn all(&self) -> Vec<LuaPane> {
        self.panes
            .iter()
            .map(|((plugin, id), pane)| view(plugin.clone(), id.clone(), pane))
            .collect()
    }
}

fn view(plugin: String, id: String, pane: &Pane) -> LuaPane {
    LuaPane {
        plugin,
        id,
        title: pane.title.clone(),
        meta: pane.meta.clone(),
        blocks: pane.blocks.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn blocks_reach_the_page_tagged_by_kind() {
        let blocks: Vec<Block> = vec![
            PaneBlock::Row {
                label: "Sky".into(),
                value: "rainy".into(),
            }
            .into(),
            PaneBlock::Gauge {
                label: "Health".into(),
                value: 1020.0,
                max: 1200.0,
            }
            .into(),
            PaneBlock::Line("{red}Coastal North{reset}".into()).into(),
            PaneBlock::Rule.into(),
        ];
        assert_eq!(
            serde_json::to_value(&blocks).expect("json"),
            json!([
                {"kind": "row", "label": "Sky", "value": "rainy"},
                {"kind": "gauge", "label": "Health", "value": 1020.0, "max": 1200.0},
                {"kind": "line", "text": "{red}Coastal North{reset}"},
                {"kind": "rule"},
            ])
        );
    }

    #[test]
    fn a_change_sends_once_and_a_removal_names_the_pane() {
        let mut panes = LuaPanes::default();
        assert_eq!(panes.take_changes(), None);
        panes.draw("weather_pane".into(), "weather".into(), "Weather".into());
        assert!(panes.meta(
            "weather_pane".into(),
            "weather".into(),
            "Coastal North".into()
        ));
        assert!(!panes.meta("weather_pane".into(), "worth".into(), "x".into()));
        let sent = panes.take_changes().expect("a change");
        assert_eq!(sent.panes.len(), 1);
        assert_eq!(sent.panes[0].meta, "Coastal North");
        assert_eq!(panes.take_changes(), None);
        assert!(!panes.drop_plugin("worth_pane"));
        assert!(panes.drop_plugin("weather_pane"));
        assert_eq!(
            panes.take_changes(),
            Some(LuaPanesPayload {
                panes: Vec::new(),
                removed: vec![PaneId {
                    plugin: "weather_pane".into(),
                    id: "weather".into(),
                }],
            })
        );
        assert_eq!(panes.all(), Vec::new());
    }
}
