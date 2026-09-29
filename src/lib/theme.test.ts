import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { pickTheme, resolveActiveTheme, themeAppearance, type ThemePrefs } from './theme';

const { setTheme, emit } = vi.hoisted(() => ({
  setTheme: vi.fn((_theme?: string | null) => Promise.resolve()),
  emit: vi.fn((_event: string, _payload?: unknown) => Promise.resolve()),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTheme }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit,
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

const prefs = (patch: Partial<ThemePrefs> = {}): ThemePrefs => ({
  theme: 'nord',
  follow_system_appearance: false,
  light_theme: 'vellum',
  dark_theme: 'tokyo-night',
  ...patch,
});

describe('resolveActiveTheme', () => {
  it('shows the manual pick while follow is off', () => {
    expect(resolveActiveTheme(prefs(), true)).toBe('nord');
    expect(resolveActiveTheme(prefs(), false)).toBe('nord');
  });

  it('shows the pair entry that matches the OS while follow is on', () => {
    const ui = prefs({ follow_system_appearance: true });
    expect(resolveActiveTheme(ui, true)).toBe('tokyo-night');
    expect(resolveActiveTheme(ui, false)).toBe('vellum');
  });

  it('falls back to the manual pick when the pair entry is blank', () => {
    const ui = prefs({ follow_system_appearance: true, dark_theme: '', light_theme: '' });
    expect(resolveActiveTheme(ui, true)).toBe('nord');
    expect(resolveActiveTheme(ui, false)).toBe('nord');
  });
});

describe('themeAppearance', () => {
  it('reads each theme by its derived chrome', () => {
    expect(themeAppearance('vellum')).toBe('light');
    expect(themeAppearance('nord')).toBe('dark');
    expect(themeAppearance('obsidian-ember')).toBe('dark');
  });
});

describe('pickTheme', () => {
  it('sets the manual pick while follow is off', () => {
    expect(pickTheme(prefs(), 'gruvbox')).toEqual(prefs({ theme: 'gruvbox' }));
  });

  it('fills the dark slot with a dark pick while follow is on', () => {
    const ui = prefs({ follow_system_appearance: true });
    const next = pickTheme(ui, 'rose-pine');
    expect(next).toEqual({ ...ui, dark_theme: 'rose-pine' });
    expect(resolveActiveTheme(next, true)).toBe('rose-pine');
    // A light OS keeps showing the light theme.
    expect(resolveActiveTheme(next, false)).toBe('vellum');
  });

  it('fills the light slot with a light pick while follow is on', () => {
    const ui = prefs({ follow_system_appearance: true, light_theme: 'classic-vivid' });
    const next = pickTheme(ui, 'vellum');
    expect(next).toEqual({ ...ui, light_theme: 'vellum' });
    expect(next.theme).toBe('nord');
  });

  it('keeps the other fields of a whole config', () => {
    const ui = { ...prefs(), font_size: 14 };
    expect(pickTheme(ui, 'dracula').font_size).toBe(14);
  });
});

describe('applyThemePrefs', () => {
  let dark = true;
  let listeners: Array<() => void> = [];

  beforeEach(() => {
    vi.resetModules();
    setTheme.mockClear();
    emit.mockClear();
    dark = true;
    listeners = [];
    vi.stubGlobal('window', {
      matchMedia: (query: string) => ({
        get matches() {
          return query.includes('dark') ? dark : false;
        },
        addEventListener: (_type: string, fn: () => void) => listeners.push(fn),
        removeEventListener: (_type: string, fn: () => void) => {
          listeners = listeners.filter((l) => l !== fn);
        },
      }),
    });
    vi.stubGlobal('document', {
      documentElement: {
        setAttribute: () => {},
        style: { setProperty: () => {} },
      },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const flip = (toDark: boolean) => {
    dark = toDark;
    for (const l of listeners) l();
  };

  it('shows the manual pick and pins the window appearance while follow is off', async () => {
    const theme = await import('./theme');
    expect(theme.applyThemePrefs(prefs({ theme: 'vellum' }))).toBe('vellum');
    expect(theme.getCurrentThemeId()).toBe('vellum');
    expect(setTheme).toHaveBeenLastCalledWith('light');
    expect(listeners).toHaveLength(0);
  });

  it('follows the OS and lets the window follow it too', async () => {
    const theme = await import('./theme');
    const id = theme.applyThemePrefs(prefs({ follow_system_appearance: true }), {
      broadcastFlips: true,
    });
    expect(id).toBe('tokyo-night');
    expect(setTheme).toHaveBeenLastCalledWith(null);
    expect(listeners).toHaveLength(1);

    flip(false);
    expect(theme.getCurrentThemeId()).toBe('vellum');
    expect(emit).toHaveBeenCalledWith('vosh://theme-changed', 'vellum');

    flip(true);
    expect(theme.getCurrentThemeId()).toBe('tokyo-night');
  });

  it('stops following when follow goes off', async () => {
    const theme = await import('./theme');
    theme.applyThemePrefs(prefs({ follow_system_appearance: true }));
    theme.applyThemePrefs(prefs());
    expect(listeners).toHaveLength(0);
    expect(theme.getCurrentThemeId()).toBe('nord');
    expect(setTheme).toHaveBeenLastCalledWith('dark');
    flip(false);
    expect(theme.getCurrentThemeId()).toBe('nord');
  });

  it('remembers the fields for the next pick', async () => {
    const theme = await import('./theme');
    expect(theme.getThemePrefs()).toBeNull();
    theme.applyThemePrefs({ ...prefs(), follow_system_appearance: true });
    expect(theme.getThemePrefs()).toEqual(prefs({ follow_system_appearance: true }));
  });
});
