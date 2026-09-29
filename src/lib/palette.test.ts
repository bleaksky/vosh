import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  buildPaletteEntries,
  chooseTheme,
  initialSelection,
  paletteSections,
  shortcutKey,
  shortcutKeys,
  shortcutLabel,
  type PaletteDeps,
} from './palette';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTheme: () => Promise.resolve() }),
}));

describe('shortcutKeys', () => {
  it('uses the Apple glyphs and modifier order on macOS', () => {
    expect(shortcutKeys('Mod+Shift+L', true)).toEqual(['⇧', '⌘', 'L']);
    expect(shortcutKeys('Mod+F', true)).toEqual(['⌘', 'F']);
    expect(shortcutKeys('Ctrl+Alt+Shift+Mod+K', true)).toEqual(['⌃', '⌥', '⇧', '⌘', 'K']);
  });

  it('spells the modifiers out on Windows and Linux, with Mod as Ctrl', () => {
    expect(shortcutKeys('Mod+Shift+L', false)).toEqual(['Ctrl', 'Shift', 'L']);
    expect(shortcutKeys('Mod+F', false)).toEqual(['Ctrl', 'F']);
    expect(shortcutKeys('Mod+Ctrl+X', false)).toEqual(['Ctrl', 'X']);
  });

  it('keeps punctuation keys and names the special ones', () => {
    expect(shortcutKeys('Mod+,', true)).toEqual(['⌘', ',']);
    expect(shortcutKeys('Mod+/', false)).toEqual(['Ctrl', '/']);
    expect(shortcutKeys('Mod++', true)).toEqual(['⌘', '+']);
    expect(shortcutKeys('Shift+Enter', true)).toEqual(['⇧', '↩']);
    expect(shortcutKeys('Shift+Enter', false)).toEqual(['Shift', 'Enter']);
    expect(shortcutKeys('Escape', false)).toEqual(['Esc']);
  });
});

describe('shortcutLabel', () => {
  it('runs the glyphs together on macOS and joins with plus elsewhere', () => {
    expect(shortcutLabel('Mod+C', true)).toBe('⌘C');
    expect(shortcutLabel('Mod+Shift+L', true)).toBe('⇧⌘L');
    expect(shortcutLabel('Mod+C', false)).toBe('Ctrl+C');
    expect(shortcutLabel('Mod+Shift+L', false)).toBe('Ctrl+Shift+L');
  });
});

describe('shortcutKey', () => {
  it('matches Latin layouts on the character typed', () => {
    expect(shortcutKey({ key: 'R', code: 'KeyR' })).toBe('r');
    // Dvorak types k from the physical V key.
    expect(shortcutKey({ key: 'k', code: 'KeyV' })).toBe('k');
    expect(shortcutKey({ key: ',', code: 'KeyW' })).toBe(',');
  });

  it('falls back to the physical key on a non-Latin layout', () => {
    expect(shortcutKey({ key: 'к', code: 'KeyR' })).toBe('r');
    expect(shortcutKey({ key: 'Л', code: 'KeyK' })).toBe('k');
    expect(shortcutKey({ key: 'б', code: 'Comma' })).toBe(',');
    expect(shortcutKey({ key: '.', code: 'Slash' })).toBe('.');
    expect(shortcutKey({ key: 'ё', code: 'Backquote' })).toBe('ё');
  });

  it('leaves named keys alone', () => {
    expect(shortcutKey({ key: 'Escape', code: 'Escape' })).toBe('escape');
  });
});

function deps(over: Partial<PaletteDeps> = {}): PaletteDeps {
  return {
    connected: true,
    host: 'play.theforsakenlands.com',
    worldName: 'The Forsaken Lands',
    panelOpen: true,
    togglePanel: () => {},
    splitOpen: false,
    toggleSplit: () => {},
    paneTypes: ['map', 'affects', 'group', 'chat'],
    paneVisible: (pane) => pane === 'map' || pane === 'affects',
    togglePane: () => {},
    openHelp: () => {},
    openFind: () => {},
    openSettingsTab: () => {},
    connect: () => {},
    disconnect: () => {},
    insertInput: () => {},
    ...over,
  };
}

const flat = (sections: ReturnType<typeof paletteSections>) => sections.flatMap((s) => s.rows);

