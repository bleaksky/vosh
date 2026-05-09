// Docking module — Phases 2-4.
//
// At rest this module touches no DOM. The `useDockable` hook attaches
// pointer listeners to a target element and detects drag intent
// (pointerdown plus 5px movement). Below threshold the listeners
// neither preventDefault nor stopPropagation, so click and hover
// behavior on the target's contents stays intact. Above threshold the
// hook publishes drag state to a singleton store that the
// DockingOverlay subscribes to.
//
// On drop into an active zone the bar gets `position: fixed` plus a
// computed left/top/width via inline styles, anchored to the dock
// root (.middle) rect. Drop outside any zone or Escape leaves the
// bar's current docked state unchanged. The layout is persisted to
// localStorage under `client_dock_layout_v1` and restored on load.

import { useEffect, type RefObject } from 'react';

const DRAG_THRESHOLD_PX = 5;

// CSS selector for the dock root. Phase 0 picked `.middle` because it
// already has `position: relative` and its rect equals the natural
// play area between the connect bar and the bottom rail.
export const DOCK_ROOT_SELECTOR = '.middle';

// Snap zone trigger distances. Spec values: 60px corner, 40px edge.
const CORNER_PX = 60;
const EDGE_PX = 40;

// Selector for child elements that own clicks. Pointerdowns landing
// on these are ignored by the docking hook so the existing handlers
// (button clicks, text input, canvas pans) continue to work.
const INTERACTIVE_SELECTOR =
  'button, input, textarea, select, a, label, canvas, [role="tab"], [role="button"]';

export type SnapZone =
  | 'top-left'
  | 'top'
  | 'top-right'
  | 'left'
  | 'right'
  | 'bottom-left'
  | 'bottom'
  | 'bottom-right';

