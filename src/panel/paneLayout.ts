import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  paneHistoryRestore,
  paneLayoutGet,
  paneLayoutReset,
  paneTreesSet,
  paneLayoutSet,
  subscribePaneLayoutChanged,
} from '../ipc/panes';
import { subscribeProfileSwitched } from '../ipc/profiles';
import { pendingWrites } from '../lib/pendingWrites';

// The one-window panel's pane tree and the sidebar's, saved per
// profile. Mirrors PaneLayoutPersist and PaneNode in
// src-tauri/src/profile/panes.rs. The sidebar across from the panel
// holds a tree of its own under the session list. A pane shows in one
// of the two at a time, Chat up to four times across both, and the
// writing card only in the panel.
//
// The backend owns the saved copy. getPaneLayout reads the active
// profile's tree, setPaneLayout writes one (debounced for splitter
// drags), and subscribePaneLayout hears trees that change somewhere
// else, which in practice means a profile switch. The tree operations
// are pure and always return a sanitized tree, so the panel can keep
// the result in state and hand it straight back to setPaneLayout.

/** The built-in content a pane can show. The panel holds up to
 *  CHAT_PANES_MAX Chat panes and one of each other type. `writing` is
 *  the writing card pinned to the panel, which only the card's pin adds
 *  (WRITING_PANE). Mirrors PANE_TYPES in src-tauri/src/profile/panes.rs. */
export const PANE_TYPES = ['map', 'affects', 'group', 'stats', 'chat', 'imm', 'writing'] as const;
export type PaneType = (typeof PANE_TYPES)[number];

/** The pane the writing card lives in while you pin it. Add a pane,
 *  the View menu and the palette never offer it. */
export const WRITING_PANE = 'writing';

/** The pane types you add or show from the menus: every built-in type
 *  but the writing card's. */
export type OfferedPaneType = Exclude<PaneType, typeof WRITING_PANE>;

/** Whether `t` is a pane type the menus offer. */
export function isOfferedPaneType(t: PaneType): t is OfferedPaneType {
  return t !== WRITING_PANE;
}

/** How many Chat panes the panel holds. Mirrors CHAT_PANES_MAX in
 *  src-tauri/src/profile/panes.rs. */
export const CHAT_PANES_MAX = 4;

/** The type of a pane a plugin draws with mud.pane. The panel holds
 *  any number of them, but only one per `plugin` and `id` in the
 *  leaf's props.
 *  The props also keep `title`, the last title the pane showed, so a
 *  pane whose plugin is not running can still name itself. */
export const LUA_PANE = 'lua';
export type PaneKind = PaneType | typeof LUA_PANE;

/** A pane as the tree tells it apart: its type, plus the props that
 *  name a Lua pane. A leaf is one, and the tree operations place one. */
export interface PaneRef {
  pane: PaneKind;
  props: Record<string, string>;
}

/** The reference to a built-in pane, which needs no props. */
export function paneRef(pane: PaneType): PaneRef {
  return { pane, props: {} };
}

/** What the tree counts a pane as: its type for a built-in pane, its
 *  plugin and id for a Lua pane. Mirrors PaneKey in
 *  src-tauri/src/profile/panes.rs. */
export function paneKey(ref: PaneRef): string {
  if (ref.pane !== LUA_PANE) return ref.pane;
  return `${LUA_PANE}:${JSON.stringify([ref.props.plugin ?? '', ref.props.id ?? ''])}`;
}

/** How many panes the tree keeps with the paneKey of `ref`:
 *  CHAT_PANES_MAX for Chat, one for anything else. Mirrors pane_cap in
 *  src-tauri/src/profile/panes.rs. */
export function paneCap(ref: PaneRef): number {
  return ref.pane === 'chat' ? CHAT_PANES_MAX : 1;
}

/** What React keys the pane's box on: its paneKey for a pane the tree
 *  holds once, and the paneKey with the leaf id for a Chat pane, which
 *  the tree can hold up to CHAT_PANES_MAX times. */
export function leafKey(leaf: PaneLeaf): string {
  const key = paneKey(leaf);
  return paneCap(leaf) === 1 ? key : `${key}#${leaf.id}`;
}

/** The pane header, the --pane-header token. */
export const PANE_HEADER_PX = 28;
/** One dense row, the --row token. */
export const PANE_ROW_PX = 22;

/** The height each pane type needs to be read: Affects its header and
 *  six rows, Group, Staff queues and a Lua pane their header and three
 *  rows, Chat a couple of messages, the Map a drawing you can follow,
 *  and the writing card its header, six rows of text and its footer. */
export const PANE_MIN_H: Record<PaneKind, number> = {
  map: 180,
  affects: PANE_HEADER_PX + 6 * PANE_ROW_PX,
  group: PANE_HEADER_PX + 3 * PANE_ROW_PX,
  stats: PANE_HEADER_PX + 6 * PANE_ROW_PX,
  chat: 120,
  imm: PANE_HEADER_PX + 3 * PANE_ROW_PX,
  writing: 240,
  lua: PANE_HEADER_PX + 3 * PANE_ROW_PX,
};

