import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  APP_SHORTCUTS,
  appShortcut,
  buildMenuState,
  commandRepeats,
  pageHasSelection,
  resetAppMenuState,
  resolveShortcut,
  appKeyOfMacro,
  setAppMenuState,
  type AppShortcutId,
  type MenuStateInput,
} from './appMenu';
import { shortcutLabel } from './shortcuts';
import { buildPaletteEntries, type PaletteDeps } from '../shell/overlays/palette';
import { paneKey } from '../panel/paneLayout';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTheme: () => Promise.resolve() }),
}));

/** A press as a US layout types it: the key, then the physical key. */
const press = (key: string, code: string, shift = false) => ({ key, code, shift });
const letter = (key: string, shift = false) => press(key, `Key${key.toUpperCase()}`, shift);

describe('resolveShortcut', () => {
  it('maps each window shortcut to its command', () => {
    const run = (id: string) => ({ kind: 'run', id });
    expect(resolveShortcut(letter('k'))).toEqual(run('palette'));
    expect(resolveShortcut(letter('f'))).toEqual(run('find'));
    expect(resolveShortcut(letter('r'))).toEqual(run('connect'));
    expect(resolveShortcut(press(',', 'Comma'))).toEqual(run('settings'));
    expect(resolveShortcut(press('/', 'Slash'))).toEqual(run('help'));
    expect(resolveShortcut(press('\\', 'Backslash'))).toEqual(run('split'));
    expect(resolveShortcut(letter('l', true))).toEqual(run('panel'));
  });

  it('maps the session keys, with Close window on Shift', () => {
    const run = (id: string) => ({ kind: 'run', id });
    expect(resolveShortcut(letter('t'))).toEqual(run('session-new'));
    expect(resolveShortcut(letter('w'))).toEqual(run('session-close'));
    expect(resolveShortcut(letter('w', true))).toEqual(run('close-window'));
  });

  it('steps on the physical bracket keys, where Shift with ] types }', () => {
    expect(resolveShortcut(press('}', 'BracketRight', true))).toEqual({
      kind: 'run',
      id: 'session-next',
    });
    expect(resolveShortcut(press('{', 'BracketLeft', true))).toEqual({
      kind: 'run',
      id: 'session-previous',
    });
    // A layout that types ] on another key steps from the bracket keys
    // alone, and the bracket keys step only with Shift.
    expect(resolveShortcut(press(']', 'Digit9', true))).toBeNull();
    expect(resolveShortcut(press(']', 'BracketRight'))).toBeNull();
  });

  it('goes to a session by its place from the digits 1 to 9', () => {
    for (let place = 1; place <= 9; place += 1) {
      expect(resolveShortcut(press(String(place), `Digit${place}`))).toEqual({
        kind: 'goto',
        place,
      });
    }
    // The key a layout types on the digit row does not matter, as on
    // AZERTY, where the 1 key types &.
    expect(resolveShortcut(press('&', 'Digit1'))).toEqual({ kind: 'goto', place: 1 });
    expect(resolveShortcut(press('0', 'Digit0'))).toBeNull();
    expect(resolveShortcut(press('1', 'Numpad1'))).toBeNull();
  });

  it('opens a Settings page from Shift with the digits 1 to 4', () => {
    const run = (id: string) => ({ kind: 'run', id });
    // Shift with 1 types ! on a US layout, and the physical key decides.
    expect(resolveShortcut(press('!', 'Digit1', true))).toEqual(run('settings-triggers'));
    expect(resolveShortcut(press('@', 'Digit2', true))).toEqual(run('settings-aliases'));
    expect(resolveShortcut(press('#', 'Digit3', true))).toEqual(run('settings-macros'));
    expect(resolveShortcut(press('$', 'Digit4', true))).toEqual(run('settings-timers'));
    // A layout that types something else on the digit row still reaches it.
    expect(resolveShortcut(press('"', 'Digit2', true))).toEqual(run('settings-aliases'));
    // Without Shift the digit still goes to a session.
    expect(resolveShortcut(press('1', 'Digit1'))).toEqual({ kind: 'goto', place: 1 });
  });

  it('leaves every other Shift with a digit to the page', () => {
    const bound = vi.fn(() => true);
    for (let place = 5; place <= 9; place += 1) {
      expect(resolveShortcut(press('%', `Digit${place}`, true), bound)).toBeNull();
    }
    expect(resolveShortcut(press(')', 'Digit0', true), bound)).toBeNull();
    expect(resolveShortcut(press('1', 'Numpad1', true), bound)).toBeNull();
    expect(bound).not.toHaveBeenCalled();
  });

  it('leaves a session key to a macro the profile binds to it', () => {
    const bound = () => true;
    expect(resolveShortcut(press('1', 'Digit1'), bound)).toEqual({ kind: 'macro' });
    expect(resolveShortcut(letter('t'), bound)).toEqual({ kind: 'macro' });
    expect(resolveShortcut(letter('w'), bound)).toEqual({ kind: 'macro' });
    expect(resolveShortcut(letter('w', true), bound)).toEqual({ kind: 'macro' });
    expect(resolveShortcut(press('}', 'BracketRight', true), bound)).toEqual({ kind: 'macro' });
    // A Settings key too.
    expect(resolveShortcut(press('!', 'Digit1', true), bound)).toEqual({ kind: 'macro' });
    expect(resolveShortcut(press('$', 'Digit4', true), bound)).toEqual({ kind: 'macro' });
    // Every other app key wins over a macro, as before.
    expect(resolveShortcut(letter('k'), bound)).toEqual({ kind: 'run', id: 'palette' });
    expect(resolveShortcut(letter('r'), bound)).toEqual({ kind: 'run', id: 'connect' });
  });

  it('asks after a macro only for a session or Settings key', () => {
    const bound = vi.fn(() => false);
    resolveShortcut(letter('k'), bound);
    resolveShortcut(letter('c'), bound);
    expect(bound).not.toHaveBeenCalled();
    resolveShortcut(press('2', 'Digit2'), bound);
    expect(bound).toHaveBeenCalledTimes(1);
    resolveShortcut(press('@', 'Digit2', true), bound);
    expect(bound).toHaveBeenCalledTimes(2);
  });

  it('toggles the sessions sidebar on Ctrl with Cmd and S on macOS', () => {
    const toggle = { kind: 'run', id: 'sessions-sidebar' };
    const ctrl = { ...letter('s'), ctrl: true };
    expect(resolveShortcut(ctrl, undefined, undefined, true)).toEqual(toggle);
    // Cmd S alone, Cmd Shift S and a Ctrl with Cmd digit stay the page's.
    expect(resolveShortcut(letter('s'), undefined, undefined, true)).toBeNull();
    expect(resolveShortcut(letter('s', true), undefined, undefined, true)).toBeNull();
    expect(
      resolveShortcut({ ...press('1', 'Digit1'), ctrl: true }, undefined, undefined, true),
    ).toBeNull();
    // Ctrl with Cmd reaches no other app key.
    expect(resolveShortcut({ ...letter('k'), ctrl: true }, undefined, undefined, true)).toBeNull();
    expect(
      resolveShortcut({ ...letter('r', true), ctrl: true }, undefined, undefined, true),
    ).toBeNull();
  });

  it('toggles the sessions sidebar on Ctrl+Shift+S on Windows and Linux', () => {
    const toggle = { kind: 'run', id: 'sessions-sidebar' };
    expect(resolveShortcut(letter('s', true), undefined, undefined, false)).toEqual(toggle);
    expect(resolveShortcut(letter('s'), undefined, undefined, false)).toBeNull();
  });

  it('leaves the sessions toggle key to a macro bound to it', () => {
    const bound = () => true;
    expect(resolveShortcut({ ...letter('s'), ctrl: true }, bound, undefined, true)).toEqual({
      kind: 'macro',
    });
    expect(resolveShortcut(letter('s', true), bound, undefined, false)).toEqual({
      kind: 'macro',
    });
  });

  it('takes Shift+R without running anything, so the page never reloads', () => {
    expect(resolveShortcut(letter('r', true))).toEqual({ kind: 'take' });
  });

  it('leaves other keys to the page and the menu bar', () => {
    expect(resolveShortcut(letter('l'))).toBeNull();
    expect(resolveShortcut(letter('k', true))).toBeNull();
    // Copy belongs to the fields and the menu bar.
    expect(resolveShortcut(letter('c'))).toBeNull();
    expect(resolveShortcut(letter('t', true))).toBeNull();
  });
});

