//! The pane layout a profile keeps in its `[ui]` table. This holds the
//! tree of panes in the panel, the cleanup that repairs a tree read from
//! disk or sent by the page, and the lazy conversion from the old dock
//! layout for a profile that has no tree yet.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::profile::ui::{default_true, UiConfig};

/// On-disk representation of a single docked bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DockEntryPersist {
    pub id: String,
    pub zone: String,
    /// Vertical alignment within a `left` or `right` zone: `"top"` or
    /// `"bottom"`. Ignored for `top`, `bottom`, and `hidden` zones.
    /// Missing means top (the default stacking behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
}

/// The built-in content types a pane can show. The panel holds up to
/// [`CHAT_PANES_MAX`] Chat panes and one of each other type, and a
/// type doubles as its first leaf's default id. Mirrored by
/// `PANE_TYPES` in src/panel/paneLayout.ts.
pub(crate) const PANE_TYPES: [&str; 5] = ["map", "affects", "group", "chat", "imm"];

/// How many Chat panes the panel holds. Mirrored by `CHAT_PANES_MAX`
/// in src/panel/paneLayout.ts.
const CHAT_PANES_MAX: usize = 4;

/// The type of a pane a plugin draws with `mud.pane`. The panel holds
/// any number of them, but only one per `plugin` and `id` in the
/// leaf's props.
/// The props also keep `title`, the last title the pane showed, so a
/// pane whose plugin is not running can still name itself.
pub(crate) const LUA_PANE: &str = "lua";

/// Schema version written into every saved pane layout.
pub(crate) const PANE_LAYOUT_VERSION: u32 = 1;

/// Bounds for the panel width in CSS pixels, so a hand edit or a
/// runaway drag cannot hide the terminal or collapse the panel.
const PANEL_WIDTH_MIN: u32 = 200;
const PANEL_WIDTH_MAX: u32 = 800;

/// Deepest depth a split may sit at, counting the root split as 0.
/// Four levels (column, row, column, row) is more than a 300 px panel
/// can show; anything deeper is a hand edit and gets flattened.
const PANE_MAX_SPLIT_DEPTH: usize = 3;

/// Weights past this are clamped so summing siblings stays finite.
const PANE_MAX_WEIGHT: f64 = 1_000_000.0;

/// The one-window panel for one profile: whether it shows, how wide
/// it is, and the tree of panes inside it. The vitals footer is
/// pinned below the tree and is not part of it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PaneLayoutPersist {
    #[serde(default = "default_pane_layout_version")]
    pub version: u32,
    #[serde(default = "default_true")]
    pub panel_open: bool,
    /// Panel width in CSS pixels. None means the stock 300 px.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_width: Option<u32>,
    /// Always a split after `sanitize`. An empty root is valid and
    /// means the panel shows only the pinned vitals.
    #[serde(default = "default_pane_root")]
    pub root: PaneNode,
}

/// One node of the pane tree. A leaf sets `pane`; a split sets
/// `split` and `children`. One struct rather than an enum keeps a
/// hand-edited profile.toml forgiving, since `sanitize` repairs
/// whatever shape it reads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PaneNode {
    /// Stable id the frontend keys pane state on. `sanitize` keeps it
    /// unless it is blank or already taken.
    #[serde(default)]
    pub id: String,
    /// Leaf content, one of [`PANE_TYPES`] or [`LUA_PANE`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<String>,
    /// Split direction: `"column"` stacks children top to bottom,
    /// `"row"` sets them side by side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    /// Share of the parent split. Siblings sum to 1 after `sanitize`.
    #[serde(default = "default_pane_weight")]
    pub weight: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PaneNode>,
    /// Per-pane settings. A Chat pane keeps `channel`, the channel it
    /// shows, and `rest`, set while it shows Everything else, the
    /// channels no other Chat pane shows. A Lua pane keeps `plugin`,
    /// `id` and `title`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, String>,
}

fn default_pane_layout_version() -> u32 {
    PANE_LAYOUT_VERSION
}

fn default_pane_weight() -> f64 {
    1.0
}

/// The map's share of the stock layout, over affects. The approved
/// boards give the Map pane 348 px and the Affects pane 315 px at
/// 1280 by 800, which shows every Affects row the boards show.
const DEFAULT_MAP_WEIGHT: f64 = 0.525;
const DEFAULT_AFFECTS_WEIGHT: f64 = 0.475;