/** `column` stacks children top to bottom (Split down), `row` sets
 *  them side by side (Split right). */
export type SplitDir = 'row' | 'column';

export interface PaneLeaf {
  /** Stable id to key pane state on. Kept across every operation. */
  id: string;
  pane: PaneKind;
  /** Share of the parent split. Siblings sum to 1. */
  weight: number;
  /** Per-pane settings, such as the chat pane's channel filter, and
   *  the plugin, id and title of a Lua pane. */
  props: Record<string, string>;
}

export interface PaneSplit {
  id: string;
  split: SplitDir;
  weight: number;
  children: PaneNode[];
}

export type PaneNode = PaneLeaf | PaneSplit;

export interface PaneLayout {
  version: number;
  panel_open: boolean;
  /** Panel width in CSS pixels. Null means the stock 300 px. */
  panel_width: number | null;
  /** Always a split. No children means the panel shows only the
   *  pinned vitals. */
  root: PaneSplit;
  /** The sidebar's panes, under the session list. Always a split, with
   *  no children while the sidebar holds none. */
  sidebar: PaneSplit;
  /** The backend's count of profile swaps when it handed out this
   *  tree. Edits keep it, a write sends it back, and the backend
   *  refuses a write made against a profile that has since been
   *  swapped out. Never saved. Absent on a tree the backend did not
   *  hand out. */
  generation?: number;
}

export const PANE_LAYOUT_VERSION = 1;
export const PANEL_WIDTH_MIN = 200;
export const PANEL_WIDTH_MAX = 800;
/** The narrowest panel Windows and Linux draw. The title band carries
 *  the window controls there, and its right buttons reach 238 px in
 *  from the window edge. At this width every one of them sits over the
 *  panel, 10 px in from its edge as at the window edge. A saved width
 *  under this stays saved and draws at this width. */
export const PANEL_WIDTH_MIN_FRAMELESS = 248;

/** The narrowest panel the window draws, PANEL_WIDTH_MIN on macOS. */
export function panelWidthFloor(mac: boolean): number {
  return mac ? PANEL_WIDTH_MIN : PANEL_WIDTH_MIN_FRAMELESS;
}
// Deepest depth a split may sit at, counting the root as 0.
const MAX_SPLIT_DEPTH = 3;
const MAX_WEIGHT = 1_000_000;

export function isLeaf(node: PaneNode): node is PaneLeaf {
  return 'pane' in node;
}

export function isPaneType(value: unknown): value is PaneType {
  return typeof value === 'string' && (PANE_TYPES as readonly string[]).includes(value);
}

/** Map above affects with the panel open and an empty sidebar, the
 *  stock layout. */
export function defaultLayout(): PaneLayout {
  return {
    version: PANE_LAYOUT_VERSION,
    panel_open: true,
    panel_width: null,
    root: defaultRoot(),
    sidebar: emptySidebar(),
  };
}

/** The sidebar with no panes. */
export function emptySidebar(): PaneSplit {
  return { id: 'sidebar', split: 'column', weight: 1, children: [] };
}

// The map's share of the stock layout, over affects. It gives the Map
// pane 348 px and the Affects pane 315 px at 1280 by 800, which is
// room for every Affects row.
const DEFAULT_MAP_WEIGHT = 0.525;
const DEFAULT_AFFECTS_WEIGHT = 0.475;

function defaultRoot(): PaneSplit {
  return {
    id: 'root',
    split: 'column',
    weight: 1,
    children: [
      { id: 'map', pane: 'map', weight: DEFAULT_MAP_WEIGHT, props: {} },
      { id: 'affects', pane: 'affects', weight: DEFAULT_AFFECTS_WEIGHT, props: {} },
    ],
  };
}

// ---------------------------------------------------------------
// Sanitize. Same rules as PaneLayoutPersist::sanitize in Rust, and
// both run against fixtures/pane-layout/sanitize.json.
// ---------------------------------------------------------------

/** A node as read off the wire, before any repair. Missing fields
 *  take the serde defaults. */