export interface RectXY {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

export interface DragState {
  id: string;
  sourceRect: { left: number; top: number; width: number; height: number };
  startCursor: { x: number; y: number };
  cursor: { x: number; y: number };
  rootRect: RectXY | null;
  zone: SnapZone | null;
}

function snapshotRoot(): RectXY | null {
  const el = document.querySelector(DOCK_ROOT_SELECTOR);
  if (!el) return null;
  const r = el.getBoundingClientRect();
  return {
    left: r.left,
    top: r.top,
    right: r.right,
    bottom: r.bottom,
    width: r.width,
    height: r.height,
  };
}

// Read the dock root's current inline padding so the zone trigger
// areas can extend over already-docked bars (so users can stack a
// new bar on an occupied edge).
function getDockRootPadding(): {
  top: number;
  right: number;
  bottom: number;
  left: number;
} {
  const root = document.querySelector(DOCK_ROOT_SELECTOR) as HTMLElement | null;
  if (!root) return { top: 0, right: 0, bottom: 0, left: 0 };
  return {
    top: parseFloat(root.style.paddingTop) || 0,
    right: parseFloat(root.style.paddingRight) || 0,
    bottom: parseFloat(root.style.paddingBottom) || 0,
    left: parseFloat(root.style.paddingLeft) || 0,
  };
}

// The side panel sits on the right of `.middle` when open. Extend
// the right edge zone over its width so users can aim at the
// visually obvious "right area" instead of having to hit the very
// last 40px of the viewport.
function getSidePanelWidth(): number {
  const el = document.querySelector('.side-panel') as HTMLElement | null;
  if (!el) return 0;
  const r = el.getBoundingClientRect();
  return r.width > 0 ? r.width : 0;
}

export function computeZone(
  cursor: { x: number; y: number },
  root: RectXY,
): SnapZone | null {
  // Outside the dock root => no zone. Bars dragged over the connect
  // bar or the bottom rail intentionally don't snap.
  if (
    cursor.x < root.left ||
    cursor.x > root.right ||
    cursor.y < root.top ||
    cursor.y > root.bottom
  ) {
    return null;
  }

  const dl = cursor.x - root.left;
  const dr = root.right - cursor.x;
  const dt = cursor.y - root.top;
  const db = root.bottom - cursor.y;

  // Compute extended edge thresholds so already-docked bars and the
  // side panel are inside their respective edge zones. This lets the
  // user stack a new bar onto an occupied edge by dropping anywhere
  // over the existing dock area, instead of needing to hit a thin
  // strip outside it.
  const padding = getDockRootPadding();
  const sidePanelW = getSidePanelWidth();
  const leftEdge = EDGE_PX + padding.left;
  const rightEdge = EDGE_PX + padding.right + sidePanelW;
  const topEdge = EDGE_PX + padding.top;
  const bottomEdge = EDGE_PX + padding.bottom;

  // Corners stay at the canonical CORNER_PX trigger square so users
  // can still hit a corner explicitly when an edge is otherwise
  // dominant. Corners take precedence: cursor must be within
  // CORNER_PX of both adjacent edges.
  const inL = dl < CORNER_PX;
  const inR = dr < CORNER_PX;
  const inT = dt < CORNER_PX;
  const inB = db < CORNER_PX;
  if (inT && inL) return 'top-left';
  if (inT && inR) return 'top-right';
  if (inB && inL) return 'bottom-left';
  if (inB && inR) return 'bottom-right';

  // Edges: within their respective extended threshold, and (since
  // corners are already handled) not in a corner zone. If multiple
  // edges qualify, pick the closest.
  let best: SnapZone | null = null;
  let bestDist = Infinity;
  if (dl < leftEdge && dl < bestDist) {
    best = 'left';
    bestDist = dl;
  }
  if (dr < rightEdge && dr < bestDist) {
    best = 'right';
    bestDist = dr;
  }
  if (dt < topEdge && dt < bestDist) {
    best = 'top';
    bestDist = dt;
  }
  if (db < bottomEdge && db < bestDist) {
    best = 'bottom';
    bestDist = db;
  }
  return best;
}

export function previewRect(
  zone: SnapZone,
  root: RectXY,
  source: { width: number; height: number },
): { left: number; top: number; width: number; height: number } {
  const w = source.width;
  const h = source.height;
  switch (zone) {
    case 'top-left':
      return { left: root.left, top: root.top, width: w, height: h };
    case 'top-right':
      return { left: root.right - w, top: root.top, width: w, height: h };
    case 'bottom-left':
      return { left: root.left, top: root.bottom - h, width: w, height: h };
    case 'bottom-right':
      return { left: root.right - w, top: root.bottom - h, width: w, height: h };
    case 'top':
      return {
        left: root.left + (root.width - w) / 2,
        top: root.top,
        width: w,
        height: h,
      };
    case 'bottom':
      return {
        left: root.left + (root.width - w) / 2,
        top: root.bottom - h,
        width: w,
        height: h,
      };
    case 'left':
      return {
        left: root.left,
        top: root.top + (root.height - h) / 2,
        width: w,
        height: h,
      };
    case 'right':
      return {
        left: root.right - w,
        top: root.top + (root.height - h) / 2,
        width: w,
        height: h,
      };
  }
}

type Listener = (state: DragState | null) => void;

class DockingStore {
  private state: DragState | null = null;
  private listeners = new Set<Listener>();

  get(): DragState | null {
    return this.state;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  startDrag(payload: Omit<DragState, 'rootRect' | 'zone'>) {
    const rootRect = snapshotRoot();
    const zone = rootRect ? computeZone(payload.cursor, rootRect) : null;
    this.state = { ...payload, rootRect, zone };
    this.notify();
  }

  updateCursor(cursor: { x: number; y: number }) {
    if (!this.state) return;
    // Re-snapshot the dock root every move so window resizes during a
    // drag are picked up. Cheap: one getBoundingClientRect call.
    const rootRect = snapshotRoot();
    const zone = rootRect ? computeZone(cursor, rootRect) : null;
    this.state = { ...this.state, cursor, rootRect, zone };
    this.notify();
  }

  endDrag() {
    if (!this.state) return;
    this.state = null;
    this.notify();
  }

  private notify() {
    for (const l of this.listeners) l(this.state);
  }
}

export const dockingStore = new DockingStore();

// -- Persistent dock layout ----------------------------------------

const STORAGE_KEY = 'client_dock_layout_v1';

const VALID_ZONES: SnapZone[] = [
  'top-left',
  'top',
  'top-right',
  'left',
  'right',
  'bottom-left',
  'bottom',
  'bottom-right',
];

function isValidZone(value: unknown): value is SnapZone {
  return typeof value === 'string' && (VALID_ZONES as string[]).includes(value);
}

export interface DockEntry {
  id: string;
  zone: SnapZone;
}

class DockLayoutStore {
  // entries are kept in drop order; multiple entries can share a zone
  // and they stack in this order.
  private entries: DockEntry[] = [];
  private listeners = new Set<() => void>();