describe('the snoop key', () => {
  it('goes to the snoop while the session has one open', () => {
    expect(resolveShortcut(letter('j'), undefined, () => true)).toEqual({
      kind: 'run',
      id: 'snoop',
    });
  });

  it('stays the page key with no snoop open, so a macro on it still fires', () => {
    expect(resolveShortcut(letter('j'))).toBeNull();
    expect(
      resolveShortcut(
        letter('j'),
        () => true,
        () => false,
      ),
    ).toBeNull();
  });

  it('is Mod+J, a window key that runs once per press', () => {
    expect(APP_SHORTCUTS.snoop).toBe('Mod+J');
    expect(commandRepeats('snoop')).toBe(false);
    expect(resolveShortcut(letter('j', true), undefined, () => true)).toBeNull();
  });
});

describe('appKeyOfMacro', () => {
  it('finds the sessions toggle key on either platform', () => {
    const toggle = { kind: 'run', id: 'sessions-sidebar' };
    expect(appKeyOfMacro('Ctrl+Meta+S', true)).toEqual(toggle);
    expect(appKeyOfMacro('Ctrl+Shift+S', false)).toEqual(toggle);
    expect(appKeyOfMacro('Meta+S', true)).toBeNull();
    expect(appKeyOfMacro('Ctrl+Shift+S', true)).toBeNull();
    expect(appKeyOfMacro('Ctrl+S', false)).toBeNull();
  });

  it('finds the session key a macro key shares on macOS', () => {
    expect(appKeyOfMacro('Meta+1', true)).toEqual({ kind: 'goto', place: 1 });
    expect(appKeyOfMacro('Meta+9', true)).toEqual({ kind: 'goto', place: 9 });
    expect(appKeyOfMacro('Meta+T', true)).toEqual({ kind: 'run', id: 'session-new' });
    expect(appKeyOfMacro('Meta+W', true)).toEqual({ kind: 'run', id: 'session-close' });
    expect(appKeyOfMacro('Shift+Meta+W', true)).toEqual({ kind: 'run', id: 'close-window' });
    // Shift with ] types } on a US layout, and either reads as the key.
    expect(appKeyOfMacro('Shift+Meta+}', true)).toEqual({ kind: 'run', id: 'session-next' });
    expect(appKeyOfMacro('Shift+Meta+]', true)).toEqual({ kind: 'run', id: 'session-next' });
    expect(appKeyOfMacro('Shift+Meta+{', true)).toEqual({
      kind: 'run',
      id: 'session-previous',
    });
    // Ctrl belongs to your macros on macOS, and no app key takes it.
    expect(appKeyOfMacro('Ctrl+1', true)).toBeNull();
    expect(appKeyOfMacro('Meta+0', true)).toBeNull();
    expect(appKeyOfMacro('Meta+K', true)).toBeNull();
    expect(appKeyOfMacro('Shift+Meta+5', true)).toBeNull();
    expect(appKeyOfMacro('Meta+!', true)).toBeNull();
    expect(appKeyOfMacro('F1', true)).toBeNull();
  });

  it('reads Ctrl on Windows and Linux', () => {
    expect(appKeyOfMacro('Ctrl+2', false)).toEqual({ kind: 'goto', place: 2 });
    expect(appKeyOfMacro('Ctrl+Shift+W', false)).toEqual({ kind: 'run', id: 'close-window' });
    expect(appKeyOfMacro('Ctrl+Shift+[', false)).toEqual({
      kind: 'run',
      id: 'session-previous',
    });
    expect(appKeyOfMacro('Meta+1', false)).toBeNull();
  });

  it('finds the Settings key a macro key shares, by the digit or what Shift types', () => {
    const run = (id: string) => ({ kind: 'run', id });
    expect(appKeyOfMacro('Shift+Meta+1', true)).toEqual(run('settings-triggers'));
    expect(appKeyOfMacro('Shift+Meta+!', true)).toEqual(run('settings-triggers'));
    expect(appKeyOfMacro('Shift+Meta+@', true)).toEqual(run('settings-aliases'));
    expect(appKeyOfMacro('Shift+Meta+3', true)).toEqual(run('settings-macros'));
    expect(appKeyOfMacro('Shift+Meta+$', true)).toEqual(run('settings-timers'));
    expect(appKeyOfMacro('Ctrl+Shift+1', false)).toEqual(run('settings-triggers'));
    expect(appKeyOfMacro('Ctrl+Shift+!', false)).toEqual(run('settings-triggers'));
    expect(appKeyOfMacro('Ctrl+Shift+2', false)).toEqual(run('settings-aliases'));
    expect(appKeyOfMacro('Ctrl+Shift+#', false)).toEqual(run('settings-macros'));
    expect(appKeyOfMacro('Ctrl+Shift+4', false)).toEqual(run('settings-timers'));
    // The other platform's spelling is no app key.
    expect(appKeyOfMacro('Ctrl+Shift+1', true)).toBeNull();
    expect(appKeyOfMacro('Shift+Meta+1', false)).toBeNull();
  });
});