/// Map above affects, the stock layout in the approved mockups.
fn default_pane_root() -> PaneNode {
    PaneNode::split(
        "root",
        "column",
        vec![
            PaneNode::leaf("map", DEFAULT_MAP_WEIGHT),
            PaneNode::leaf("affects", DEFAULT_AFFECTS_WEIGHT),
        ],
    )
}

/// Where each old dock panel sat on a fresh install, as
/// `(id, zone, align)`. Copied from the old frontend's panel table so
/// the migration fills ids a saved layout never mentioned the same
/// way the old frontend did.
const OLD_DOCK_DEFAULTS: [(&str, &str, &str); 8] = [
    ("map", "right", "top"),
    ("group", "right", "top"),
    ("vitals", "right", "bottom"),
    ("roomstrip", "top", "top"),
    ("chat", "hidden", "bottom"),
    ("affects", "right", "bottom"),
    ("combat", "hidden", "bottom"),
    ("imm", "hidden", "top"),
];

/// Reading order of the old zones when they fold into one column:
/// the right zone first (top then bottom stack), then the left zone,
/// then the full width strips. Hidden panels have no rank.
fn old_zone_rank(zone: &str, align: &str) -> Option<u8> {
    match (zone, align) {
        ("right", "top") => Some(0),
        ("right", _) => Some(1),
        ("left", "top") => Some(2),
        ("left", _) => Some(3),
        ("top", _) => Some(4),
        ("bottom", _) => Some(5),
        _ => None,
    }
}

impl PaneNode {
    fn leaf(pane: &str, weight: f64) -> Self {
        Self {
            id: pane.to_string(),
            pane: Some(pane.to_string()),
            split: None,
            weight,
            children: Vec::new(),
            props: BTreeMap::new(),
        }
    }

    fn split(id: &str, dir: &str, children: Vec<PaneNode>) -> Self {
        Self {
            id: id.to_string(),
            pane: None,
            split: Some(dir.to_string()),
            weight: 1.0,
            children,
            props: BTreeMap::new(),
        }
    }
}

impl PaneLayoutPersist {
    /// Map above affects with the panel open.
    pub(crate) fn default_layout() -> Self {
        Self {
            version: PANE_LAYOUT_VERSION,
            panel_open: true,
            panel_width: None,
            root: default_pane_root(),
        }
    }

    /// This panel with the stock map over affects tree, keeping whether
    /// the panel shows and how wide it is. What Reset to default puts
    /// back.
    pub(crate) fn with_default_tree(&self) -> Self {
        Self {
            version: PANE_LAYOUT_VERSION,
            panel_open: self.panel_open,
            panel_width: self.panel_width,
            root: default_pane_root(),
        }
    }