  constructor() {
    this.load();
  }

  private load() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return;
      const parsed = JSON.parse(raw) as { entries?: unknown };
      if (!Array.isArray(parsed?.entries)) return;
      const valid: DockEntry[] = [];
      for (const e of parsed.entries) {
        if (
          e &&
          typeof e === 'object' &&
          typeof (e as DockEntry).id === 'string' &&
          isValidZone((e as DockEntry).zone)
        ) {
          valid.push({ id: (e as DockEntry).id, zone: (e as DockEntry).zone });
        }
      }
      this.entries = valid;
    } catch {
      // Bad JSON or storage unavailable; ignore and start empty.
    }
  }

  private save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ entries: this.entries }));
    } catch {
      // Storage full or private mode; persistence best-effort.
    }
  }

  getEntries(): readonly DockEntry[] {
    return this.entries;
  }

  getZoneFor(id: string): SnapZone | null {
    return this.entries.find((e) => e.id === id)?.zone ?? null;
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  dock(id: string, zone: SnapZone) {
    const filtered = this.entries.filter((e) => e.id !== id);
    filtered.push({ id, zone });
    this.entries = filtered;
    this.save();
    this.notify();
    scheduleLayout();
  }

  undock(id: string) {
    if (!this.entries.some((e) => e.id === id)) return;
    this.entries = this.entries.filter((e) => e.id !== id);
    this.save();
    this.notify();
    scheduleLayout();
  }

  reset() {
    this.entries = [];
    this.save();
    this.notify();
    scheduleLayout();
  }

  /// Replace the in-memory entries with what came back from the
  /// backend or the layout-editor's broadcast event. Bypasses the
  /// localStorage cache (since the source of truth was external) and
  /// triggers a re-layout. No-op when the entries are byte-identical
  /// so cross-window broadcasts don't loop.
  applyExternal(entries: DockEntry[]) {
    if (
      entries.length === this.entries.length &&
      entries.every((e, i) => e.id === this.entries[i].id && e.zone === this.entries[i].zone)
    ) {
      return;
    }
    this.entries = entries.map((e) => ({ id: e.id, zone: e.zone }));
    this.save();
    this.notify();
    scheduleLayout();
  }

  private notify() {
    for (const l of this.listeners) l();
  }
}

export const dockLayoutStore = new DockLayoutStore();

// -- Layout engine -------------------------------------------------

const refsById = new Map<string, HTMLElement>();
let layoutScheduled = false;

function scheduleLayout() {
  if (layoutScheduled) return;
  layoutScheduled = true;
  queueMicrotask(() => {
    layoutScheduled = false;
    runLayout();
  });
}