interface RawNode {
  id: string;
  pane: string | null;
  split: string | null;
  weight: number;
  children: RawNode[];
  props: Record<string, string>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function readNode(value: unknown): RawNode {
  const o = isRecord(value) ? value : {};
  const props: Record<string, string> = {};
  if (isRecord(o.props)) {
    for (const [k, v] of Object.entries(o.props)) {
      if (typeof v === 'string') props[k] = v;
    }
  }
  return {
    id: typeof o.id === 'string' ? o.id : '',
    pane: typeof o.pane === 'string' ? o.pane : null,
    split: typeof o.split === 'string' ? o.split : null,
    weight: typeof o.weight === 'number' ? o.weight : 1,
    children: Array.isArray(o.children) ? o.children.map(readNode) : [],
    props,
  };
}

interface SanitizeContext {
  /** Every non-blank id in the raw tree, so a fresh id never steals
   *  one a later node already owns. */
  reserved: Set<string>;
  used: Set<string>;
  /** How many leaves of each paneKey are placed so far. */
  panes: Map<string, number>;
  /** Whether the tree being read may hold the writing card, which only
   *  the panel's does. */
  writing: boolean;
}

/** A context for one pass over `roots`, which reserves their ids. */
function sanitizeContext(...roots: RawNode[]): SanitizeContext {
  const reserved = new Set<string>();
  const collect = (node: RawNode) => {
    if (node.id.trim().length > 0) reserved.add(node.id);
    node.children.forEach(collect);
  };
  roots.forEach(collect);
  return { reserved, used: new Set(), panes: new Map(), writing: true };
}

function claimId(ctx: SanitizeContext, raw: string, base: string): string {
  if (raw.trim().length > 0 && !ctx.used.has(raw)) {
    ctx.used.add(raw);
    return raw;
  }
  for (let n = 1; ; n += 1) {
    const candidate = n === 1 ? base : `${base}-${n}`;
    if (!ctx.used.has(candidate) && !ctx.reserved.has(candidate)) {
      ctx.used.add(candidate);
      return candidate;
    }
  }
}

function paneType(raw: string): PaneKind | null {
  const wanted = raw.trim().toLowerCase();
  if (wanted === LUA_PANE) return LUA_PANE;
  return PANE_TYPES.find((t) => t === wanted) ?? null;
}

const blank = (value: string | undefined) => (value ?? '').trim().length === 0;

function splitDir(raw: string | null): SplitDir {
  return raw !== null && raw.trim().toLowerCase() === 'row' ? 'row' : 'column';
}

function cleanWeight(w: number): number {
  return Number.isFinite(w) && w > 0 ? Math.min(w, MAX_WEIGHT) : 1;
}

// Props sorted by key, matching the BTreeMap order Rust sends back, so
// a round trip compares equal.
function sortedProps(props: Record<string, string>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const key of Object.keys(props).sort()) out[key] = props[key];
  return out;
}

// Scale sibling weights to sum to 1 unless they already do (within
// 0.001, which keeps a second pass from nudging rounded values), then
// round each to four places with a floor of 0.0001.
function normalizeWeights(nodes: PaneNode[]): PaneNode[] {
  const sum = nodes.reduce((acc, n) => acc + n.weight, 0);
  const scale = sum > 0 && Math.abs(sum - 1) > 1e-3;
  return nodes.map((n) => {
    const w = scale ? n.weight / sum : n.weight;
    return { ...n, weight: Math.max(Math.round(w * 10_000) / 10_000, 0.0001) };
  });
}

function collectLeaves(nodes: PaneNode[]): PaneLeaf[] {
  const out: PaneLeaf[] = [];
  for (const node of nodes) {
    if (isLeaf(node)) out.push(node);
    else out.push(...collectLeaves(node.children));
  }
  return out;
}

function sanitizeNode(ctx: SanitizeContext, raw: RawNode, depth: number): PaneNode | null {
  const weight = cleanWeight(raw.weight);
  if (raw.pane !== null) {
    const kind = paneType(raw.pane);
    if (kind === null) return null;
    if (kind === WRITING_PANE && !ctx.writing) return null;
    if (kind === LUA_PANE && (blank(raw.props.plugin) || blank(raw.props.id))) return null;
    const ref = { pane: kind, props: raw.props };
    const key = paneKey(ref);
    const placed = ctx.panes.get(key) ?? 0;
    if (placed >= paneCap(ref)) return null;
    ctx.panes.set(key, placed + 1);
    return { id: claimId(ctx, raw.id, kind), pane: kind, weight, props: sortedProps(raw.props) };
  }
  const dir = splitDir(raw.split);
  const id = claimId(ctx, raw.id, 'split');
  const children = sanitizeChildren(ctx, dir, raw.children, depth);
  if (children.length > 1) return { id, split: dir, weight, children };
  const only = children[0];
  return only ? { ...only, weight } : null;
}

function sanitizeChildren(
  ctx: SanitizeContext,
  dir: SplitDir,
  raw: RawNode[],
  depth: number,
): PaneNode[] {
  const out: PaneNode[] = [];
  for (const child of raw) {
    const node = sanitizeNode(ctx, child, depth + 1);
    if (node === null) continue;
    if (isLeaf(node)) {
      out.push(node);
    } else if (node.split === dir) {
      // Same direction as this split: lift the grandchildren, whose
      // shares already sum to 1 inside the child.
      for (const grandchild of node.children) {
        out.push({ ...grandchild, weight: grandchild.weight * node.weight });
      }
    } else if (depth + 1 > MAX_SPLIT_DEPTH) {
      const leaves = collectLeaves(node.children);
      const share = node.weight / leaves.length;
      for (const leaf of leaves) out.push({ ...leaf, weight: share });
    } else {
      out.push(node);
    }
  }
  return normalizeWeights(out);
}