    /// Seed a profile's tree from the old zone layout the first time
    /// the profile opens in the one-window build. Mirrors how the old
    /// frontend read a dock layout: unknown ids and bad zones are
    /// skipped, and ids the list never mentions take their old default
    /// placement. Vitals is pinned now, the room strip moved into the
    /// map pane, and the combat target moved into the vitals footer, so
    /// those three never become panes. An empty list (a fresh install,
    /// or someone who never customized) gives the default layout.
    pub(crate) fn from_dock(entries: &[DockEntryPersist]) -> Self {
        if entries.is_empty() {
            return Self::default_layout();
        }
        let mut placed: Vec<(&str, &str, &str)> = Vec::new();
        for entry in entries {
            let Some(&(id, _, default_align)) =
                OLD_DOCK_DEFAULTS.iter().find(|(id, _, _)| *id == entry.id)
            else {
                continue;
            };
            let zone = entry.zone.as_str();
            let valid_zone = if id == "map" {
                matches!(zone, "left" | "right" | "hidden")
            } else {
                matches!(zone, "top" | "bottom" | "left" | "right" | "hidden")
            };
            if !valid_zone || placed.iter().any(|(seen, _, _)| *seen == id) {
                continue;
            }
            let align = match entry.align.as_deref() {
                Some("top") => "top",
                Some("bottom") => "bottom",
                _ => default_align,
            };
            placed.push((id, zone, align));
        }
        for &(id, zone, align) in &OLD_DOCK_DEFAULTS {
            if !placed.iter().any(|(seen, _, _)| *seen == id) {
                placed.push((id, zone, align));
            }
        }

        let vitals_shown = placed
            .iter()
            .any(|&(id, zone, align)| id == "vitals" && old_zone_rank(zone, align).is_some());
        let mut shown: Vec<(u8, &str)> = placed
            .iter()
            .filter(|(id, _, _)| PANE_TYPES.contains(id))
            .filter_map(|&(id, zone, align)| old_zone_rank(zone, align).map(|rank| (rank, id)))
            .collect();
        // Stable, so panes sharing a zone keep their saved order.
        shown.sort_by_key(|&(rank, _)| rank);

        let has_map = shown.iter().any(|&(_, id)| id == "map");
        let has_affects = shown.iter().any(|&(_, id)| id == "affects");
        let others = shown.len() - usize::from(has_map);
        let children: Vec<PaneNode> = shown
            .iter()
            .map(|&(_, id)| PaneNode::leaf(id, migrated_weight(id, has_map, has_affects, others)))
            .collect();

        let mut layout = Self {
            version: PANE_LAYOUT_VERSION,
            panel_open: !children.is_empty() || vitals_shown,
            panel_width: None,
            root: PaneNode::split("root", "column", children),
        };
        layout.sanitize();
        layout
    }

    /// Repair a layout read from disk or sent by the frontend. Unknown
    /// pane types, panes past their cap (see [`pane_cap`]) and Lua
    /// panes without a plugin or an id drop out, blank or clashing ids get
    /// fresh ones, a split with one child gives way to that child, a
    /// split inside a split of the same direction merges into it,
    /// splits nested deeper than [`PANE_MAX_SPLIT_DEPTH`] flatten,
    /// weights become positive shares that sum to 1 (rounded to four
    /// places), and the root is always a split. The same rules run in
    /// `sanitize` in src/panel/paneLayout.ts, and both are checked
    /// against fixtures/pane-layout/sanitize.json.
    pub(crate) fn sanitize(&mut self) {
        self.version = PANE_LAYOUT_VERSION;
        self.panel_width = self
            .panel_width
            .map(|w| w.clamp(PANEL_WIDTH_MIN, PANEL_WIDTH_MAX));
        let root = std::mem::replace(
            &mut self.root,
            PaneNode::split("root", "column", Vec::new()),
        );
        self.root = TreeSanitizer::new(&root).root(root);
    }
}

impl UiConfig {
    /// The pane layout the panel should show: the saved tree, or one
    /// migrated from `dock_layout` while this profile has none. The
    /// migration is lazy, so nothing reaches disk until the first edit
    /// and `dock_layout` stays intact for a rollback.
    pub(crate) fn pane_layout(&self) -> PaneLayoutPersist {
        match &self.panes {
            Some(saved) => {
                let mut layout = saved.clone();
                layout.sanitize();
                layout
            }
            None => PaneLayoutPersist::from_dock(&self.dock_layout),
        }
    }
}

/// The pane types under `node` in tree order, which reads each split top
/// to bottom or left to right.
pub(crate) fn leaf_panes(node: &PaneNode) -> Vec<String> {
    if let Some(pane) = &node.pane {
        return vec![pane.clone()];
    }
    node.children.iter().flat_map(leaf_panes).collect()
}

#[allow(clippy::cast_precision_loss)]
fn count_as_f64(n: usize) -> f64 {
    n as f64
}

/// Weight for a pane migrated from the old dock. Over a single pane the
/// map takes the default layout's share. With two or more panes under
/// it the map drops to 0.45 and affects, the longest list, takes 0.3 so
/// its rows still show, and the rest share what is left. Without a map
/// the panes split evenly. `others` counts the panes that are not the
/// map. Sanitize normalizes the weights afterward.
fn migrated_weight(id: &str, has_map: bool, has_affects: bool, others: usize) -> f64 {
    if !has_map || others == 0 {
        return 1.0;
    }
    if others == 1 {
        return if id == "map" {
            DEFAULT_MAP_WEIGHT
        } else {
            DEFAULT_AFFECTS_WEIGHT
        };
    }
    match id {
        "map" => 0.45,
        "affects" => 0.3,
        _ if has_affects => 0.25 / count_as_f64(others - 1),
        _ => 0.55 / count_as_f64(others),
    }
}

