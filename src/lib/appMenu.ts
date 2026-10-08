import type { UnlistenFn } from '@tauri-apps/api/event';
import { menuSetState, subscribeAppMenu } from '../ipc/windows';
import SHORTCUTS from './appShortcuts.json';
import { PANE_TYPES, type PaneType } from '../panel/paneLayout';

// The page side of the macOS menu bar (src-tauri/src/app/menu.rs). A
// menu command reaches the main window as `vosh://app-menu` with the
// palette entry id, and shell/useAppCommands.ts runs it through the same
// dispatcher as the keyboard shortcuts, so a command behaves the same
// from the menu, the keyboard, and the palette. The main window sends
// the menu a snapshot of its state whenever the snapshot changes, and
// the menu mirrors it.
//
// The shortcut specs live in appShortcuts.json, which the Rust menu
// reads too, so the menu, the palette keycaps, and the keydown handler
// can never disagree about a key.

export type AppShortcutId = keyof typeof SHORTCUTS;

/** Every command with a shortcut, as a palette spec like `Mod+K`. */
export const APP_SHORTCUTS: Readonly<Record<AppShortcutId, string>> = SHORTCUTS;

/** Opens the session popover under the title, on what a request names. */
export const SESSION_MENU_EVENT = 'vosh:session-menu';

/** Opens Add a pane under the plus in the title band, as Show me on
 *  the Chat and Group step of Get started does. */
export const ADD_PANE_MENU_EVENT = 'vosh:add-pane-menu';

/** A session New session… opened for its form: the session, the one
 *  selected before it, which Cancel goes back to, the profile in front
 *  then, which the form's pick falls back to, and the profile it plays. */
export interface OpenedSession {
  id: number;
  previous: number;
  front: string;
  profile: string;
}

/** What the session popover opens on: its list, the Edit connection
 *  form, the Rename session form, or the New session form of a session
 *  New session… opened. */
export type SessionMenuRequest =
  | { mode: 'menu' }
  | { mode: 'edit' }
  | { mode: 'rename' }
  | { mode: 'new'; opened: OpenedSession };

// The shortcuts the main window binds in its own keydown handler. Copy
// belongs to the fields and the menu bar.
const WINDOW_SHORTCUTS: readonly AppShortcutId[] = [
  'connect',
  'panel',
  'palette',
  'find',
  'settings',
  'help',
  'split',
  'snoop',
  'session-new',
  'session-close',
  'close-window',
  'session-next',
  'session-previous',
];

// The keys that act on sessions, otty's keys (Sessions Q11). A macro
// bound to one of them, or to a Settings key below, keeps the key in
// the sessions on its profile, where every other app key wins over a
// macro. Mod with a digit from 1 to 9 goes to a session by its place in
// the list, and is one of them.
const SESSION_SHORTCUTS = [
  'session-new',
  'session-close',
  'close-window',
  'session-next',
  'session-previous',
] as const satisfies readonly AppShortcutId[];

/** A key that acts on sessions. */
export type SessionShortcutId = (typeof SESSION_SHORTCUTS)[number];

function isSessionShortcut(id: AppShortcutId): id is SessionShortcutId {
  return (SESSION_SHORTCUTS as readonly AppShortcutId[]).includes(id);
}

// The keys that open a Settings page, Mod and Shift with a digit from 1
// to 4. Both windows take them, and a macro keeps them in the main
// window as it keeps a session key.
const SETTINGS_SHORTCUTS = [
  'settings-triggers',
  'settings-aliases',
  'settings-macros',
  'settings-timers',
] as const satisfies readonly AppShortcutId[];

/** A key that opens a Settings page. */
export type SettingsShortcutId = (typeof SETTINGS_SHORTCUTS)[number];

/** An app key a macro can keep: a session key or a Settings key. */
export type MacroKeptShortcutId = SessionShortcutId | SettingsShortcutId;

// Shift with ] types } on a US layout, and other layouts put the
// brackets on other keys, so the step keys match the physical keys.
const PHYSICAL_KEYS: Record<string, string> = { '[': 'BracketLeft', ']': 'BracketRight' };

// What Shift makes of each bracket and digit on a US layout, so a macro
// saved from Shift with one of those keys still reads as that key.
const SHIFTED_KEYS: Record<string, string> = {
  '[': '{',
  ']': '}',
  '1': '!',
  '2': '@',
  '3': '#',
  '4': '$',
};

function specKey(spec: string): { key: string; shift: boolean } {
  const parts = spec.split('+');
  const key = (parts.pop() ?? '').toLowerCase();
  return { key, shift: parts.some((p) => p.toLowerCase() === 'shift') };
}

/** A primary modifier key press: `key` from shortcutKey (so lowercase),
 *  `code` the physical key. */
export interface ShortcutPress {
  key: string;
  code: string;
  shift: boolean;
}

/** What a press does in the main window. */
export type ShortcutHit =
  /** Run the command. */
  | { kind: 'run'; id: AppShortcutId }
  /** Bring the session at `place` in the list to the front, from 1. */
  | { kind: 'goto'; place: number }
  /** Take the key and run nothing: Shift+R, which would otherwise
   *  reload the page on Windows. */
  | { kind: 'take' }
  /** A session or Settings key a macro is bound to. The macro runs
   *  from the command line, and nothing else takes the key. */
  | { kind: 'macro' };

/** The Settings key a press is, or null. Shift with a digit types !
 *  or another character, which differs by layout, so the digit matches
 *  on the physical key. Settings and the main window both read it. */