// Computes the docked rect for each bar in a zone, using each bar's
// measured rect (already positioned bars report their docked rect;
// newly docked bars report their in-flow rect). The first bar in
// edge zones lands at the centered preview position so single-bar
// drops match Phase 3's preview. Subsequent bars stack:
//   - top/bottom edges: rightward
//   - left/right edges: downward
//   - top corners: downward from corner
//   - bottom corners: upward from corner
function layoutZone(
  zone: SnapZone,
  root: RectXY,
  bars: { id: string; rect: { width: number; height: number } }[],
): Map<string, { left: number; top: number; width: number; height: number }> {
  const out = new Map<string, { left: number; top: number; width: number; height: number }>();
  if (bars.length === 0) return out;

  switch (zone) {
    case 'top':
    case 'bottom': {
      let cursorLeft = 0;
      for (let i = 0; i < bars.length; i++) {
        const { id, rect } = bars[i];
        const left =
          i === 0 ? root.left + (root.width - rect.width) / 2 : cursorLeft;
        const top = zone === 'top' ? root.top : root.bottom - rect.height;
        out.set(id, { left, top, width: rect.width, height: rect.height });
        cursorLeft = left + rect.width;
      }
      return out;
    }
    case 'left':
    case 'right': {
      let cursorTop = 0;
      for (let i = 0; i < bars.length; i++) {
        const { id, rect } = bars[i];
        const left = zone === 'left' ? root.left : root.right - rect.width;
        const top =
          i === 0 ? root.top + (root.height - rect.height) / 2 : cursorTop;
        out.set(id, { left, top, width: rect.width, height: rect.height });
        cursorTop = top + rect.height;
      }
      return out;
    }
    case 'top-left':
    case 'top-right': {
      let cursorTop = root.top;
      for (const { id, rect } of bars) {
        const left = zone === 'top-left' ? root.left : root.right - rect.width;
        out.set(id, { left, top: cursorTop, width: rect.width, height: rect.height });
        cursorTop += rect.height;
      }
      return out;
    }
    case 'bottom-left':
    case 'bottom-right': {
      let cursorBottom = root.bottom;
      for (const { id, rect } of bars) {
        const left =
          zone === 'bottom-left' ? root.left : root.right - rect.width;
        const top = cursorBottom - rect.height;
        out.set(id, { left, top, width: rect.width, height: rect.height });
        cursorBottom = top;
      }
      return out;
    }
  }
}

function runLayout() {
  ensureRootObserver();
  ensureSidePanelObserver();
  const entries = dockLayoutStore.getEntries();
  const dockedIds = new Set<string>();
  // Side-panel sections lock their width to the side panel wrapper's
  // current width, so resizing the side panel resizes them all
  // together. Fallback to 320 if the side panel isn't mounted.
  const sidePanelWidth = getSidePanelWrapperWidth();

  // Pass 1: apply data-docked + position: fixed for all currently
  // docked bars; clear inline styles for any that are no longer
  // docked. Setting the attribute first lets the docking stylesheet
  // (which keys `[data-docked].status-pane` etc. to a fixed width)
  // take effect before pass 2 measures. Once the bar is detached
  // from its parent flex flow the dock root collapses, so pass 2
  // reads the correct dock-root rect.
  for (const entry of entries) {
    const el = refsById.get(entry.id);
    if (!el) continue;
    el.setAttribute('data-docked', entry.zone);
    if (el.style.position !== 'fixed') {
      el.style.position = 'fixed';
      // Stage at origin; pass 2 overwrites with real coords
      // synchronously before paint.
      el.style.left = '0px';
      el.style.top = '0px';
    }
    if (SIDE_PANEL_IDS.includes(entry.id)) {
      // Only right-zone sections track the side panel's width (so the
      // resize handle on the right resizes them together). Left, top,
      // bottom, and corner-on-other-sides sections lock to a fixed
      // 320px default so they don't get dragged around when the user
      // resizes the right column.
      const tracksSidePanel = RIGHT_ZONES.includes(entry.zone);
      el.style.width = tracksSidePanel ? `${sidePanelWidth}px` : '320px';
    }
    el.style.zIndex = '50';
    observeBar(entry.id, el);
    dockedIds.add(entry.id);
  }

  for (const [id, el] of refsById) {
    if (dockedIds.has(id)) continue;
    if (el.hasAttribute('data-docked')) {
      el.removeAttribute('data-docked');
      el.style.position = '';
      el.style.left = '';
      el.style.top = '';
      el.style.width = '';
      el.style.zIndex = '';
    }
    unobserveBar(id);
  }

  // Pass 2: re-snapshot the dock root (now reflecting any flex-flow
  // collapse from pass 1) and compute final coordinates from each
  // bar's now-rendered rect. Width comes from the docking stylesheet
  // for bars that opt in; height is content-driven.
  const root = snapshotRoot();
  if (!root) return;

  const byZone = new Map<
    SnapZone,
    { id: string; el: HTMLElement; rect: { width: number; height: number } }[]
  >();
  for (const entry of entries) {
    const el = refsById.get(entry.id);
    if (!el) continue;
    const r = el.getBoundingClientRect();
    const list = byZone.get(entry.zone) ?? [];
    list.push({ id: entry.id, el, rect: { width: r.width, height: r.height } });
    byZone.set(entry.zone, list);
  }

  for (const [zone, bars] of byZone) {
    const layout = layoutZone(
      zone,
      root,
      bars.map((b) => ({ id: b.id, rect: b.rect })),
    );
    for (const bar of bars) {
      const pos = layout.get(bar.id);
      if (!pos) continue;
      bar.el.style.left = `${pos.left}px`;
      bar.el.style.top = `${pos.top}px`;
    }
  }

  // Pass 3: pad the dock root so its in-flow children (terminal
  // column and side panel) make room for the docked bars. Without
  // this, docked bars sit on top of the terminal and side panel.
  // Padding is applied to the OUTER element via inline style; bars
  // themselves are position: fixed against the outer rect, which is
  // unchanged by inner padding. So bars sit in the padded area and
  // the in-flow children flex within the reduced inner area.
  applyDockRootPadding(byZone);

  // Pass 4: hide the side panel's resize handle when all four
  // side-panel sections are docked elsewhere, since the handle would
  // resize an empty panel.
  applySidePanelHandleVisibility();
}