/// What the tree counts a leaf as: its type for a built-in pane, its
/// plugin and id for a Lua pane. Mirrored by `paneKey` in
/// src/panel/paneLayout.ts.
#[derive(PartialEq, Eq, Hash)]
enum PaneKey {
    Builtin(&'static str),
    Lua { plugin: String, id: String },
}

/// The key of a leaf of type `kind`, or None for a Lua leaf whose
/// props lack a plugin or an id.
fn pane_key(kind: &'static str, props: &BTreeMap<String, String>) -> Option<PaneKey> {
    if kind != LUA_PANE {
        return Some(PaneKey::Builtin(kind));
    }
    let prop = |name: &str| {
        props
            .get(name)
            .filter(|value| !value.trim().is_empty())
            .cloned()
    };
    Some(PaneKey::Lua {
        plugin: prop("plugin")?,
        id: prop("id")?,
    })
}

/// How many leaves the tree keeps with `key`: [`CHAT_PANES_MAX`] for
/// Chat, one for anything else. Mirrored by `paneCap` in
/// src/panel/paneLayout.ts.
fn pane_cap(key: &PaneKey) -> usize {
    match key {
        PaneKey::Builtin("chat") => CHAT_PANES_MAX,
        _ => 1,
    }
}

/// Walks a raw tree once, handing out ids and remembering which panes
/// it has already placed.
struct TreeSanitizer {
    /// Every non-blank id in the raw tree, so a fresh id never steals
    /// one a later node already owns.
    reserved: HashSet<String>,
    used: HashSet<String>,
    /// How many leaves of each key are placed so far.
    panes: HashMap<PaneKey, usize>,
}

impl TreeSanitizer {
    fn new(root: &PaneNode) -> Self {
        fn collect(node: &PaneNode, out: &mut HashSet<String>) {
            if !node.id.trim().is_empty() {
                out.insert(node.id.clone());
            }
            for child in &node.children {
                collect(child, out);
            }
        }
        let mut reserved = HashSet::new();
        collect(root, &mut reserved);
        Self {
            reserved,
            used: HashSet::new(),
            panes: HashMap::new(),
        }
    }

    /// Keep `raw` when it is non-blank and unclaimed, otherwise hand
    /// out `base`, `base-2`, `base-3`, and so on.
    fn claim_id(&mut self, raw: &str, base: &str) -> String {
        if !raw.trim().is_empty() && !self.used.contains(raw) {
            self.used.insert(raw.to_string());
            return raw.to_string();
        }
        let mut n = 1u32;
        loop {
            let candidate = if n == 1 {
                base.to_string()
            } else {
                format!("{base}-{n}")
            };
            if !self.used.contains(&candidate) && !self.reserved.contains(&candidate) {
                self.used.insert(candidate.clone());
                return candidate;
            }
            n += 1;
        }
    }

    fn root(&mut self, raw: PaneNode) -> PaneNode {
        // A bare leaf at the root gets wrapped so the root stays a split.
        let raw = if raw.pane.is_some() {
            PaneNode::split("", "column", vec![raw])
        } else {
            raw
        };
        let mut dir = split_dir(raw.split.as_deref());
        let id = self.claim_id(&raw.id, "root");
        let mut children = self.children(dir, raw.children, 0);
        // A lone split under the root takes its place, keeping the root id.
        if children.len() == 1 && children[0].pane.is_none() {
            let only = children.remove(0);
            dir = split_dir(only.split.as_deref());
            children = only.children;
        }
        PaneNode::split(&id, dir, children)
    }

    fn node(&mut self, raw: PaneNode, depth: usize) -> Option<PaneNode> {
        let weight = clean_weight(raw.weight);
        if let Some(kind) = raw.pane.as_deref() {
            let kind = pane_type(kind)?;
            let key = pane_key(kind, &raw.props)?;
            let cap = pane_cap(&key);
            let placed = self.panes.entry(key).or_default();
            if *placed >= cap {
                return None;
            }
            *placed += 1;
            let id = self.claim_id(&raw.id, kind);
            return Some(PaneNode {
                id,
                props: raw.props,
                ..PaneNode::leaf(kind, weight)
            });
        }
        let dir = split_dir(raw.split.as_deref());
        let id = self.claim_id(&raw.id, "split");
        let mut children = self.children(dir, raw.children, depth);
        if children.len() > 1 {
            return Some(PaneNode {
                weight,
                ..PaneNode::split(&id, dir, children)
            });
        }
        let mut only = children.pop()?;
        only.weight = weight;
        Some(only)
    }

