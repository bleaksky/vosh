import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import SHORTCUTS from './appShortcuts.json';
import { PANE_TYPES, type PaneType } from './paneLayout';

// The page side of the macOS menu bar (src-tauri/src/app_menu.rs). A
// menu command reaches the main window as `vosh://app-menu` with the
// palette entry id, and App runs it through the same dispatcher as its
// keyboard shortcuts, so a command behaves the same from the menu, the
// keyboard, and the palette. The main window sends the menu a snapshot
// of its state whenever the snapshot changes, and the menu mirrors it.
//
// The shortcut specs live in appShortcuts.json, which the Rust menu
// reads too, so the menu, the palette keycaps, and the keydown handler
// can never disagree about a key.

export type AppShortcutId = keyof typeof SHORTCUTS;

/** Every command with a shortcut, as a palette spec like `Mod+K`. */
export const APP_SHORTCUTS: Readonly<Record<AppShortcutId, string>> = SHORTCUTS;

/** Menu commands arrive on this event, in the main window. */
export const APP_MENU_EVENT = 'vosh://app-menu';

/** Find, chosen while Settings is in front, arrives in Settings here. */
export const SETTINGS_FIND_EVENT = 'vosh://settings-find';

/** Opens the session popover under the title, in a given mode. */
export const SESSION_MENU_EVENT = 'vosh:session-menu';

export type SessionMenuMode = 'menu' | 'edit' | 'new';

// The shortcuts the main window binds in its own keydown handler. Copy
// and Close window belong to the menu and the fields.
const WINDOW_SHORTCUTS: readonly AppShortcutId[] = [
  'connect',
  'panel',
  'palette',
  'find',
  'settings',
  'help',
  'split',
];

function specKey(spec: string): { key: string; shift: boolean } {
  const parts = spec.split('+');
  const key = (parts.pop() ?? '').toLowerCase();
  return { key, shift: parts.some((p) => p.toLowerCase() === 'shift') };
}

/** What a primary modifier key press means in the main window. `key`
 *  comes from shortcutKey (so it is lowercase). Null leaves the key to
 *  the page. A hit with no id takes the key and runs nothing: Shift+R,
 *  which would otherwise reload the page on Windows. */
export function resolveShortcut(key: string, shift: boolean): { id: AppShortcutId | null } | null {
  if (key === 'r' && shift) return { id: null };
  for (const id of WINDOW_SHORTCUTS) {
    const spec = specKey(APP_SHORTCUTS[id]);
    if (spec.key === key && spec.shift === shift) return { id };
  }
  return null;
}

/** Help carries its own search, so the palette and the find bar stand
 *  down while it is open, from the keyboard and the menu alike. */
export function commandBlocked(id: string, state: { helpOpen: boolean }): boolean {
  return state.helpOpen && (id === 'palette' || id === 'find');
}

/** Commands a held key repeats. The rest run once per press. */
export function commandRepeats(id: string): boolean {
  return id === 'find' || id === 'help';
}

// ── Menu state ───────────────────────────────────────────────────────

export interface MenuPaneState {
  pane: PaneType;
  /** Checked. The pane shows in the open panel. */
  visible: boolean;
  /** Listed. Staff queues waits for the MUD to offer it. */
  offered: boolean;
}

export interface MenuThemeState {
  id: string;
  label: string;
  /** Your own theme. The menu lists these after a separator. */
  custom: boolean;
}

/** The snapshot menu_set_state takes. camelCase on the wire. */
export interface MenuState {
  connected: boolean;
  worldName: string | null;
  panelOpen: boolean;
  splitOpen: boolean;
  panes: MenuPaneState[];
  themes: MenuThemeState[];
  theme: string;
}

export interface MenuStateInput {
  /** Connecting or connected. */
  live: boolean;
  worldName: string | null;
  panelOpen: boolean;
  /** The split is open, or the native grid is scrolled back. */
  splitOpen: boolean;
  shownPanes: readonly PaneType[];
  /** The MUD sent Imm.Queues this session. */
  staffOffered: boolean;
  /** Every theme in gallery order. */
  themes: readonly MenuThemeState[];
  theme: string;
}

/** The menu's view of the window, the same checks the palette shows. */
export function buildMenuState(input: MenuStateInput): MenuState {
  const world = input.worldName?.trim() ?? '';
  return {
    connected: input.live,
    worldName: world.length > 0 ? world : null,
    panelOpen: input.panelOpen,
    splitOpen: input.splitOpen,
    panes: PANE_TYPES.map((pane) => {
      const shown = input.shownPanes.includes(pane);
      return {
        pane,
        visible: input.panelOpen && shown,
        // A pane the tree already shows stays listed, so you can hide it.
        offered: pane !== 'imm' || input.staffOffered || shown,
      };
    }),
    themes: input.themes.map(({ id, label, custom }) => ({ id, label, custom })),
    theme: input.theme,
  };
}

let lastSent: string | null = null;

/** Send the menu a snapshot, skipping one it already has. The caller
 *  sends only on macOS, since Windows and Linux have no menu bar. */
export function setAppMenuState(state: MenuState): void {
  const json = JSON.stringify(state);
  if (json === lastSent) return;
  lastSent = json;
  invoke('menu_set_state', { state }).catch((e: unknown) => {
    // Send the next snapshot even when it matches this one.
    lastSent = null;
    console.error('[menu] menu_set_state failed', e);
  });
}

/** Forget the last snapshot, so the next one always goes out. Tests. */
export function resetAppMenuState(): void {
  lastSent = null;
}

/** Hear menu commands. Main window only. */
export function listenAppMenu(cb: (id: string) => void): Promise<UnlistenFn> {
  return listen<unknown>(APP_MENU_EVENT, (event) => {
    if (typeof event.payload === 'string') cb(event.payload);
  });
}

// ── Copy ─────────────────────────────────────────────────────────────

interface SelectionSource {
  activeElement: Element | null;
  getSelection(): { isCollapsed: boolean; toString(): string } | null;
}

/** Whether the page holds a text selection of its own: a range in the
 *  focused field, or selected text anywhere in the page. */
export function pageHasSelection(doc: SelectionSource = document): boolean {
  const field = doc.activeElement as { selectionStart?: unknown; selectionEnd?: unknown } | null;
  if (
    field &&
    typeof field.selectionStart === 'number' &&
    typeof field.selectionEnd === 'number' &&
    field.selectionStart !== field.selectionEnd
  ) {
    return true;
  }
  const selection = doc.getSelection();
  return !!selection && !selection.isCollapsed && selection.toString().length > 0;
}

/** Edit, then Copy, with the native grid. With `terminal` a grid
 *  selection wins. Otherwise the system copies the page's own. */
export function menuCopy(terminal: boolean): void {
  invoke('menu_copy', { terminal }).catch((e: unknown) => {
    console.error('[menu] menu_copy failed', e);
  });
}

/** Open the session popover in `mode`, from the menu bar. */
export function requestSessionMenu(mode: SessionMenuMode): void {
  window.dispatchEvent(new CustomEvent<SessionMenuMode>(SESSION_MENU_EVENT, { detail: mode }));
}