/** Repair a pane tree from disk, the wire, or a hand edit. Unknown
 *  pane types, panes past their paneCap and Lua panes without a plugin
 *  or an id
 *  drop out, blank or clashing ids get
 *  fresh ones, a split with one child gives way to that child, a split
 *  inside a split of the same direction merges into it, splits nested
 *  past depth three flatten, weights become positive shares that sum
 *  to 1 (four places), and the root is always a split. */
export function sanitize(tree: unknown): PaneSplit {
  const raw = readNode(tree);
  return sanitizeRoot(sanitizeContext(raw), raw, 'root');
}

// One tree's root in `ctx`, which may already hold the panes and ids of
// another tree.
function sanitizeRoot(ctx: SanitizeContext, rawRoot: RawNode, base: string): PaneSplit {
  // A bare leaf at the root gets wrapped so the root stays a split.
  const raw: RawNode =
    rawRoot.pane !== null
      ? { id: '', pane: null, split: 'column', weight: 1, children: [rawRoot], props: {} }
      : rawRoot;
  let dir = splitDir(raw.split);
  const id = claimId(ctx, raw.id, base);
  let children = sanitizeChildren(ctx, dir, raw.children, 0);
  // A lone split under the root takes its place, keeping the root id.
  const only = children.length === 1 ? children[0] : undefined;
  if (only && !isLeaf(only)) {
    dir = only.split;
    children = only.children;
  }
  return { id, split: dir, weight: 1, children };
}

/** Repair a whole layout. Missing fields take their defaults, the
 *  width clamps to the panel bounds, and each tree goes through the
 *  rules of {@link sanitize}, the sidebar's after the panel's: a pane
 *  the panel shows drops out of the sidebar, an id stays unique across
 *  both, and the writing card never sits in the sidebar. */
export function sanitizeLayout(raw: unknown): PaneLayout {
  const o = isRecord(raw) ? raw : {};
  const width =
    typeof o.panel_width === 'number' && Number.isFinite(o.panel_width)
      ? Math.min(PANEL_WIDTH_MAX, Math.max(PANEL_WIDTH_MIN, Math.round(o.panel_width)))
      : null;
  const generation =
    typeof o.generation === 'number' && Number.isSafeInteger(o.generation) && o.generation >= 0
      ? { generation: o.generation }
      : {};
  return {
    version: PANE_LAYOUT_VERSION,
    panel_open: typeof o.panel_open === 'boolean' ? o.panel_open : true,
    panel_width: width,
    ...sanitizeTrees(o.root, o.sidebar),
    ...generation,
  };
}

// The panel's tree, the stock one when none came, and the sidebar's,
// empty when none came, in one pass.
function sanitizeTrees(root: unknown, sidebar: unknown): Pick<PaneLayout, 'root' | 'sidebar'> {
  const rawRoot = readNode(root === undefined || root === null ? defaultRoot() : root);
  const rawSide = readNode(sidebar === undefined || sidebar === null ? emptySidebar() : sidebar);
  const ctx = sanitizeContext(rawRoot, rawSide);
  const cleanRoot = sanitizeRoot(ctx, rawRoot, 'root');
  ctx.writing = false;
  return { root: cleanRoot, sidebar: sanitizeRoot(ctx, rawSide, 'sidebar') };
}

// ---------------------------------------------------------------
// Tree operations. Pure: inputs are never mutated, ids of untouched
// nodes never change, and a call that changes nothing returns the
// input tree itself.
// ---------------------------------------------------------------

function walk(node: PaneNode, visit: (n: PaneNode) => void): void {
  visit(node);
  if (!isLeaf(node)) for (const child of node.children) walk(child, visit);
}

export function findNode(tree: PaneNode, id: string): PaneNode | null {
  if (tree.id === id) return tree;
  if (isLeaf(tree)) return null;
  for (const child of tree.children) {
    const hit = findNode(child, id);
    if (hit !== null) return hit;
  }
  return null;
}

/** The id of the first leaf in reading order showing `ref`, or null
 *  when the panel does not show it. */
export function leafIdFor(node: PaneNode, ref: PaneRef): string | null {
  const key = paneKey(ref);
  let hit: string | null = null;
  walk(node, (n) => {
    if (hit === null && isLeaf(n) && paneKey(n) === key) hit = n.id;
  });
  return hit;
}

/** The paneKey of every pane in the tree, in reading order. A built-in
 *  pane's key is its type. */
export function allPanes(tree: PaneSplit): string[] {
  const out: string[] = [];
  walk(tree, (n) => {
    if (isLeaf(n)) out.push(paneKey(n));
  });
  return out;
}

/** How many leaves of the tree show `ref`. */
export function countPanes(tree: PaneNode, ref: PaneRef): number {
  const key = paneKey(ref);
  let count = 0;
  walk(tree, (n) => {
    if (isLeaf(n) && paneKey(n) === key) count += 1;
  });
  return count;
}

