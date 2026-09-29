import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { pickTheme, resolveActiveTheme, themeAppearance, type ThemePrefs } from './theme';

const { setTheme, emit, invoke, getUiConfig } = vi.hoisted(() => ({
  setTheme: vi.fn((_theme?: string | null) => Promise.resolve()),
  emit: vi.fn((_event: string, _payload?: unknown) => Promise.resolve()),
  invoke: vi.fn((_cmd: string, _args?: unknown) => Promise.resolve()),
  // What a window fetches when it applies a theme id it does not know.
  getUiConfig: vi.fn(() => Promise.resolve({ custom_themes: [] as unknown[] })),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTheme }),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  emit,
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('./session', () => ({ getUiConfig }));

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

describe('the paint cache', () => {
  let dark = true;
  let listeners: Array<() => void> = [];
  let stored: Record<string, string> = {};
  let rootAttrs: Record<string, string> = {};

  beforeEach(() => {
    vi.resetModules();
    getUiConfig.mockReset();
    getUiConfig.mockImplementation(() => Promise.resolve({ custom_themes: [] }));
    dark = true;
    listeners = [];
    stored = {};
    rootAttrs = {};
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
      localStorage: {
        getItem: (key: string) => stored[key] ?? null,
        setItem: (key: string, value: string) => {
          stored[key] = value;
        },
      },
    });
    vi.stubGlobal('document', {
      documentElement: {
        setAttribute: (name: string, value: string) => {
          rootAttrs[name] = value;
        },
        style: { setProperty: () => {} },
      },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const cached = async () => {
    const { pageStorage, readThemePaint } = await import('./themePaint');
    return readThemePaint(pageStorage());
  };

  const flip = (toDark: boolean) => {
    dark = toDark;
    for (const l of listeners) l();
  };

  it('leaves the manual pick, tokens and terminal ground included', async () => {
    const theme = await import('./theme');
    const { findTheme, themeTokens } = await import('./themes');
    theme.applyThemePrefs(prefs({ theme: 'vellum' }));
    const paint = await cached();
    expect(paint?.follow).toBe(false);
    if (paint?.follow !== false) return;
    const vellum = findTheme('vellum');
    expect(paint.manual.id).toBe('vellum');
    expect(paint.manual.appearance).toBe('light');
    expect(paint.manual.vars['--bg']).toBe(themeTokens(vellum).bg);
    expect(paint.manual.vars['--text']).toBe(themeTokens(vellum).text);
    expect(paint.manual.vars['--xterm-bg']).toBe(vellum.xterm.background);
    expect(paint.manual).toEqual(theme.themePaintSide(vellum));
  });

  it('leaves both sides while the theme follows the system', async () => {
    const theme = await import('./theme');
    const { findTheme } = await import('./themes');
    theme.applyThemePrefs(prefs({ follow_system_appearance: true }));
    let paint = await cached();
    expect(paint).toEqual({
      v: 1,
      follow: true,
      light: theme.themePaintSide(findTheme('vellum')),
      dark: theme.themePaintSide(findTheme('tokyo-night')),
    });
    // An OS flip repaints and leaves the same pair.
    flip(false);
    paint = await cached();
    expect(paint?.follow && paint.light.id).toBe('vellum');
    expect(paint?.follow && paint.dark.id).toBe('tokyo-night');
  });

  it('skips a theme id that runs ahead of the fields that go with it', async () => {
    const theme = await import('./theme');
    theme.applyThemePrefs(prefs({ theme: 'nord' }));
    theme.applyTheme('gruvbox');
    const paint = await cached();
    expect(paint?.follow === false && paint.manual.id).toBe('nord');
  });

  it('leaves nothing before the window knows the fields', async () => {
    const theme = await import('./theme');
    theme.applyTheme('vellum');
    expect(await cached()).toBeNull();
  });

  it('holds a custom theme by its colors', async () => {
    const theme = await import('./theme');
    const { customToAppTheme, setCustomThemes } = await import('./themes');
    setCustomThemes([
      customToAppTheme({
        id: 'paper',
        label: 'Paper',
        description: '',
        xterm: { background: '#fdfcf8', foreground: '#222222' },
        chrome: {},
      }),
    ]);
    theme.applyThemePrefs(prefs({ theme: 'paper' }));
    const paint = await cached();
    expect(paint?.follow === false && paint.manual.vars['--xterm-bg']).toBe('#fdfcf8');
    expect(paint?.follow === false && paint.manual.appearance).toBe('light');
  });

  const backdrops = () =>
    invoke.mock.calls.filter(([cmd]) => cmd === 'window_backdrop_set').map(([, args]) => args);

  it('tells the backend what a new window opens on', async () => {
    invoke.mockClear();
    const theme = await import('./theme');
    const { findTheme, themeTokens } = await import('./themes');
    theme.applyThemePrefs(prefs({ theme: 'vellum' }));
    expect(backdrops()).toEqual([
      { background: themeTokens(findTheme('vellum')).bg, appearance: 'light' },
    ]);
  });

  it('leaves the native appearance to the system while the theme follows it', async () => {
    invoke.mockClear();
    const theme = await import('./theme');
    const { findTheme, themeTokens } = await import('./themes');
    theme.applyThemePrefs(prefs({ follow_system_appearance: true }));
    flip(false);
    expect(backdrops()).toEqual([
      { background: themeTokens(findTheme('tokyo-night')).bg, appearance: null },
      { background: themeTokens(findTheme('vellum')).bg, appearance: null },
    ]);
  });

  it('reports no backdrop for a theme id ahead of its fields', async () => {
    const theme = await import('./theme');
    theme.applyThemePrefs(prefs({ theme: 'nord' }));
    invoke.mockClear();
    theme.applyTheme('gruvbox');
    expect(backdrops()).toEqual([]);
  });

  it('starts on the theme the startup paint put on screen', async () => {
    const first = await import('./theme');
    expect(first.getCurrentThemeId()).toBe('obsidian-ember');
    first.applyThemePrefs(prefs({ theme: 'vellum' }));

    // The next window starts from that cache. The terminal reads the
    // id when it mounts, before the config arrives.
    vi.resetModules();
    const { prepaintTheme } = await import('./themePaint');
    prepaintTheme();
    const theme = await import('./theme');
    expect(theme.getCurrentThemeId()).toBe('vellum');
  });

  it('says whether the startup paint already shows the active theme', async () => {
    const first = await import('./theme');
    first.applyThemePrefs(prefs({ theme: 'vellum' }));

    // The next window starts from that cache.
    vi.resetModules();
    const { prepaintTheme } = await import('./themePaint');
    const theme = await import('./theme');
    expect(prepaintTheme()?.id).toBe('vellum');
    expect(theme.paintMatchesBoot()).toBe(false);
    theme.applyThemePrefs(prefs({ theme: 'vellum' }));
    expect(theme.paintMatchesBoot()).toBe(true);
    theme.applyThemePrefs(prefs({ theme: 'nord' }));
    expect(theme.paintMatchesBoot()).toBe(false);
  });

  // A profile can name a theme this build does not have: a hand edited
  // or imported profile.toml, a custom theme deleted elsewhere, or a
  // theme from a newer build. The window shows the first built in theme
  // instead, and once the catalog confirms the id is gone, that is the
  // theme the next window has to open on.
  const manualId = async () => {
    const paint = await cached();
    return paint?.follow === false ? paint.manual.id : null;
  };

  const gone = {
    id: 'gone',
    label: 'Gone',
    description: '',
    xterm: { background: '#fdfcf8', foreground: '#222222' },
    chrome: {},
  };

  it('leaves the theme on screen once the saved theme turns out to be gone', async () => {
    const theme = await import('./theme');
    const { findTheme, themeTokens } = await import('./themes');
    theme.applyThemePrefs(prefs({ theme: 'vellum' }));
    invoke.mockClear();
    theme.applyThemePrefs(prefs({ theme: 'missing' }));
    expect(rootAttrs['data-theme']).toBe('obsidian-ember');
    await vi.waitFor(async () => expect(await manualId()).toBe('obsidian-ember'));
    expect(backdrops()).toEqual([
      { background: themeTokens(findTheme('obsidian-ember')).bg, appearance: 'dark' },
    ]);

    // The next window opens on it, and its first frame already holds
    // the theme it shows.
    vi.resetModules();
    const { prepaintTheme } = await import('./themePaint');
    expect(prepaintTheme()?.id).toBe('obsidian-ember');
    const next = await import('./theme');
    next.applyThemePrefs(prefs({ theme: 'missing' }));
    expect(next.paintMatchesBoot()).toBe(true);
  });

  it('leaves the default theme for a blank theme field', async () => {
    const theme = await import('./theme');
    theme.applyThemePrefs(prefs({ theme: 'vellum' }));
    theme.applyThemePrefs(prefs({ theme: '' }));
    expect(rootAttrs['data-theme']).toBe('obsidian-ember');
    expect(await manualId()).toBe('obsidian-ember');
  });

  it('waits while a theme it does not know may still be loading', async () => {
    let answer: (cfg: { custom_themes: unknown[] }) => void = () => {};
    getUiConfig.mockImplementation(
      () => new Promise<{ custom_themes: unknown[] }>((resolve) => (answer = resolve)),
    );
    const theme = await import('./theme');
    theme.applyThemePrefs(prefs({ theme: 'vellum' }));
    invoke.mockClear();
    theme.applyThemePrefs(prefs({ theme: 'gone' }));
    expect(rootAttrs['data-theme']).toBe('obsidian-ember');
    expect(await manualId()).toBe('vellum');
    expect(backdrops()).toEqual([]);

    // Another window had saved it, and the catalog brings it in.
    await vi.waitFor(() => expect(getUiConfig).toHaveBeenCalled());
    answer({ custom_themes: [gone] });
    await vi.waitFor(async () => expect(await manualId()).toBe('gone'));
    expect(rootAttrs['data-theme']).toBe('gone');
  });

  it('keeps a later pick over a slow catalog answer', async () => {
    let answer: (cfg: { custom_themes: unknown[] }) => void = () => {};
    getUiConfig.mockImplementation(
      () => new Promise<{ custom_themes: unknown[] }>((resolve) => (answer = resolve)),
    );
    const theme = await import('./theme');
    theme.applyThemePrefs(prefs({ theme: 'gone' }));
    theme.applyThemePrefs(prefs({ theme: 'nord' }));
    await vi.waitFor(() => expect(getUiConfig).toHaveBeenCalled());
    answer({ custom_themes: [gone] });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(rootAttrs['data-theme']).toBe('nord');
    expect(theme.getCurrentThemeId()).toBe('nord');
    expect(await manualId()).toBe('nord');
  });

  // Editing a custom theme saves it without a repaint unless it is the
  // theme on screen. While the theme follows the system, the side that
  // is not on screen still has to reach the cache, for a launch after
  // the OS flips while Vosh is closed.
  const night = (background: string) => ({
    id: 'night',
    label: 'Night',
    description: '',
    xterm: { background, foreground: '#dddddd' },
    chrome: {},
  });

  it('caches the new colors of the custom theme on the side not on screen', async () => {
    const theme = await import('./theme');
    const { customToAppTheme, setCustomThemes } = await import('./themes');
    setCustomThemes([customToAppTheme(night('#101010'))]);
    dark = false;
    theme.applyThemePrefs(prefs({ follow_system_appearance: true, dark_theme: 'night' }));
    invoke.mockClear();

    setCustomThemes([customToAppTheme(night('#302010'))]);
    const paint = await cached();
    expect(paint?.follow === true && paint.dark.vars['--xterm-bg']).toBe('#302010');
    expect(paint?.follow === true && paint.light.id).toBe('vellum');
    // The theme on screen did not change, so neither did the backdrop.
    expect(backdrops()).toEqual([]);
  });

  it('caches the custom theme on screen once the window repaints it', async () => {
    const theme = await import('./theme');
    const { customToAppTheme, setCustomThemes } = await import('./themes');
    setCustomThemes([customToAppTheme(night('#101010'))]);
    theme.applyThemePrefs(prefs({ theme: 'night' }));

    // Until the repaint the cache holds what the window shows.
    setCustomThemes([customToAppTheme(night('#302010'))]);
    let paint = await cached();
    expect(paint?.follow === false && paint.manual.vars['--xterm-bg']).toBe('#101010');

    theme.applyTheme('night');
    paint = await cached();
    expect(paint?.follow === false && paint.manual.vars['--xterm-bg']).toBe('#302010');
  });
});