    fn children(&mut self, dir: &str, raw: Vec<PaneNode>, depth: usize) -> Vec<PaneNode> {
        let mut out = Vec::new();
        for child in raw {
            let Some(node) = self.node(child, depth + 1) else {
                continue;
            };
            if node.pane.is_some() {
                out.push(node);
            } else if node.split.as_deref() == Some(dir) {
                // Same direction as this split: lift the grandchildren,
                // whose shares already sum to 1 inside the child.
                for mut grandchild in node.children {
                    grandchild.weight *= node.weight;
                    out.push(grandchild);
                }
            } else if depth + 1 > PANE_MAX_SPLIT_DEPTH {
                let leaves = collect_leaves(node.children);
                let share = node.weight / count_as_f64(leaves.len());
                for mut leaf in leaves {
                    leaf.weight = share;
                    out.push(leaf);
                }
            } else {
                out.push(node);
            }
        }
        normalize_weights(&mut out);
        out
    }
}

fn collect_leaves(nodes: Vec<PaneNode>) -> Vec<PaneNode> {
    let mut out = Vec::new();
    for node in nodes {
        if node.pane.is_some() {
            out.push(node);
        } else {
            out.extend(collect_leaves(node.children));
        }
    }
    out
}

fn pane_type(raw: &str) -> Option<&'static str> {
    let wanted = raw.trim().to_lowercase();
    PANE_TYPES
        .iter()
        .copied()
        .chain([LUA_PANE])
        .find(|t| *t == wanted)
}

fn split_dir(raw: Option<&str>) -> &'static str {
    match raw.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("row") => "row",
        _ => "column",
    }
}

fn clean_weight(w: f64) -> f64 {
    if w.is_finite() && w > 0.0 {
        w.min(PANE_MAX_WEIGHT)
    } else {
        1.0
    }
}