// The tree to place `ref` in. A pane the tree holds once leaves the
// spot it shows in, so placing it moves it. A Chat pane stays where it
// is and the new one joins it, unless CHAT_PANES_MAX already show, which
// gives null.
function roomFor(tree: PaneSplit, ref: PaneRef): PaneSplit | null {
  const cap = paneCap(ref);
  if (cap === 1) {
    const key = paneKey(ref);
    return removeWhere(tree, (n) => isLeaf(n) && paneKey(n) === key);
  }
  return countPanes(tree, ref) < cap ? tree : null;
}

/** `base` if no node uses it yet, else `base-2`, `base-3`, and so on. */
function freshId(tree: PaneSplit, base: string): string {
  const ids = new Set<string>();
  walk(tree, (n) => ids.add(n.id));
  if (!ids.has(base)) return base;
  for (let n = 2; ; n += 1) {
    const candidate = `${base}-${n}`;
    if (!ids.has(candidate)) return candidate;
  }
}

// A new leaf showing `ref`, with an id `tree` does not use yet, such as
// chat-2 beside a chat.
function freshLeaf(tree: PaneSplit, ref: PaneRef): PaneLeaf {
  return { id: freshId(tree, ref.pane), pane: ref.pane, weight: 1, props: ref.props };
}

// Drop every non-root node matching `pred`. The result may hold empty
// or one child splits, which the caller's sanitize pass cleans up.
function removeWhere(node: PaneSplit, pred: (n: PaneNode) => boolean): PaneSplit {
  return {
    ...node,
    children: node.children
      .filter((c) => !pred(c))
      .map((c) => (isLeaf(c) ? c : removeWhere(c, pred))),
  };
}

// Replace the node with `id` by `fn(node)`.
function mapNode(node: PaneNode, id: string, fn: (n: PaneNode) => PaneNode): PaneNode {
  if (node.id === id) return fn(node);
  if (isLeaf(node)) return node;
  return { ...node, children: node.children.map((c) => mapNode(c, id, fn)) };
}

function mapTree(tree: PaneSplit, id: string, fn: (n: PaneNode) => PaneNode): PaneSplit {
  const out = mapNode(tree, id, fn);
  return isLeaf(out) ? tree : out;
}

/** Split the node `id` and put `ref` after it: to its right for
 *  `row`, below it for `column`. Inside a split of the same direction
 *  the new pane becomes a sibling and the two halve the old share.
 *  A pane the tree holds once moves here from where it shows. Another
 *  Chat pane joins the ones shown, and none comes once
 *  CHAT_PANES_MAX show. */
export function splitPane(tree: PaneSplit, id: string, dir: SplitDir, ref: PaneRef): PaneSplit {
  const target = findNode(tree, id);
  if (target === null || id === tree.id) return tree;
  if (paneCap(ref) === 1 && isLeaf(target) && paneKey(target) === paneKey(ref)) return tree;
  const base = roomFor(tree, ref);
  if (base === null) return tree;
  const fresh = freshLeaf(base, ref);
  const splitId = freshId(base, 'split');
  const insert = (node: PaneSplit): PaneSplit => {
    const at = node.children.findIndex((c) => c.id === id);
    if (at < 0) {
      return { ...node, children: node.children.map((c) => (isLeaf(c) ? c : insert(c))) };
    }
    const hit = node.children[at];
    const children = node.children.slice();
    if (node.split === dir) {
      const half = hit.weight / 2;
      children.splice(at, 1, { ...hit, weight: half }, { ...fresh, weight: half });
    } else {
      children[at] = {
        id: splitId,
        split: dir,
        weight: hit.weight,
        children: [{ ...hit, weight: 1 }, fresh],
      };
    }
    return { ...node, children };
  };
  return sanitize(insert(base));
}

/** Remove the node `id`. Its siblings grow to fill the space, and
 *  closing the root empties the panel. */
export function closePane(tree: PaneSplit, id: string): PaneSplit {
  if (id === tree.id) return tree.children.length === 0 ? tree : { ...tree, children: [] };
  if (findNode(tree, id) === null) return tree;
  return sanitize(removeWhere(tree, (n) => n.id === id));
}

/** Show `ref` in the leaf `id` instead of what it shows now (Show
 *  here instead). The leaf keeps its id and share and starts with the
 *  props of `ref`. A pane the tree holds once moves here from where it
 *  shows. Other Chat panes stay, and a leaf already showing Chat, or a
 *  tree with CHAT_PANES_MAX of them, is left alone. */
export function replacePane(tree: PaneSplit, id: string, ref: PaneRef): PaneSplit {
  const target = findNode(tree, id);
  if (target === null || !isLeaf(target) || paneKey(target) === paneKey(ref)) return tree;
  const base = roomFor(tree, ref);
  if (base === null) return tree;
  return sanitize(
    mapTree(base, id, (n) => ({ id: n.id, pane: ref.pane, weight: n.weight, props: ref.props })),
  );
}

/** Set the shares of split `parentId`'s children, one weight per
 *  child in order. Any positive numbers work (pixel sizes from a drag
 *  are fine) since they are normalized. A length mismatch or a
 *  non-positive weight leaves the tree alone. */