export function settingsShortcutOf(press: ShortcutPress): SettingsShortcutId | null {
  if (!press.shift) return null;
  return (
    SETTINGS_SHORTCUTS.find((id) => press.code === `Digit${specKey(APP_SHORTCUTS[id]).key}`) ?? null
  );
}

/** What a primary modifier key press means in the main window. Null
 *  leaves the key to the page. `macroBound` says whether the selected
 *  session's profile binds a macro to this press, and is asked only for
 *  a session or Settings key. `snooping` says whether the selected session has a
 *  snoop open. Without one the snoop key stays the page's, so a macro
 *  on Ctrl+J on Windows and Linux keeps working. */
export function resolveShortcut(
  press: ShortcutPress,
  macroBound: () => boolean = () => false,
  snooping: () => boolean = () => false,
): ShortcutHit | null {
  const { key, code, shift } = press;
  if (key === 'r' && shift) return { kind: 'take' };
  const digit = /^Digit([1-9])$/.exec(code);
  if (digit && !shift) {
    return macroBound() ? { kind: 'macro' } : { kind: 'goto', place: Number(digit[1]) };
  }
  if (digit) {
    const id = settingsShortcutOf(press);
    if (!id) return null;
    return macroBound() ? { kind: 'macro' } : { kind: 'run', id };
  }
  for (const id of WINDOW_SHORTCUTS) {
    const spec = specKey(APP_SHORTCUTS[id]);
    const physical = PHYSICAL_KEYS[spec.key];
    if (spec.shift !== shift || (physical ? code !== physical : spec.key !== key)) continue;
    if (id === 'snoop' && !snooping()) return null;
    return isSessionShortcut(id) && macroBound() ? { kind: 'macro' } : { kind: 'run', id };
  }
  return null;
}

/** The app key a macro's key is too, by the macro's canonical key
 *  (automation/macroKeys.ts): Meta on macOS and Ctrl elsewhere, with a
 *  digit, one of the session keys or one of the Settings keys. Settings
 *  names the clash with it. A bracket or a shifted digit matches as
 *  Shift types it on a US layout too. */
export function appKeyOfMacro(
  canonical: string,
  mac: boolean,
): { kind: 'run'; id: MacroKeptShortcutId } | { kind: 'goto'; place: number } | null {
  const named = (key: string, shift: boolean) =>
    mac ? `${shift ? 'Shift+' : ''}Meta+${key}` : `Ctrl+${shift ? 'Shift+' : ''}${key}`;
  for (let place = 1; place <= 9; place += 1) {
    if (canonical === named(String(place), false)) return { kind: 'goto', place };
  }
  for (const id of [...SESSION_SHORTCUTS, ...SETTINGS_SHORTCUTS]) {
    const { key, shift } = specKey(APP_SHORTCUTS[id]);
    const keys = [key.toUpperCase(), SHIFTED_KEYS[key]].filter(Boolean);
    if (keys.some((k) => canonical === named(k, shift))) return { kind: 'run', id };
  }
  return null;
}

/** Commands a held key repeats. The rest run once per press. */
export function commandRepeats(id: string): boolean {
  return id === 'find';
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
  /** A series of redials runs. The menu shows Disconnect between the
   *  tries too, as the palette and the session menu do. */
  redialing: boolean;
  worldName: string | null;
  panelOpen: boolean;
  splitOpen: boolean;
  panes: MenuPaneState[];
  themes: MenuThemeState[];
  theme: string;
  /** How many sessions are open. Next session, Previous session and
   *  Show sessions show dimmed with one. */
  sessions: number;
  /** You keep the sessions sidebar showing, checked in View, though a
   *  narrow window can fold it. */
  sessionsShown: boolean;
  /** How many snoop tabs the selected session has, live or ended. View
   *  lists Go to snoop while there is one. */
  snoops: number;
}

export interface MenuStateInput {
  /** Connecting or connected. */
  live: boolean;
  /** A redial waits for its next try or dials it. */
  redialing: boolean;
  worldName: string | null;
  panelOpen: boolean;
  /** The split is open, or the native grid is scrolled back. */
  splitOpen: boolean;
  /** The paneKey of every pane the panel tree holds. The menu lists
   *  the built-in panes alone, whose key is their type. */
  shownPanes: readonly string[];
  /** The MUD sent Imm.Queues this session. */
  staffOffered: boolean;
  /** Every theme in gallery order. */
  themes: readonly MenuThemeState[];
  theme: string;
  sessions: number;
  sessionsShown: boolean;
  snoops: number;
}

/** The menu's view of the window, the same checks the palette shows. */
export function buildMenuState(input: MenuStateInput): MenuState {
  const world = input.worldName?.trim() ?? '';
  return {
    connected: input.live,
    redialing: input.redialing,
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
    sessions: input.sessions,
    sessionsShown: input.sessionsShown,
    snoops: input.snoops,
  };
}

let lastSent: string | null = null;

/** Send the menu a snapshot, skipping one it already has. The caller
 *  sends only on macOS, since Windows and Linux have no menu bar. */
export function setAppMenuState(state: MenuState): void {
  const json = JSON.stringify(state);
  if (json === lastSent) return;
  lastSent = json;
  menuSetState(state).catch((e: unknown) => {
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
  return subscribeAppMenu((id) => {
    if (typeof id === 'string') cb(id);
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

/** Open the session popover on what `request` names, from the menu bar
 *  and New session…. */
export function requestSessionMenu(request: SessionMenuRequest): void {
  window.dispatchEvent(
    new CustomEvent<SessionMenuRequest>(SESSION_MENU_EVENT, { detail: request }),
  );
}