/// Scale sibling weights to sum to 1 unless they already do (within
/// 0.001, which keeps a second pass from nudging rounded values), then
/// round each to four places with a floor of 0.0001.
fn normalize_weights(nodes: &mut [PaneNode]) {
    let sum: f64 = nodes.iter().map(|n| n.weight).sum();
    if sum > 0.0 && (sum - 1.0).abs() > 1e-3 {
        for node in nodes.iter_mut() {
            node.weight /= sum;
        }
    }
    for node in nodes.iter_mut() {
        node.weight = ((node.weight * 10_000.0).round() / 10_000.0).max(0.0001);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::profile::file::ProfileConfig;
    use crate::profile::live::Profile;
    use crate::profile::shared::{strip_global_fields, GlobalConfig};

    fn dock(entries: &[(&str, &str, Option<&str>)]) -> Vec<DockEntryPersist> {
        entries
            .iter()
            .map(|&(id, zone, align)| DockEntryPersist {
                id: id.to_string(),
                zone: zone.to_string(),
                align: align.map(str::to_string),
            })
            .collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// A layout with a nested row, a panel width and per-pane props,
    /// so round trips cover every field.
    pub(crate) fn custom_layout() -> PaneLayoutPersist {
        let mut chat = PaneNode::leaf("chat", 0.5);
        chat.props.insert("channel".into(), "tell".into());
        let row = PaneNode {
            weight: 0.4,
            ..PaneNode::split("split", "row", vec![PaneNode::leaf("group", 0.5), chat])
        };
        let mut layout = PaneLayoutPersist {
            version: PANE_LAYOUT_VERSION,
            panel_open: false,
            panel_width: Some(360),
            root: PaneNode::split("root", "column", vec![PaneNode::leaf("map", 0.6), row]),
        };
        layout.sanitize();
        layout
    }

    #[test]
    fn default_layout_is_map_over_affects() {
        let layout = PaneLayoutPersist::default_layout();
        assert!(layout.panel_open);
        assert_eq!(layout.panel_width, None);
        assert_eq!(layout.root.split.as_deref(), Some("column"));
        assert_eq!(leaf_panes(&layout.root), ["map", "affects"]);
        // The boards' split, 348 px over 315 px of the 663 px the two
        // panes share at 1280 by 800.
        assert!(close(layout.root.children[0].weight, 0.525));
        assert!(close(layout.root.children[1].weight, 0.475));
        // Already canonical, so a sanitize pass leaves it alone.
        let mut again = layout.clone();
        again.sanitize();
        assert_eq!(again, layout);
    }

    #[test]
    fn with_default_tree_keeps_the_panel_and_replaces_the_tree() {
        let reset = custom_layout().with_default_tree();
        assert_eq!(reset.root, PaneLayoutPersist::default_layout().root);
        assert_eq!(reset.panel_open, custom_layout().panel_open);
        assert_eq!(reset.panel_width, custom_layout().panel_width);
    }

    #[test]
    fn from_dock_empty_yields_default() {
        assert_eq!(
            PaneLayoutPersist::from_dock(&[]),
            PaneLayoutPersist::default_layout()
        );
    }

    #[test]
    fn from_dock_keeps_right_order_and_drops_vitals_roomstrip_combat_hidden() {
        let entries = dock(&[
            ("vitals", "right", Some("bottom")),
            ("affects", "right", Some("bottom")),
            ("chat", "bottom", None),
            ("map", "right", Some("top")),
            ("roomstrip", "top", None),
            ("combat", "right", Some("top")),
            ("group", "left", Some("top")),
            ("imm", "hidden", None),
        ]);
        let layout = PaneLayoutPersist::from_dock(&entries);
        assert!(layout.panel_open);
        assert_eq!(
            leaf_panes(&layout.root),
            ["map", "affects", "group", "chat"]
        );
        // Affects gets the largest share under the map so its rows show.
        let weights: Vec<f64> = layout.root.children.iter().map(|n| n.weight).collect();
        assert!(close(weights[0], 0.45));
        assert!(close(weights[1], 0.3));
        assert!(weights[2..].iter().all(|w| close(*w, 0.125)));
        // Ids are the pane types, so every read of the same dock layout
        // hands the frontend the same ids.
        assert_eq!(layout.root.children[0].id, "map");
    }

    #[test]
    fn from_dock_without_affects_splits_the_rest_under_the_map() {
        let layout = PaneLayoutPersist::from_dock(&dock(&[
            ("map", "right", Some("top")),
            ("group", "right", Some("top")),
            ("chat", "right", None),
            ("affects", "hidden", None),
        ]));
        assert_eq!(leaf_panes(&layout.root), ["map", "group", "chat"]);
        let weights: Vec<f64> = layout.root.children.iter().map(|n| n.weight).collect();
        assert!(close(weights[0], 0.45));
        assert!(weights[1..].iter().all(|w| close(*w, 0.275)));
    }

    #[test]
    fn from_dock_fills_ids_the_list_never_mentions() {
        // Only chat was ever saved. The rest take their old defaults:
        // map and group stack at the top of the right zone, affects at
        // the bottom, and chat (no align, so its bottom default) joins
        // the bottom stack ahead of affects because it comes first.
        let layout = PaneLayoutPersist::from_dock(&dock(&[("chat", "right", None)]));
        assert_eq!(
            leaf_panes(&layout.root),
            ["map", "group", "chat", "affects"]
        );
    }

    #[test]
    fn from_dock_rejects_a_map_in_a_strip() {
        // The map never allowed the top or bottom strip, so the saved
        // entry is skipped and the map falls back to the right zone.
        let layout = PaneLayoutPersist::from_dock(&dock(&[
            ("map", "top", None),
            ("affects", "hidden", None),
        ]));
        assert_eq!(leaf_panes(&layout.root), ["map", "group"]);
        assert!(close(layout.root.children[0].weight, 0.525));
        assert!(close(layout.root.children[1].weight, 0.475));
    }

    #[test]
    fn from_dock_all_hidden_closes_panel() {
        let ids = [
            "map",
            "group",
            "vitals",
            "roomstrip",
            "chat",
            "affects",
            "combat",
            "imm",
        ];
        let entries: Vec<_> = ids.iter().map(|id| (*id, "hidden", None)).collect();
        let layout = PaneLayoutPersist::from_dock(&dock(&entries));
        assert!(!layout.panel_open);
        let leftover = &layout.root.children;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn from_dock_vitals_alone_keeps_the_panel_open_with_no_panes() {
        let entries = dock(&[
            ("map", "hidden", None),
            ("group", "hidden", None),
            ("vitals", "left", Some("top")),
            ("affects", "hidden", None),
        ]);
        let layout = PaneLayoutPersist::from_dock(&entries);
        assert!(layout.panel_open);
        let leftover = &layout.root.children;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(layout.root.split.as_deref(), Some("column"));
    }

    #[test]
    fn sanitize_matches_the_shared_fixtures() {
        // The same cases run against sanitize in src/panel/paneLayout.ts,
        // so the two implementations cannot drift apart.
        let text = include_str!("../../../fixtures/pane-layout/sanitize.json");
        let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty(), "expected entries");
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let mut got: PaneLayoutPersist = serde_json::from_value(case["input"].clone()).unwrap();
            got.sanitize();
            let expected: PaneLayoutPersist =
                serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(got, expected, "case `{name}`");
            let mut again = got.clone();
            again.sanitize();
            assert_eq!(
                again, got,
                "case `{name}` is not stable under a second pass"
            );
        }
    }

    #[test]
    fn panes_round_trip_through_toml() {
        let mut config = ProfileConfig::default();
        config.ui.panes = Some(custom_layout());
        let text = config.to_toml().unwrap();
        assert!(text.contains("[ui.panes]"), "{text}");
        let parsed = ProfileConfig::from_toml(&text).unwrap();
        assert_eq!(parsed.ui.panes, Some(custom_layout()));
    }

    #[test]
    fn profile_without_panes_loads_none() {
        let parsed = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n").unwrap();
        assert!(parsed.ui.panes.is_none());
        assert_eq!(parsed.ui.pane_layout(), PaneLayoutPersist::default_layout());
        // Nothing is written for a profile that never touched its panes.
        assert!(!parsed.to_toml().unwrap().contains("panes"));
    }

    #[test]
    fn pane_layout_prefers_the_saved_tree_over_the_dock() {
        // Group was saved first, so it leads the right zone's top stack
        // ahead of the defaulted map, as it did in the old dock.
        let mut ui = UiConfig {
            dock_layout: dock(&[("group", "right", None)]),
            ..UiConfig::default()
        };
        assert_eq!(
            leaf_panes(&ui.pane_layout().root),
            ["group", "map", "affects"]
        );
        ui.panes = Some(custom_layout());
        assert_eq!(ui.pane_layout(), custom_layout());
        // The dock layout stays untouched for a rollback.
        assert_eq!(ui.dock_layout.len(), 1);
    }

    #[test]
    fn profiles_keep_their_own_panes_across_a_switch() {
        use crate::profile::set::ProfileSet;

        // Mirrors persist_profile: per-profile file minus the global
        // fields, plus global.toml.
        fn persist(set: &ProfileSet, profile: &Profile) {
            let mut snapshot = ProfileConfig::from_profile(profile);
            strip_global_fields(&mut snapshot, set.scope());
            snapshot.save(&set.active_path()).unwrap();
            GlobalConfig::from_profile(profile, set.scope())
                .save(&set.global_path())
                .unwrap();
        }
        // Mirrors apply_profile_switch step 3.
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
        let first = set.active_name().to_string();
        let mut profile = Profile::default();
        profile.ui.panes = Some(custom_layout());
        persist(&set, &profile);

        set.create("alt").unwrap();
        set.switch("alt").unwrap();
        let mut alt = load(&set);
        assert!(alt.ui.panes.is_none(), "a new profile starts unarranged");
        alt.ui.panes = Some(PaneLayoutPersist::default_layout());
        persist(&set, &alt);

        set.switch(&first).unwrap();
        assert_eq!(load(&set).ui.panes, Some(custom_layout()));
        set.switch("alt").unwrap();
        assert_eq!(
            load(&set).ui.panes,
            Some(PaneLayoutPersist::default_layout())
        );
    }
}