export function setWeights(tree: PaneSplit, parentId: string, weights: number[]): PaneSplit {
  const parent = findNode(tree, parentId);
  if (parent === null || isLeaf(parent) || parent.children.length !== weights.length) return tree;
  if (!weights.every((w) => Number.isFinite(w) && w > 0)) return tree;
  return sanitize(
    mapTree(tree, parentId, (n) =>
      isLeaf(n) ? n : { ...n, children: n.children.map((c, i) => ({ ...c, weight: weights[i] })) },
    ),
  );
}

/** Add `ref` at the bottom of the panel (Add a pane). Its share
 *  stands to the panes already there as its reading height stands to
 *  theirs, so a short list such as Group takes a short share and the
 *  panes above keep their rows. An even share would squeeze Affects to
 *  its minimum and leave Group half empty. A row root nests under a new
 *  column so the pane still lands at the bottom. A pane at its paneCap
 *  leaves the tree alone. */
export function addPane(tree: PaneSplit, ref: PaneRef): PaneSplit {
  if (countPanes(tree, ref) >= paneCap(ref)) return tree;
  return appendLeaf(tree, freshLeaf(tree, ref));
}

/** Put `leaf` at the foot of `tree`, with the share addPane gives a new
 *  pane, keeping its id and props. */
export function appendLeaf(tree: PaneSplit, leaf: PaneLeaf): PaneSplit {
  const fresh = { ...leaf, weight: 1 };
  if (tree.children.length === 0) {
    return sanitize({ ...tree, split: 'column', children: [fresh] });
  }
  const above: PaneNode[] =
    tree.split === 'column' ? tree.children : [{ ...tree, id: freshId(tree, 'split'), weight: 1 }];
  const held = above.reduce((acc, c) => acc + c.weight, 0);
  const read = above.reduce((acc, c) => acc + readingHeight(c), 0);
  const weight =
    held > 0 && read > 0 ? (held * PANE_MIN_H[leaf.pane]) / read : 1 / Math.max(1, above.length);
  return sanitize({
    id: tree.id,
    split: 'column',
    weight: 1,
    children: [...above, { ...fresh, weight }],
  });
}

// The least height `node` reads at in a column: a pane's minimum, the
// sum over a column, and the tallest pane of a row.
function readingHeight(node: PaneNode): number {
  if (isLeaf(node)) return PANE_MIN_H[node.pane];
  const parts = node.children.map(readingHeight);
  if (parts.length === 0) return 0;
  return node.split === 'column' ? parts.reduce((acc, p) => acc + p, 0) : Math.max(...parts);
}

// ---------------------------------------------------------------
// The panel and the sidebar together.
// ---------------------------------------------------------------

/** The two trees a layout holds. */
export type PaneTree = 'panel' | 'sidebar';

/** The tree `which` of `layout`. */
export function treeIn(layout: Pick<PaneLayout, 'root' | 'sidebar'>, which: PaneTree): PaneSplit {
  return which === 'panel' ? layout.root : layout.sidebar;
}

/** `layout` with `tree` as its tree `which`, the same layout when it
 *  already is. */
export function withTree(layout: PaneLayout, which: PaneTree, tree: PaneSplit): PaneLayout {
  if (treeIn(layout, which) === tree) return layout;
  return which === 'panel' ? { ...layout, root: tree } : { ...layout, sidebar: tree };
}

/** Both trees under one split, to read across them, such as which
 *  panes show anywhere or how many Chat panes there are. Never saved
 *  and never edited. */
export function bothTrees(layout: Pick<PaneLayout, 'root' | 'sidebar'>): PaneSplit {
  return { id: '', split: 'column', weight: 1, children: [layout.root, layout.sidebar] };
}

/** The tree that holds the pane `id`, or null when neither does. A
 *  tree's own root counts as in it. */
export function treeOf(layout: Pick<PaneLayout, 'root' | 'sidebar'>, id: string): PaneTree | null {
  if (findNode(layout.root, id) !== null) return 'panel';
  if (findNode(layout.sidebar, id) !== null) return 'sidebar';
  return null;
}

/** The other tree. */
export function otherTree(which: PaneTree): PaneTree {
  return which === 'panel' ? 'sidebar' : 'panel';
}

/** The side of a pane another lands against: above it or below it,
 *  or to its left or right. */
export type DropEdge = 'before' | 'after' | 'left' | 'right';

/** Where a dragged pane lands: next to the pane `at` in tree `tree`,
 *  on the side `edge` names, or alone in an empty tree when `at` is
 *  null. */
export interface PaneDrop {
  tree: PaneTree;
  at: string | null;
  edge: DropEdge;
}

/** Put `leaf` against the pane `id` in `tree` on the side `edge` names.
 *  In a split of the edge's direction, a column for above and below and
 *  a row for left and right, the two halve the old share. In the other
 *  direction the pane becomes a split of the two. With `id` null or not
 *  found, `leaf` goes at the foot. */