const SIDE_PANEL_IDS: readonly string[] = [
  'status-pane',
  'affects-pane',
  'map-pane',
  'info-tabs-pane',
];

const RIGHT_ZONES: SnapZone[] = ['right', 'top-right', 'bottom-right'];

function applySidePanelHandleVisibility() {
  // Find the .resizable wrapper that contains .side-panel, then its
  // handle. Other Resizable wrappers (drawers, search) are unaffected.
  const sidePanel = document.querySelector('.side-panel');
  const handle = sidePanel
    ?.closest('.resizable')
    ?.querySelector('.resizable-handle') as HTMLElement | null;
  if (!handle) return;
  // Show the handle if there's anything to resize: any side-panel
  // section still in flow OR any side-panel section docked at a
  // right-side zone (where the handle drives docked-bar width).
  // Hide only when none of those apply.
  const someInFlow = SIDE_PANEL_IDS.some(
    (id) => dockLayoutStore.getZoneFor(id) === null,
  );
  const someAtRight = SIDE_PANEL_IDS.some((id) => {
    const z = dockLayoutStore.getZoneFor(id);
    return z !== null && RIGHT_ZONES.includes(z);
  });
  handle.style.display = someInFlow || someAtRight ? '' : 'none';

  // When all 4 sections are docked AND any are at a right-side zone,
  // applyDockRootPadding skips padding-right so the docked bars
  // overlay the empty side panel area. Bump the handle's z-index so
  // it stays grabbable above the docked bars sharing its x range.
  const allDocked = SIDE_PANEL_IDS.every(
    (id) => dockLayoutStore.getZoneFor(id) !== null,
  );
  if (allDocked && someAtRight) {
    handle.style.zIndex = '60';
    handle.style.position = 'relative';
  } else {
    handle.style.zIndex = '';
    handle.style.position = '';
  }
}

