import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  APP_SHORTCUTS,
  buildMenuState,
  commandRepeats,
  pageHasSelection,
  resetAppMenuState,
  resolveShortcut,
  sessionKeyOfMacro,
  setAppMenuState,
  type MenuStateInput,
} from './appMenu';
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
    expect(resolveShortcut(press('!', 'Digit1', true))).toBeNull();
    expect(resolveShortcut(press('1', 'Numpad1'))).toBeNull();
  });

  it('leaves a session key to a macro the profile binds to it', () => {
    const bound = () => true;
    expect(resolveShortcut(press('1', 'Digit1'), bound)).toEqual({ kind: 'macro' });
    expect(resolveShortcut(letter('t'), bound)).toEqual({ kind: 'macro' });
    expect(resolveShortcut(letter('w'), bound)).toEqual({ kind: 'macro' });
    expect(resolveShortcut(letter('w', true), bound)).toEqual({ kind: 'macro' });
    expect(resolveShortcut(press('}', 'BracketRight', true), bound)).toEqual({ kind: 'macro' });
    // Every other app key wins over a macro, as before.
    expect(resolveShortcut(letter('k'), bound)).toEqual({ kind: 'run', id: 'palette' });
    expect(resolveShortcut(letter('r'), bound)).toEqual({ kind: 'run', id: 'connect' });
  });

  it('asks after a macro only for a session key', () => {
    const bound = vi.fn(() => false);
    resolveShortcut(letter('k'), bound);
    resolveShortcut(letter('c'), bound);
    expect(bound).not.toHaveBeenCalled();
    resolveShortcut(press('2', 'Digit2'), bound);
    expect(bound).toHaveBeenCalledTimes(1);
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

describe('sessionKeyOfMacro', () => {
  it('finds the session key a macro key shares on macOS', () => {
    expect(sessionKeyOfMacro('Meta+1', true)).toEqual({ kind: 'goto', place: 1 });
    expect(sessionKeyOfMacro('Meta+9', true)).toEqual({ kind: 'goto', place: 9 });
    expect(sessionKeyOfMacro('Meta+T', true)).toEqual({ kind: 'run', id: 'session-new' });
    expect(sessionKeyOfMacro('Meta+W', true)).toEqual({ kind: 'run', id: 'session-close' });
    expect(sessionKeyOfMacro('Shift+Meta+W', true)).toEqual({ kind: 'run', id: 'close-window' });
    // Shift with ] types } on a US layout, and either reads as the key.
    expect(sessionKeyOfMacro('Shift+Meta+}', true)).toEqual({ kind: 'run', id: 'session-next' });
    expect(sessionKeyOfMacro('Shift+Meta+]', true)).toEqual({ kind: 'run', id: 'session-next' });
    expect(sessionKeyOfMacro('Shift+Meta+{', true)).toEqual({
      kind: 'run',
      id: 'session-previous',
    });
    // Ctrl belongs to your macros on macOS, and no app key takes it.
    expect(sessionKeyOfMacro('Ctrl+1', true)).toBeNull();
    expect(sessionKeyOfMacro('Meta+0', true)).toBeNull();
    expect(sessionKeyOfMacro('Meta+K', true)).toBeNull();
    expect(sessionKeyOfMacro('F1', true)).toBeNull();
  });

  it('reads Ctrl on Windows and Linux', () => {
    expect(sessionKeyOfMacro('Ctrl+2', false)).toEqual({ kind: 'goto', place: 2 });
    expect(sessionKeyOfMacro('Ctrl+Shift+W', false)).toEqual({ kind: 'run', id: 'close-window' });
    expect(sessionKeyOfMacro('Ctrl+Shift+[', false)).toEqual({
      kind: 'run',
      id: 'session-previous',
    });
    expect(sessionKeyOfMacro('Meta+1', false)).toBeNull();
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

  it('uses Mod for every menu shortcut, never Ctrl', () => {
    for (const spec of Object.values(APP_SHORTCUTS)) {
      expect(spec.startsWith('Mod+')).toBe(true);
      expect(spec.toLowerCase()).not.toContain('ctrl');
    }
  });
});

describe('buildMenuState', () => {
  const input = (over: Partial<MenuStateInput> = {}): MenuStateInput => ({
    live: false,
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

  it('carries how many sessions are open and whether the sidebar shows', () => {
    expect(buildMenuState(input())).toMatchObject({ sessions: 1, sessionsShown: false });
    expect(buildMenuState(input({ sessions: 3, sessionsShown: true }))).toMatchObject({
      sessions: 3,
      sessionsShown: true,
    });
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
      worldName: null,
      panelOpen: true,
      splitOpen: false,
      shownPanes: [],
      staffOffered: false,
      themes: [],
      theme: 'nord',
      sessions: 1,
      sessionsShown: false,
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