export function insertBeside(
  tree: PaneSplit,
  id: string | null,
  leaf: PaneLeaf,
  edge: DropEdge,
): PaneSplit {
  if (id === null || id === tree.id || findNode(tree, id) === null) return appendLeaf(tree, leaf);
  const dir: SplitDir = edge === 'left' || edge === 'right' ? 'row' : 'column';
  const first = edge === 'before' || edge === 'left';
  const insert = (node: PaneSplit): PaneSplit => {
    const at = node.children.findIndex((c) => c.id === id);
    if (at < 0) {
      return { ...node, children: node.children.map((c) => (isLeaf(c) ? c : insert(c))) };
    }
    const hit = node.children[at];
    const children = node.children.slice();
    if (node.split === dir) {
      const half = hit.weight / 2;
      const pair = [
        { ...hit, weight: half },
        { ...leaf, weight: half },
      ];
      children.splice(at, 1, ...(first ? pair.reverse() : pair));
    } else {
      const pair = [
        { ...hit, weight: 1 },
        { ...leaf, weight: 1 },
      ];
      children[at] = {
        id: freshId(tree, 'split'),
        split: dir,
        weight: hit.weight,
        children: first ? pair.reverse() : pair,
      };
    }
    return { ...node, children };
  };
  return sanitize(insert(tree));
}

/** A new leaf showing `ref`, with an id neither tree of `layout` uses
 *  yet, such as chat-2 beside a chat. */
export function newLeaf(layout: Pick<PaneLayout, 'root' | 'sidebar'>, ref: PaneRef): PaneLeaf {
  return freshLeaf(bothTrees(layout), ref);
}

/** Take the pane `id` out of `layout`, wherever it shows, and hand back
 *  the layout without it, the leaf with its id and props, and the tree
 *  it left. Null when no tree holds it. */
export function liftLeaf(
  layout: PaneLayout,
  id: string,
): { layout: PaneLayout; leaf: PaneLeaf; from: PaneTree } | null {
  const from = treeOf(layout, id);
  if (from === null) return null;
  const leaf = findNode(treeIn(layout, from), id);
  if (leaf === null || !isLeaf(leaf)) return null;
  return { layout: withTree(layout, from, closePane(treeIn(layout, from), id)), leaf, from };
}

/** Put `leaf`, which no tree of `layout` holds, where `drop` says. */
export function placeLeaf(layout: PaneLayout, leaf: PaneLeaf, drop: PaneDrop): PaneLayout {
  const placed = withTree(
    layout,
    drop.tree,
    insertBeside(treeIn(layout, drop.tree), drop.at, leaf, drop.edge),
  );
  return { ...placed, ...sanitizeTrees(placed.root, placed.sidebar) };
}

/** Move the pane `id` to where `drop` says, from either tree to either
 *  tree. It keeps its id and props, so a Chat pane keeps its channel.
 *  The writing card stays where it is, as does a drop on the pane
 *  itself or on a pane that has gone. */
export function moveLeaf(layout: PaneLayout, id: string, drop: PaneDrop): PaneLayout {
  const from = treeOf(layout, id);
  if (from === null || drop.at === id) return layout;
  const leaf = findNode(treeIn(layout, from), id);
  if (leaf === null || !isLeaf(leaf) || leaf.pane === WRITING_PANE) return layout;
  if (drop.at !== null && treeOf(layout, drop.at) !== drop.tree) return layout;
  const lifted = withTree(
    layout,
    from,
    removeWhere(treeIn(layout, from), (n) => n.id === id),
  );
  const placed = withTree(
    lifted,
    drop.tree,
    insertBeside(treeIn(lifted, drop.tree), drop.at, leaf, drop.edge),
  );
  return { ...placed, ...sanitizeTrees(placed.root, placed.sidebar) };
}

/** Move the pane `id` to the foot of the other tree, as Move to panel
 *  and Move to sidebar do. */
export function moveToOtherTree(layout: PaneLayout, id: string): PaneLayout {
  const from = treeOf(layout, id);
  if (from === null) return layout;
  return moveLeaf(layout, id, { tree: otherTree(from), at: null, edge: 'after' });
}

/** Clean both trees of `layout` as one pass, keeping its other fields. */
export function cleanTrees(layout: PaneLayout): PaneLayout {
  return { ...layout, ...sanitizeTrees(layout.root, layout.sidebar) };
}

// ---------------------------------------------------------------
// Persistence.
// ---------------------------------------------------------------

const SAVE_DEBOUNCE_MS = 250;

// A write waiting on its debounce, and writes sent but not answered.
// While either exists, a change event from the backend is most likely
// the echo of an older write of ours, so it is held back and the
// saved tree is fetched once the writes settle.
let pending: PaneLayout | null = null;
let pendingTimer: ReturnType<typeof setTimeout> | null = null;
let inFlight = 0;
let missedRemote = false;
// JSON of the tree this window last sent or delivered, to skip echoes
// that change nothing.
let lastKnown: string | null = null;
const listeners = new Set<(layout: PaneLayout) => void>();
let listening: Promise<void> | null = null;

