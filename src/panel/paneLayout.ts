import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  paneLayoutGet,
  paneLayoutReset,
  paneLayoutSet,
  subscribePaneLayoutChanged,
} from '../ipc/panes';
import { subscribeProfileSwitched } from '../ipc/profiles';
import { pendingWrites } from '../lib/pendingWrites';

// The one-window panel's pane tree, saved per profile. Mirrors
// PaneLayoutPersist and PaneNode in src-tauri/src/profile/panes.rs.
//
// The backend owns the saved copy. getPaneLayout reads the active
// profile's tree, setPaneLayout writes one (debounced for splitter
// drags), and subscribePaneLayout hears trees that change somewhere
// else, which in practice means a profile switch. The tree operations
// are pure and always return a sanitized tree, so the panel can keep
// the result in state and hand it straight back to setPaneLayout.

/** The built-in content a pane can show. The panel holds at most one
 *  of each. */
export const PANE_TYPES = ['map', 'affects', 'group', 'chat', 'imm'] as const;
export type PaneType = (typeof PANE_TYPES)[number];

/** The type of a pane a plugin draws with mud.pane. The panel holds
 *  any number of them, one per `plugin` and `id` in the leaf's props.
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

/** What makes a pane one of a kind in the tree: its type for a
 *  built-in pane, its plugin and id for a Lua pane. Mirrors PaneKey in
 *  src-tauri/src/profile/panes.rs. */
export function paneKey(ref: PaneRef): string {
  if (ref.pane !== LUA_PANE) return ref.pane;
  return `${LUA_PANE}:${JSON.stringify([ref.props.plugin ?? '', ref.props.id ?? ''])}`;
}

/** The pane header, the --pane-header token. */
export const PANE_HEADER_PX = 28;
/** One dense row, the --row token. */
export const PANE_ROW_PX = 22;

/** The height each pane type needs to be read: Affects its header and
 *  six rows, Group, Staff queues and a Lua pane their header and three
 *  rows, Chat a couple of messages, and the Map a drawing you can
 *  follow. */
export const PANE_MIN_H: Record<PaneKind, number> = {
  map: 180,
  affects: PANE_HEADER_PX + 6 * PANE_ROW_PX,
  group: PANE_HEADER_PX + 3 * PANE_ROW_PX,
  chat: 120,
  imm: PANE_HEADER_PX + 3 * PANE_ROW_PX,
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

/** Map above affects with the panel open, the stock layout. */
export function defaultLayout(): PaneLayout {
  return {
    version: PANE_LAYOUT_VERSION,
    panel_open: true,
    panel_width: null,
    root: defaultRoot(),
  };
}

// The map's share of the stock layout, over affects. The approved
// boards give the Map pane 348 px and the Affects pane 315 px at 1280
// by 800, which shows every Affects row the boards show.
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
  /** The paneKey of every leaf placed so far. */
  panes: Set<string>;
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
    if (kind === LUA_PANE && (blank(raw.props.plugin) || blank(raw.props.id))) return null;
    const key = paneKey({ pane: kind, props: raw.props });
    if (ctx.panes.has(key)) return null;
    ctx.panes.add(key);
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
 *  pane types, repeat panes and Lua panes without a plugin or an id
 *  drop out, blank or clashing ids get
 *  fresh ones, a split with one child gives way to that child, a split
 *  inside a split of the same direction merges into it, splits nested
 *  past depth three flatten, weights become positive shares that sum
 *  to 1 (four places), and the root is always a split. */
export function sanitize(tree: unknown): PaneSplit {
  const rawRoot = readNode(tree);
  const reserved = new Set<string>();
  const collect = (node: RawNode) => {
    if (node.id.trim().length > 0) reserved.add(node.id);
    node.children.forEach(collect);
  };
  collect(rawRoot);
  const ctx: SanitizeContext = { reserved, used: new Set(), panes: new Set() };

  // A bare leaf at the root gets wrapped so the root stays a split.
  const raw: RawNode =
    rawRoot.pane !== null
      ? { id: '', pane: null, split: 'column', weight: 1, children: [rawRoot], props: {} }
      : rawRoot;
  let dir = splitDir(raw.split);
  const id = claimId(ctx, raw.id, 'root');
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
 *  width clamps to the panel bounds, and the tree goes through
 *  {@link sanitize}. */
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
    root: o.root === undefined || o.root === null ? defaultRoot() : sanitize(o.root),
    ...generation,
  };
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

/** The id of the leaf showing `ref`, or null when the panel does not
 *  show it. */
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

// Drop the leaf showing `ref`, so the tree can place it somewhere else.
function withoutPane(tree: PaneSplit, ref: PaneRef): PaneSplit {
  const key = paneKey(ref);
  return removeWhere(tree, (n) => isLeaf(n) && paneKey(n) === key);
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

// A new leaf showing `ref`, with an id `tree` does not use yet.
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
 *  A pane already shown elsewhere moves here. */
export function splitPane(tree: PaneSplit, id: string, dir: SplitDir, ref: PaneRef): PaneSplit {
  const target = findNode(tree, id);
  if (target === null || id === tree.id) return tree;
  if (isLeaf(target) && paneKey(target) === paneKey(ref)) return tree;
  const base = withoutPane(tree, ref);
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
 *  props of `ref`. A pane already shown elsewhere moves here. */
export function replacePane(tree: PaneSplit, id: string, ref: PaneRef): PaneSplit {
  const target = findNode(tree, id);
  if (target === null || !isLeaf(target) || paneKey(target) === paneKey(ref)) return tree;
  const base = withoutPane(tree, ref);
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
 *  column so the pane still lands at the bottom. A pane already shown
 *  leaves the tree alone. */
export function addPane(tree: PaneSplit, ref: PaneRef): PaneSplit {
  if (leafIdFor(tree, ref) !== null) return tree;
  const fresh = freshLeaf(tree, ref);
  if (tree.children.length === 0) {
    return sanitize({ ...tree, split: 'column', children: [fresh] });
  }
  const above: PaneNode[] =
    tree.split === 'column' ? tree.children : [{ ...tree, id: freshId(tree, 'split'), weight: 1 }];
  const held = above.reduce((acc, c) => acc + c.weight, 0);
  const read = above.reduce((acc, c) => acc + readingHeight(c), 0);
  const weight =
    held > 0 && read > 0 ? (held * PANE_MIN_H[ref.pane]) / read : 1 / Math.max(1, above.length);
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