describe('command gates', () => {
  it('repeats only find on a held key', () => {
    expect(commandRepeats('find')).toBe(true);
    // Help is a window of its own, so a held key opens it once.
    for (const id of ['help', 'palette', 'connect', 'panel', 'split', 'settings']) {
      expect(commandRepeats(id)).toBe(false);
    }
  });
});

describe('shared shortcut table', () => {
  const deps = (connected: boolean): PaletteDeps => ({
    connected,
    worldName: 'The Forsaken Lands',
    panelOpen: true,
    togglePanel: () => {},
    splitOpen: false,
    toggleSplit: () => {},
    paneTypes: [],
    paneVisible: () => false,
    togglePane: () => {},
    openHelp: () => {},
    openFind: () => {},
    openSettings: () => {},
    openSettingsTab: () => {},
    connect: () => {},
    disconnect: () => {},
    insertInput: () => {},
  });

  it('gives the palette keycaps the keys the menu bar binds', () => {
    const keys = new Map(buildPaletteEntries(deps(false)).map((e) => [e.id, e.keys]));
    for (const id of ['panel', 'split', 'find', 'help', 'settings', 'connect'] as const) {
      expect(keys.get(id)).toBe(APP_SHORTCUTS[id]);
    }
  });

  it('uses Mod for every menu shortcut, and Ctrl only beside it on macOS', () => {
    for (const id of Object.keys(APP_SHORTCUTS) as AppShortcutId[]) {
      const other = appShortcut(id, false);
      expect(other.startsWith('Mod+')).toBe(true);
      expect(other.toLowerCase()).not.toContain('ctrl');
      const mac = appShortcut(id, true);
      expect(mac.startsWith('Mod+') || mac.startsWith('Ctrl+Mod+')).toBe(true);
    }
  });

  it('gives no two app keys one key on either platform', () => {
    for (const mac of [true, false]) {
      const ids = Object.keys(APP_SHORTCUTS) as AppShortcutId[];
      const keys = ids.map((id) => shortcutLabel(appShortcut(id, mac), mac));
      expect(new Set(keys).size).toBe(keys.length);
      // Mod with a digit goes to a session, so no app key may take one.
      for (const key of keys) expect(key).not.toMatch(/^(⌘|Ctrl\+)[1-9]$/);
    }
  });
});

