// Persisted ordering of the side-panel sections. The set of panel IDs
// is fixed in code; only their order is user-controlled. Stored in
// localStorage so each window starts up immediately, with a Tauri
// event so reorders made in the Layout Editor window propagate to the
// main window without a relaunch.

import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';

const STORAGE_KEY = 'vosh.sidebar.order';
const SYNC_EVENT = 'vosh://sidebar-order-changed';

export const SIDEBAR_PANEL_IDS = ['affects-pane', 'map-pane', 'info-tabs-pane'] as const;

export type SidebarPanelId = (typeof SIDEBAR_PANEL_IDS)[number];

const KNOWN: Set<string> = new Set(SIDEBAR_PANEL_IDS);

function normalize(arr: unknown): SidebarPanelId[] {
  if (!Array.isArray(arr)) return [...SIDEBAR_PANEL_IDS];
  const valid = arr.filter(
    (v): v is SidebarPanelId => typeof v === 'string' && KNOWN.has(v),
  );
  for (const id of SIDEBAR_PANEL_IDS) {
    if (!valid.includes(id)) valid.push(id);
  }
  return valid;
}

export function loadSidebarOrder(): SidebarPanelId[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [...SIDEBAR_PANEL_IDS];
    return normalize(JSON.parse(raw));
  } catch {
    return [...SIDEBAR_PANEL_IDS];
  }
}

export function saveSidebarOrder(order: SidebarPanelId[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(order));
  } catch {
    // Storage unavailable; the next render falls back to defaults.
  }
}

export function moveSidebarPanel(
  order: SidebarPanelId[],
  id: SidebarPanelId,
  direction: 'up' | 'down',
): SidebarPanelId[] {
  const idx = order.indexOf(id);
  if (idx < 0) return order;
  const target = direction === 'up' ? idx - 1 : idx + 1;
  if (target < 0 || target >= order.length) return order;
  const next = [...order];
  [next[idx], next[target]] = [next[target], next[idx]];
  return next;
}

/// Persist `order` and broadcast it to other windows so live reorders
/// in the Settings window's Layout Editor reach the main window's
/// SidePanel without a relaunch.
export async function broadcastSidebarOrder(order: SidebarPanelId[]): Promise<void> {
  saveSidebarOrder(order);
  try {
    await emit(SYNC_EVENT, order);
  } catch {
    // Tauri unavailable (e.g., dev preview without backend); the
    // localStorage write is the persistent fallback.
  }
}

export async function subscribeSidebarOrder(
  callback: (order: SidebarPanelId[]) => void,
): Promise<UnlistenFn> {
  return listen<SidebarPanelId[]>(SYNC_EVENT, (event) => {
    const next = normalize(event.payload);
    saveSidebarOrder(next);
    callback(next);
  });
}