function applyDockRootPadding(
  byZone: Map<
    SnapZone,
    { id: string; el: HTMLElement; rect: { width: number; height: number } }[]
  >,
) {
  const root = document.querySelector(DOCK_ROOT_SELECTOR) as HTMLElement | null;
  if (!root) return;

  let pTop = 0;
  let pBottom = 0;
  let pLeft = 0;
  let pRight = 0;

  for (const [zone, bars] of byZone) {
    let maxW = 0;
    let maxH = 0;
    for (const bar of bars) {
      maxW = Math.max(maxW, bar.rect.width);
      maxH = Math.max(maxH, bar.rect.height);
    }
    switch (zone) {
      case 'top':
        pTop = Math.max(pTop, maxH);
        break;
      case 'bottom':
        pBottom = Math.max(pBottom, maxH);
        break;
      case 'left':
      case 'top-left':
      case 'bottom-left':
        // Corner stacks contribute to the side they're anchored
        // against, not to top/bottom: terminal/side-panel get
        // shifted past the corner bar's width but don't need to
        // make room above or below it.
        pLeft = Math.max(pLeft, maxW);
        break;
      case 'right':
      case 'top-right':
      case 'bottom-right':
        pRight = Math.max(pRight, maxW);
        break;
    }
  }

  // When the side panel is fully empty (all 4 sections docked) and
  // at least one is at a right-side zone, skip padding-right so the
  // docked bars overlay the empty side panel area. This eliminates
  // the gap between the resize handle and the docked stack. The
  // handle gets bumped to a higher z-index in
  // applySidePanelHandleVisibility so it stays clickable above the
  // overlaying docked bars.
  const allDocked = SIDE_PANEL_IDS.every(
    (id) => dockLayoutStore.getZoneFor(id) !== null,
  );
  const anyAtRight = SIDE_PANEL_IDS.some((id) => {
    const z = dockLayoutStore.getZoneFor(id);
    return z !== null && RIGHT_ZONES.includes(z);
  });
  if (allDocked && anyAtRight) {
    pRight = 0;
  }

  // Switch to border-box while padding is active. Without this, the
  // dock root's CSS `width: 100%` is interpreted as content-box, so
  // padding-right: 320 expands the outer box to viewport.width + 320.
  // Bars positioned at root.right - bar.width then sit beyond the
  // visible viewport, looking like they "disappeared". Clearing
  // box-sizing when no padding is needed restores the original CSS.
  const anyPadding = pTop || pBottom || pLeft || pRight;
  root.style.boxSizing = anyPadding ? 'border-box' : '';

  root.style.paddingTop = pTop ? `${pTop}px` : '';
  root.style.paddingBottom = pBottom ? `${pBottom}px` : '';
  root.style.paddingLeft = pLeft ? `${pLeft}px` : '';
  root.style.paddingRight = pRight ? `${pRight}px` : '';
}

// ResizeObserver on the dock root catches every event that changes
// the dock root's rect: window resize, side-panel Resizable handle
// drags, drawer open/close. Window resize is also caught here, so a
// separate window listener would be redundant.
let rootObserver: ResizeObserver | null = null;
function ensureRootObserver() {
  if (rootObserver) return;
  if (typeof ResizeObserver === 'undefined') return;
  const root = document.querySelector(DOCK_ROOT_SELECTOR);
  if (!root) return;
  rootObserver = new ResizeObserver(() => scheduleLayout());
  rootObserver.observe(root);
}

// Side-panel ResizeObserver tracks the resizable wrapper that holds
// the side panel. Dragging its handle changes the wrapper's width;
// we re-layout so docked side-panel sections track that width.
let sidePanelObserver: ResizeObserver | null = null;
let sidePanelObservedEl: Element | null = null;
function ensureSidePanelObserver() {
  if (typeof ResizeObserver === 'undefined') return;
  const wrapper = document.querySelector('.side-panel')?.closest('.resizable');
  if (!wrapper) {
    // Side panel toggled off; drop the observer until it returns.
    if (sidePanelObserver && sidePanelObservedEl) {
      sidePanelObserver.unobserve(sidePanelObservedEl);
    }
    sidePanelObservedEl = null;
    return;
  }
  if (sidePanelObservedEl === wrapper) return;
  if (!sidePanelObserver) {
    sidePanelObserver = new ResizeObserver(() => scheduleLayout());
  } else if (sidePanelObservedEl) {
    sidePanelObserver.unobserve(sidePanelObservedEl);
  }
  sidePanelObserver.observe(wrapper);
  sidePanelObservedEl = wrapper;
}

function getSidePanelWrapperWidth(): number {
  const wrapper = document
    .querySelector('.side-panel')
    ?.closest('.resizable') as HTMLElement | null;
  if (!wrapper) return 320;
  const w = wrapper.getBoundingClientRect().width;
  return w > 0 ? w : 320;
}

