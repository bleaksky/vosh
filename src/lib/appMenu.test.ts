import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  APP_SHORTCUTS,
  buildMenuState,
  commandBlocked,
  commandRepeats,
  pageHasSelection,
  resetAppMenuState,
  resolveShortcut,
  setAppMenuState,
  type MenuStateInput,
} from './appMenu';
import { buildPaletteEntries, type PaletteDeps } from './palette';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTheme: () => Promise.resolve() }),
}));

describe('resolveShortcut', () => {
  it('maps each window shortcut to its command', () => {
    expect(resolveShortcut('k', false)).toEqual({ id: 'palette' });
    expect(resolveShortcut('f', false)).toEqual({ id: 'find' });
    expect(resolveShortcut('r', false)).toEqual({ id: 'connect' });
    expect(resolveShortcut(',', false)).toEqual({ id: 'settings' });
    expect(resolveShortcut('/', false)).toEqual({ id: 'help' });
    expect(resolveShortcut('\\', false)).toEqual({ id: 'split' });
    expect(resolveShortcut('l', true)).toEqual({ id: 'panel' });
  });

  it('takes Shift+R without running anything, so the page never reloads', () => {
    expect(resolveShortcut('r', true)).toEqual({ id: null });
  });

  it('leaves other keys to the page and the menu bar', () => {
    expect(resolveShortcut('l', false)).toBeNull();
    expect(resolveShortcut('k', true)).toBeNull();
    // Copy and Close window belong to the fields and the menu bar.
    expect(resolveShortcut('c', false)).toBeNull();
    expect(resolveShortcut('w', false)).toBeNull();
  });
});

describe('command gates', () => {
  it('stands the palette and the find bar down while help is open', () => {
    expect(commandBlocked('palette', { helpOpen: true })).toBe(true);
    expect(commandBlocked('find', { helpOpen: true })).toBe(true);
    expect(commandBlocked('palette', { helpOpen: false })).toBe(false);
    expect(commandBlocked('help', { helpOpen: true })).toBe(false);
    expect(commandBlocked('connect', { helpOpen: true })).toBe(false);
  });

  it('repeats only find and help on a held key', () => {
    expect(commandRepeats('find')).toBe(true);
    expect(commandRepeats('help')).toBe(true);
    for (const id of ['palette', 'connect', 'panel', 'split', 'settings']) {
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