describe('paletteSections', () => {
  it('lists the approved View and Session rows with Disconnect last', () => {
    const sections = paletteSections(buildPaletteEntries(deps()), '', []);
    expect(sections.map((s) => s.label)).toEqual(['View', 'Session']);
    expect(sections[0].rows.map((r) => r.title)).toEqual([
      'Show panel',
      'Split terminal',
      'Choose theme',
    ]);
    expect(sections[0].rows[1].keys).toBe('Mod+\\');
    const rows = flat(sections);
    expect(rows[rows.length - 1].id).toBe('disconnect');
    expect(rows.filter((r) => r.destructive).map((r) => r.id)).toEqual(['disconnect']);
  });

  it('checks the toggles that are on', () => {
    const rows = buildPaletteEntries(deps());
    const checked = rows.filter((r) => r.checked).map((r) => r.id);
    expect(checked).toEqual(['panel', 'pane-map', 'pane-affects']);
    const split = buildPaletteEntries(deps({ splitOpen: true })).find((r) => r.id === 'split');
    expect(split?.checked).toBe(true);
  });

  it('lists a Show row for each pane type the shell offers', () => {
    const ids = (paneTypes: PaletteDeps['paneTypes']) =>
      buildPaletteEntries(deps({ paneTypes }))
        .filter((r) => r.id.startsWith('pane-'))
        .map((r) => r.id);
    expect(ids(['map', 'affects', 'group', 'chat'])).not.toContain('pane-imm');
    expect(ids(['map', 'imm'])).toEqual(['pane-map', 'pane-imm']);
  });

  it('names the world on the connect row when you are offline', () => {
    const rows = buildPaletteEntries(deps({ connected: false }));
    const last = rows[rows.length - 1];
    expect(last.title).toBe('Connect to The Forsaken Lands');
    expect(last.destructive).toBeFalsy();
  });

  it('leads with Recent and keeps destructive commands out of it', () => {
    const sections = paletteSections(buildPaletteEntries(deps()), '', [
      'disconnect',
      'find',
      'settings-themes',
      'gone',
      'profile-save',
      'help',
    ]);
    expect(sections[0].label).toBe('Recent');
    expect(sections[0].rows.map((r) => r.id)).toEqual(['find', 'settings-themes', 'profile-save']);
  });

  it('hides search only rows until you type, then ranks matches by section', () => {
    const entries = buildPaletteEntries(deps());
    expect(flat(paletteSections(entries, '', [])).some((r) => r.searchOnly)).toBe(false);
    const sections = paletteSections(entries, 'settings', ['find']);
    expect(sections.map((s) => s.label)).toEqual(['View']);
    expect(sections[0].rows[0].id).toBe('settings');
    expect(sections[0].rows.map((r) => r.id)).toContain('settings-themes');
  });
});

describe('initialSelection', () => {
  it('never opens on a destructive row', () => {
    const entries = buildPaletteEntries(deps());
    expect(initialSelection(flat(paletteSections(entries, '', [])))).toBe(0);
    const onlyDisconnect = flat(paletteSections(entries, 'disconnect', []));
    expect(onlyDisconnect.map((r) => r.id)).toEqual(['disconnect']);
    expect(initialSelection(onlyDisconnect)).toBe(-1);
  });

  it('selects a destructive row you typed for', () => {
    const entries = buildPaletteEntries(deps());
    const onlyDisconnect = flat(paletteSections(entries, 'disconnect', []));
    expect(initialSelection(onlyDisconnect, 'disconnect')).toBe(0);
    expect(initialSelection([], 'nothing matches')).toBe(-1);
  });
});

describe('chooseTheme', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
    // A dark OS.
    vi.stubGlobal('window', {
      matchMedia: (query: string) => ({
        matches: query.includes('dark'),
        addEventListener: () => {},
        removeEventListener: () => {},
      }),
    });
    vi.stubGlobal('document', {
      documentElement: { setAttribute: () => {}, style: { setProperty: () => {} } },
    });
  });

  afterEach(async () => {
    const { applyThemePrefs } = await import('./theme');
    applyThemePrefs({
      theme: 'obsidian-ember',
      follow_system_appearance: false,
      light_theme: 'vellum',
      dark_theme: 'obsidian-ember',
    });
    vi.unstubAllGlobals();
  });

  it('sets the manual pick while follow is off', async () => {
    const { applyThemePrefs, getCurrentThemeId, getThemePrefs } = await import('./theme');
    applyThemePrefs({
      theme: 'nord',
      follow_system_appearance: false,
      light_theme: 'vellum',
      dark_theme: 'nord',
    });
    await chooseTheme('gruvbox');
    expect(getCurrentThemeId()).toBe('gruvbox');
    expect(getThemePrefs()?.theme).toBe('gruvbox');
    expect(invoke).toHaveBeenCalledWith('ui_set_theme', {
      theme: 'gruvbox',
      lightTheme: 'vellum',
      darkTheme: 'nord',
    });
  });

  it('fills the slot that matches the pick while follow is on', async () => {
    const { applyThemePrefs, getCurrentThemeId, getThemePrefs } = await import('./theme');
    applyThemePrefs({
      theme: 'nord',
      follow_system_appearance: true,
      light_theme: 'vellum',
      dark_theme: 'tokyo-night',
    });
    await chooseTheme('rose-pine');
    expect(getCurrentThemeId()).toBe('rose-pine');
    expect(getThemePrefs()).toMatchObject({ theme: 'nord', dark_theme: 'rose-pine' });

    // A light pick fills the light slot and stays hidden on a dark OS.
    const { customToAppTheme, setCustomThemes } = await import('./themes');
    setCustomThemes([
      customToAppTheme({
        id: 'paper',
        label: 'Paper',
        description: '',
        xterm: { background: '#ffffff', foreground: '#222222' },
        chrome: {},
      }),
    ]);
    await chooseTheme('paper');
    setCustomThemes([]);
    expect(getThemePrefs()).toMatchObject({ light_theme: 'paper', dark_theme: 'rose-pine' });
    expect(getCurrentThemeId()).toBe('rose-pine');
    expect(invoke).toHaveBeenLastCalledWith('ui_set_theme', {
      theme: 'nord',
      lightTheme: 'paper',
      darkTheme: 'rose-pine',
    });
  });
});