function deliver(layout: PaneLayout): void {
  const json = JSON.stringify(layout);
  if (json === lastKnown) return;
  lastKnown = json;
  for (const cb of listeners) cb(layout);
}

function busy(): boolean {
  return pending !== null || inFlight > 0;
}

function settle(): void {
  if (busy() || !missedRemote) return;
  missedRemote = false;
  getPaneLayout()
    .then((layout) => {
      if (busy()) missedRemote = true;
      else deliver(layout);
    })
    .catch(() => {
      // The next change event resyncs.
    });
}

function onRemoteLayout(payload: unknown): void {
  if (busy()) {
    missedRemote = true;
    return;
  }
  deliver(sanitizeLayout(payload));
}

// A write still waiting on its debounce was edited from the profile
// that just went away. The backend saved that profile without it before
// the switch, and now refuses it by its generation, so drop it here and
// load the new profile's tree.
function onProfileSwitched(): void {
  if (pendingTimer !== null) clearTimeout(pendingTimer);
  pendingTimer = null;
  pending = null;
  lastKnown = null;
  settle();
}

function ensureListening(): Promise<void> {
  listening ??= Promise.all([
    subscribePaneLayoutChanged((payload) => onRemoteLayout(payload)),
    subscribeProfileSwitched(() => onProfileSwitched()),
  ]).then(
    () => undefined,
    (e: unknown) => {
      listening = null;
      throw e;
    },
  );
  return listening;
}

/** The active profile's pane layout. A profile that never saved one
 *  gets a tree migrated from its old dock layout, or the default. */
export async function getPaneLayout(): Promise<PaneLayout> {
  return sanitizeLayout(await paneLayoutGet());
}

/** Put a profile's panes back to the stock tree, the live profile's
 *  when you name none, and return the new layout (paneLayoutReset). */
export async function resetPaneLayout(profile?: string | null): Promise<PaneLayout> {
  return sanitizeLayout(await paneLayoutReset(profile));
}

/** Give the profile `profile` the trees of `layout`, as a move in
 *  Settings > Characters does, and return its layout as saved. Done in
 *  arrange mode sets `arranged`, so the layout it replaces always joins
 *  the earlier ones. */
export async function setProfileTrees(
  profile: string,
  layout: PaneLayout,
  arranged = false,
): Promise<PaneLayout> {
  return sanitizeLayout(await paneTreesSet(profile, layout, arranged));
}

/** Put back the earlier layout `until` names on the profile `profile`,
 *  or the live profile when you name none, and return the layout as
 *  saved (paneHistoryRestore). */
export async function restoreEarlierLayout(
  profile: string | null,
  until: number,
): Promise<PaneLayout> {
  return sanitizeLayout(await paneHistoryRestore(profile, until));
}

/** Save the active profile's pane layout. Calls within 250 ms coalesce
 *  into one write, so a splitter drag can call this on every move.
 *  Local subscribers are not called back for their own writes. */
export function setPaneLayout(layout: PaneLayout): void {
  void ensureListening().catch(() => {});
  pending = layout;
  if (pendingTimer !== null) clearTimeout(pendingTimer);
  pendingTimer = setTimeout(() => {
    pendingTimer = null;
    flushPaneLayout().catch((e: unknown) => console.error('[paneLayout] save failed', e));
  }, SAVE_DEBOUNCE_MS);
}

// A write waiting on its debounce goes at once when the Settings window
// closes over a Width you just typed, and when Vosh quits.
pendingWrites.register(flushPaneLayout);

/** Send a debounced write now. Resolves once the backend has it, and
 *  rejects if the write fails. */
export async function flushPaneLayout(): Promise<void> {
  if (pendingTimer !== null) clearTimeout(pendingTimer);
  pendingTimer = null;
  const layout = pending;
  pending = null;
  if (layout === null) return;
  const clean = sanitizeLayout(layout);
  lastKnown = JSON.stringify(clean);
  inFlight += 1;
  try {
    const applied = await paneLayoutSet({
      layout: clean,
      generation: clean.generation ?? null,
    });
    // Refused: the tree was edited from a profile that has been swapped
    // out since. Read the current one.
    if (applied === false) {
      lastKnown = null;
      missedRemote = true;
    }
  } finally {
    inFlight -= 1;
    settle();
  }
}

/** Take a tree this window got back from a command of its own, such as
 *  a reset, the way a tree from another window lands: at once when no
 *  write of ours is waiting, else once our writes settle. The backend's
 *  broadcast of the same tree then changes nothing. */
export function acceptPaneLayout(layout: unknown): void {
  onRemoteLayout(layout);
}

/** Hear pane layouts that change outside this window's own writes: a
 *  profile switch, or another window editing the tree. */
export async function subscribePaneLayout(cb: (layout: PaneLayout) => void): Promise<UnlistenFn> {
  const entry = (layout: PaneLayout) => cb(layout);
  listeners.add(entry);
  try {
    await ensureListening();
  } catch (e) {
    listeners.delete(entry);
    throw e;
  }
  return () => {
    listeners.delete(entry);
  };
}