describe('buildMenuState', () => {
  const input = (over: Partial<MenuStateInput> = {}): MenuStateInput => ({
    live: false,
    redialing: false,
    worldName: 'The Forsaken Lands',
    panelOpen: true,
    splitOpen: false,
    shownPanes: ['map', 'affects'],
    staffOffered: false,
    themes: [
      { id: 'nord', label: 'Nord', custom: false },
      { id: 'mine', label: 'Nord (custom)', custom: true },
    ],
    theme: 'nord',
    sessions: 1,
    sessionsShown: false,
    snoops: 0,
    ...over,
  });

  it('checks the panes the open panel shows', () => {
    const state = buildMenuState(input());
    expect(state.panes.map((p) => [p.pane, p.visible])).toEqual([
      ['map', true],
      ['affects', true],
      ['group', false],
      ['chat', false],
      ['imm', false],
    ]);
    const hidden = buildMenuState(input({ panelOpen: false }));
    expect(hidden.panes.every((p) => !p.visible)).toBe(true);
  });

  it('lists the built-in panes alone while a Lua pane shows', () => {
    const lua = paneKey({ pane: 'lua', props: { plugin: 'weather_pane', id: 'weather' } });
    const state = buildMenuState(input({ shownPanes: ['map', 'affects', lua] }));
    expect(state).toEqual(buildMenuState(input()));
  });

  it('lists staff queues only once the MUD offers it or the panel shows it', () => {
    const imm = (s: ReturnType<typeof buildMenuState>) => s.panes.find((p) => p.pane === 'imm');
    expect(imm(buildMenuState(input()))?.offered).toBe(false);
    expect(imm(buildMenuState(input({ staffOffered: true })))?.offered).toBe(true);
    expect(imm(buildMenuState(input({ shownPanes: ['imm'] })))?.offered).toBe(true);
    const others = buildMenuState(input()).panes.filter((p) => p.pane !== 'imm');
    expect(others.every((p) => p.offered)).toBe(true);
  });

  it('carries the session, the split, and the themes as given', () => {
    const state = buildMenuState(input({ live: true, splitOpen: true }));
    expect(state).toMatchObject({
      connected: true,
      worldName: 'The Forsaken Lands',
      splitOpen: true,
      theme: 'nord',
    });
    expect(state.themes).toEqual([
      { id: 'nord', label: 'Nord', custom: false },
      { id: 'mine', label: 'Nord (custom)', custom: true },
    ]);
  });

  it('carries a redial, which shows Disconnect while a try waits', () => {
    expect(buildMenuState(input())).toMatchObject({ connected: false, redialing: false });
    expect(buildMenuState(input({ redialing: true }))).toMatchObject({
      connected: false,
      redialing: true,
    });
  });

  it('carries how many sessions are open and whether the sidebar shows', () => {
    expect(buildMenuState(input())).toMatchObject({ sessions: 1, sessionsShown: false });
    expect(buildMenuState(input({ sessions: 3, sessionsShown: true }))).toMatchObject({
      sessions: 3,
      sessionsShown: true,
    });
  });

  it('carries how many snoops the session has, which lists Go to snoop', () => {
    expect(buildMenuState(input()).snoops).toBe(0);
    expect(buildMenuState(input({ snoops: 2 })).snoops).toBe(2);
  });

  it('sends no world name when there is none to show', () => {
    expect(buildMenuState(input({ worldName: '  ' })).worldName).toBeNull();
    expect(buildMenuState(input({ worldName: null })).worldName).toBeNull();
  });
});