// Per-bar ResizeObservers fire when a docked bar's own rect changes,
// e.g., InfoTabsPane growing to its max-height when the user switches
// to the CHAT tab. Without this, padding stays sized to the bar's
// rect at dock time and the grown bar overlaps content below.
const barObservers = new Map<string, ResizeObserver>();
function observeBar(id: string, el: HTMLElement) {
  if (typeof ResizeObserver === 'undefined') return;
  if (barObservers.has(id)) return;
  const ro = new ResizeObserver(() => scheduleLayout());
  ro.observe(el);
  barObservers.set(id, ro);
}
function unobserveBar(id: string) {
  const ro = barObservers.get(id);
  if (!ro) return;
  ro.disconnect();
  barObservers.delete(id);
}

/// Register a bar element with the docking layout engine without
/// installing any drag interactions. The Layout Editor window drives
/// docking now; the main window only needs to know which DOM nodes
/// belong to which bar id so `runLayout` can position them when the
/// editor saves a new layout.
export function useDockTarget<T extends HTMLElement>(
  ref: RefObject<T | null>,
  id: string,
) {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    refsById.set(id, el);
    scheduleLayout();
    return () => {
      refsById.delete(id);
      // The element is going away (parent unmounted, etc.) — clear
      // any inline docked styles so a remount starts clean.
      if (el.hasAttribute('data-docked')) {
        el.removeAttribute('data-docked');
        el.style.position = '';
        el.style.left = '';
        el.style.top = '';
        el.style.width = '';
        el.style.zIndex = '';
      }
    };
  }, [ref, id]);
}

export function useDockable<T extends HTMLElement>(
  ref: RefObject<T | null>,
  id: string,
) {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    // Register the bar with the layout engine so already-saved docks
    // can be applied on first paint, and so future re-layouts (e.g.,
    // window resize) can target this element.
    refsById.set(id, el);
    scheduleLayout();

    let down: { x: number; y: number; pointerId: number } | null = null;
    let dragging = false;
    // True when this drag was canceled (Escape), so finish() should
    // not commit a dock even if the cursor is over a snap zone.
    let canceled = false;

    const finish = () => {
      if (dragging) {
        dragging = false;
        el.style.opacity = '';
        const zone = canceled ? null : dockingStore.get()?.zone ?? null;
        if (zone) {
          // Drop into an active zone => commit dock at that zone.
          dockLayoutStore.dock(id, zone);
        }
        // Else: drop outside any zone or canceled. Per spec, the
        // bar's docked state is unchanged.
        dockingStore.endDrag();
      }
      if (down) {
        try {
          el.releasePointerCapture(down.pointerId);
        } catch {
          // Pointer may have been released already.
        }
        down = null;
      }
      canceled = false;
    };

    const onPointerDown = (e: PointerEvent) => {
      // Primary button only. Right-clicks, middle-clicks, and pen
      // erasers should never start a drag.
      if (e.button !== 0) return;
      const target = e.target as HTMLElement | null;
      if (target && target.closest(INTERACTIVE_SELECTOR)) return;
      down = { x: e.clientX, y: e.clientY, pointerId: e.pointerId };
    };

    const onPointerMove = (e: PointerEvent) => {
      if (!down) return;
      if (!dragging) {
        const dx = e.clientX - down.x;
        const dy = e.clientY - down.y;
        if (dx * dx + dy * dy < DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) return;
        dragging = true;
        const rect = el.getBoundingClientRect();
        try {
          el.setPointerCapture(down.pointerId);
        } catch {
          // setPointerCapture can throw if the pointer is no longer
          // active. Drag continues using window-level listeners.
        }
        dockingStore.startDrag({
          id,
          sourceRect: {
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
          },
          startCursor: { x: down.x, y: down.y },
          cursor: { x: e.clientX, y: e.clientY },
        });
        el.style.opacity = '0.6';
      } else {
        dockingStore.updateCursor({ x: e.clientX, y: e.clientY });
      }
    };

    const onPointerUp = () => finish();
    const onPointerCancel = () => {
      canceled = true;
      finish();
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && (dragging || down)) {
        canceled = true;
        finish();
      }
    };

    el.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('pointercancel', onPointerCancel);
    window.addEventListener('keydown', onKeyDown);

    return () => {
      el.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerCancel);
      window.removeEventListener('keydown', onKeyDown);
      canceled = true;
      finish();
      refsById.delete(id);
    };
  }, [ref, id]);
}