describe('setAppMenuState', () => {
  beforeEach(() => {
    resetAppMenuState();
    vi.mocked(invoke).mockClear();
  });

  it('sends a snapshot once and skips one the menu already has', () => {
    const state = buildMenuState({
      live: false,
      redialing: false,
      worldName: null,
      panelOpen: true,
      splitOpen: false,
      shownPanes: [],
      staffOffered: false,
      themes: [],
      theme: 'nord',
      sessions: 1,
      sessionsShown: false,
      snoops: 0,
    });
    setAppMenuState(state);
    setAppMenuState({ ...state });
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('menu_set_state', { state });
    setAppMenuState({ ...state, panelOpen: false });
    expect(invoke).toHaveBeenCalledTimes(2);
  });
});

describe('pageHasSelection', () => {
  const doc = (
    active: { selectionStart?: unknown; selectionEnd?: unknown } | null,
    selected = '',
  ) => ({
    activeElement: active as Element | null,
    getSelection: () => ({ isCollapsed: selected.length === 0, toString: () => selected }),
  });

  it('sees a range in the focused field', () => {
    expect(pageHasSelection(doc({ selectionStart: 2, selectionEnd: 5 }))).toBe(true);
    expect(pageHasSelection(doc({ selectionStart: 3, selectionEnd: 3 }))).toBe(false);
  });

  it('sees text selected anywhere in the page', () => {
    expect(pageHasSelection(doc(null, 'Blackwatch Village Square'))).toBe(true);
    expect(pageHasSelection(doc({}, ''))).toBe(false);
  });

  it('ignores a field type without a selection', () => {
    expect(pageHasSelection(doc({ selectionStart: null, selectionEnd: null }))).toBe(false);
  });
});
