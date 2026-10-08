import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { pickTheme, resolveActiveTheme, themeAppearance, type ThemePrefs } from './theme';

type Handler = (event: { payload: unknown }) => void;

const { setTheme, emit, invoke, getUiConfig, handlers } = vi.hoisted(() => ({
  setTheme: vi.fn((_theme?: string | null) => Promise.resolve()),
  emit: vi.fn((_event: string, _payload?: unknown) => Promise.resolve()),
  invoke: vi.fn((_cmd: string, _args?: unknown): Promise<unknown> => Promise.resolve()),
  // Every listener a test module adds, by event, so a test can fire one.
  handlers: new Map<string, Set<Handler>>(),
  // What a window fetches when it applies a theme id it does not know.
  getUiConfig: vi.fn(() => Promise.resolve({ custom_themes: [] as unknown[] })),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTheme }),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  emit,
  listen: vi.fn((event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return Promise.resolve(() => set.delete(cb));
  }),
}));
vi.mock('../ipc/uiConfig', async (original) => ({
  ...(await original<typeof import('../ipc/uiConfig')>()),
  getUiConfig,
}));

const prefs = (patch: Partial<ThemePrefs> = {}): ThemePrefs => ({
  theme: 'nord',
  follow_system_appearance: false,
  light_theme: 'rubric',
  dark_theme: 'tokyo-night',
  theme_follow: 'off',
  day_theme: '',
  night_theme: '',
  ...patch,
});

describe('resolveActiveTheme', () => {
  it('shows the manual pick while follow is off', () => {
    expect(resolveActiveTheme(prefs(), true, null)).toBe('nord');
    expect(resolveActiveTheme(prefs(), false, null)).toBe('nord');
  });

  it('shows the pair entry that matches the OS while follow is on', () => {
    const ui = prefs({ follow_system_appearance: true });
    expect(resolveActiveTheme(ui, true, null)).toBe('tokyo-night');
    expect(resolveActiveTheme(ui, false, null)).toBe('rubric');
  });

  it('falls back to the manual pick when the pair entry is blank', () => {
    const ui = prefs({ follow_system_appearance: true, dark_theme: '', light_theme: '' });
    expect(resolveActiveTheme(ui, true, null)).toBe('nord');
    expect(resolveActiveTheme(ui, false, null)).toBe('nord');
  });
});

describe('resolveActiveTheme with the game', () => {
  const ui = prefs({ theme_follow: 'game', day_theme: 'gruvbox', night_theme: 'obsidian-ember' });

  it('shows the day theme by day and the night theme by night, whatever the OS', () => {
    expect(resolveActiveTheme(ui, true, 'day')).toBe('gruvbox');
    expect(resolveActiveTheme(ui, false, 'night')).toBe('obsidian-ember');
  });

  it('shows the manual pick before the game says, and for a slot left empty', () => {
    expect(resolveActiveTheme(ui, true, null)).toBe('nord');
    expect(resolveActiveTheme({ ...ui, day_theme: '' }, true, 'day')).toBe('nord');
  });

  it('follows the game even where an older Vosh left follow system on', () => {
    const both = { ...ui, follow_system_appearance: true };
    expect(resolveActiveTheme(both, true, 'day')).toBe('gruvbox');
  });
});

describe('themeAppearance', () => {
  it('reads each theme by its derived chrome', () => {
    expect(themeAppearance('rubric')).toBe('light');
    expect(themeAppearance('nord')).toBe('dark');
    expect(themeAppearance('obsidian-ember')).toBe('dark');
  });

  it('reads a retired id by the theme that took its place', () => {
    expect(themeAppearance('vellum')).toBe('light');
    expect(themeAppearance('everforest-light')).toBe('light');
    expect(themeAppearance('one-dark')).toBe('dark');
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
    expect(resolveActiveTheme(next, true, null)).toBe('rose-pine');
    // A light OS keeps showing the light theme.
    expect(resolveActiveTheme(next, false, null)).toBe('rubric');
  });

  it('fills the light slot with a light pick while follow is on', () => {
    const ui = prefs({ follow_system_appearance: true, light_theme: 'classic-vivid' });
    const next = pickTheme(ui, 'rubric');
    expect(next).toEqual({ ...ui, light_theme: 'rubric' });
    expect(next.theme).toBe('nord');
  });

  it('fills the slot of the theme a retired id shows while follow is on', () => {
    const ui = prefs({ follow_system_appearance: true });
    expect(pickTheme(ui, 'one-dark')).toEqual({ ...ui, dark_theme: 'one-dark' });
    expect(pickTheme(ui, 'everforest-light')).toEqual({ ...ui, light_theme: 'everforest-light' });
  });

  it('fills the slot showing now while the theme follows the game', () => {
    const ui = prefs({ theme_follow: 'game', day_theme: 'nord', night_theme: 'nord' });
    // A light pick by night fills the night slot all the same.
    expect(pickTheme(ui, 'rubric', 'night')).toEqual({ ...ui, night_theme: 'rubric' });
    expect(pickTheme(ui, 'obsidian-ember', 'day')).toEqual({ ...ui, day_theme: 'obsidian-ember' });
    // Before the game says, the manual pick is what shows.
    expect(pickTheme(ui, 'gruvbox', null)).toEqual({ ...ui, theme: 'gruvbox' });
  });

  it('shows the high contrast pair under Increase contrast while it follows the system', () => {
    const ui = prefs({ follow_system_appearance: true });
    expect(resolveActiveTheme(ui, true, null, true)).toBe('high-contrast');
    expect(resolveActiveTheme(ui, false, null, true)).toBe('high-contrast-light');
    const blank = prefs({ follow_system_appearance: true, dark_theme: '', light_theme: '' });
    expect(resolveActiveTheme(blank, true, null, true)).toBe('high-contrast');
    expect(resolveActiveTheme(blank, false, null, true)).toBe('high-contrast-light');
  });

  it('leaves Increase contrast alone while off or following the game', () => {
    expect(resolveActiveTheme(prefs(), true, null, true)).toBe('nord');
    const game = prefs({ theme_follow: 'game', day_theme: 'rubric', night_theme: 'dracula' });
    expect(resolveActiveTheme(game, true, 'day', true)).toBe('rubric');
    expect(resolveActiveTheme(game, false, 'night', true)).toBe('dracula');
    expect(resolveActiveTheme(game, false, null, true)).toBe('nord');
  });

  it('keeps the other fields of a whole config', () => {
    const ui = { ...prefs(), font_size: 14 };
    expect(pickTheme(ui, 'dracula').font_size).toBe(14);
  });
});

describe('applyThemePrefs', () => {
  let dark = true;
  let more = false;
  let listeners: Array<() => void> = [];

  beforeEach(() => {
    vi.resetModules();
    setTheme.mockClear();
    emit.mockClear();
    dark = true;
    more = false;
    listeners = [];
    vi.stubGlobal('window', {
      matchMedia: (query: string) => ({
        get matches() {
          return query.includes('dark') ? dark : query.includes('contrast') ? more : false;
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
    expect(theme.applyThemePrefs(prefs({ theme: 'rubric' }))).toBe('rubric');
    expect(theme.getCurrentThemeId()).toBe('rubric');
    expect(setTheme).toHaveBeenLastCalledWith('light');
    expect(listeners).toHaveLength(0);
  });

  it('shows the successor of a retired id and keeps the saved id', async () => {
    const theme = await import('./theme');
    expect(theme.applyThemePrefs(prefs({ theme: 'vellum' }))).toBe('vellum');
    expect(theme.getCurrentThemeId()).toBe('rubric');
    expect(theme.getThemePrefs()?.theme).toBe('vellum');
    expect(setTheme).toHaveBeenLastCalledWith('light');
  });

  it('follows the OS between the successors of retired ids', async () => {
    const theme = await import('./theme');
    theme.applyThemePrefs(
      prefs({ follow_system_appearance: true, light_theme: 'vellum', dark_theme: 'one-dark' }),
    );
    expect(theme.getCurrentThemeId()).toBe('one-half-dark');
    flip(false);
    expect(theme.getCurrentThemeId()).toBe('rubric');
    flip(true);
    expect(theme.getCurrentThemeId()).toBe('one-half-dark');
  });

  it('follows the OS and lets the window follow it too', async () => {
    const theme = await import('./theme');
    const id = theme.applyThemePrefs(prefs({ follow_system_appearance: true }), {
      broadcastFlips: true,
    });
    expect(id).toBe('tokyo-night');
    expect(setTheme).toHaveBeenLastCalledWith(null);
    // One listener for the appearance, one for Increase contrast.
    expect(listeners).toHaveLength(2);

    flip(false);
    expect(theme.getCurrentThemeId()).toBe('rubric');
    expect(emit).toHaveBeenCalledWith('vosh://theme-changed', 'rubric');

    flip(true);
    expect(theme.getCurrentThemeId()).toBe('tokyo-night');
  });

  it('swaps in the high contrast pair when Increase contrast goes on', async () => {
    const theme = await import('./theme');
    theme.applyThemePrefs(prefs({ follow_system_appearance: true }), { broadcastFlips: true });
    expect(theme.getCurrentThemeId()).toBe('tokyo-night');
    more = true;
    for (const l of listeners) l();
    expect(theme.getCurrentThemeId()).toBe('high-contrast');
    expect(emit).toHaveBeenCalledWith('vosh://theme-changed', 'high-contrast');
    flip(false);
    expect(theme.getCurrentThemeId()).toBe('high-contrast-light');
    expect(emit).toHaveBeenCalledWith('vosh://theme-changed', 'high-contrast-light');
    more = false;
    for (const l of listeners) l();
    expect(theme.getCurrentThemeId()).toBe('rubric');
  });

  it('opens on the high contrast pair when Increase contrast is already on', async () => {
    more = true;
    const theme = await import('./theme');
    expect(theme.applyThemePrefs(prefs({ follow_system_appearance: true }))).toBe('high-contrast');
    expect(theme.applyThemePrefs(prefs())).toBe('nord');
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
  let more = false;
  let listeners: Array<() => void> = [];
  let stored: Record<string, string> = {};
  let rootAttrs: Record<string, string> = {};

  beforeEach(() => {
    vi.resetModules();
    getUiConfig.mockReset();
    getUiConfig.mockImplementation(() => Promise.resolve({ custom_themes: [] }));
    dark = true;
    more = false;
    listeners = [];
    stored = {};
    rootAttrs = {};
    vi.stubGlobal('window', {
      matchMedia: (query: string) => ({
        get matches() {
          return query.includes('dark') ? dark : query.includes('contrast') ? more : false;
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
    theme.applyThemePrefs(prefs({ theme: 'rubric' }));
    const paint = await cached();
    expect(paint?.follow).toBe(false);
    if (paint?.follow !== false) return;
    const rubric = findTheme('rubric');
    expect(paint.manual.id).toBe('rubric');
    expect(paint.manual.appearance).toBe('light');
    expect(paint.manual.vars['--bg']).toBe(themeTokens(rubric).bg);
    expect(paint.manual.vars['--text']).toBe(themeTokens(rubric).text);
    expect(paint.manual.vars['--xterm-bg']).toBe(rubric.xterm.background);
    expect(paint.manual).toEqual(theme.themePaintSide(rubric));
  });

  // Your color vision tunes the status colors every window paints, and
  // the paint the next window opens on.
  it('paints the status colors for your color vision and leaves them for the next window', async () => {
    const theme = await import('./theme');
    const { findTheme, themeTokens } = await import('./themes');
    const kanso = findTheme('kanso-zen');
    const typical = themeTokens(kanso);
    const deutan = themeTokens(kanso, 'deuteranopia');
    expect(deutan.success).not.toBe(typical.success);
    theme.applyThemePrefs({ ...prefs({ theme: 'kanso-zen' }), color_vision: 'deuteranopia' });
    expect(theme.getColorVision()).toBe('deuteranopia');
    let paint = await cached();
    if (paint?.follow !== false) throw new Error('no manual paint');
    expect(paint.manual.vars['--danger']).toBe(deutan.danger);
    expect(paint.manual.vars['--success']).toBe(deutan.success);
    // The seven fields another window sends keep the vision.
    theme.applyThemePrefs(prefs({ theme: 'kanso-zen' }));
    expect(theme.getColorVision()).toBe('deuteranopia');
    // A new vision paints the theme on screen again at once.
    const heard = vi.fn();
    const stop = theme.subscribeColorVision(heard);
    theme.setColorVision('typical');
    expect(heard).toHaveBeenCalledTimes(1);
    paint = await cached();
    if (paint?.follow !== false) throw new Error('no manual paint');
    expect(paint.manual.vars['--success']).toBe(typical.success);
    expect(paint.manual).toEqual(theme.themePaintSide(kanso, 'typical'));
    stop();
  });

  it('leaves both sides while the theme follows the system', async () => {
    const theme = await import('./theme');
    const { findTheme } = await import('./themes');
    theme.applyThemePrefs(prefs({ follow_system_appearance: true }));
    let paint = await cached();
    expect(paint).toEqual({
      v: 1,
      follow: true,
      light: theme.themePaintSide(findTheme('rubric')),
      dark: theme.themePaintSide(findTheme('tokyo-night')),
      more: {
        light: theme.themePaintSide(findTheme('high-contrast-light')),
        dark: theme.themePaintSide(findTheme('high-contrast')),
      },
    });
    // An OS flip repaints and leaves the same pair.
    flip(false);
    paint = await cached();
    expect(paint?.follow === true && paint.light.id).toBe('rubric');
    expect(paint?.follow === true && paint.dark.id).toBe('tokyo-night');
  });

  it('leaves all four sides while Increase contrast shows the pair', async () => {
    more = true;
    const theme = await import('./theme');
    theme.applyThemePrefs(prefs({ follow_system_appearance: true }));
    expect(rootAttrs['data-theme']).toBe('high-contrast');
    const paint = await cached();
    expect(paint?.follow === true && [paint.light.id, paint.dark.id]).toEqual([
      'rubric',
      'tokyo-night',
    ]);
    expect(
      paint?.follow === true && paint.more && [paint.more.light.id, paint.more.dark.id],
    ).toEqual(['high-contrast-light', 'high-contrast']);
  });

  it('leaves the successor of a retired id at once, with no catalog to wait on', async () => {
    const theme = await import('./theme');
    const { findTheme } = await import('./themes');
    theme.applyThemePrefs(prefs({ theme: 'vellum' }));
    expect(rootAttrs['data-theme']).toBe('rubric');
    const paint = await cached();
    expect(paint?.follow === false && paint.manual).toEqual(
      theme.themePaintSide(findTheme('rubric')),
    );
    // A retired id is not a custom theme this window has yet to load.
    expect(getUiConfig).not.toHaveBeenCalled();
  });

  it('leaves Rubric for a saved vellum on the light side while following the system', async () => {
    dark = false;
    const theme = await import('./theme');
    const { findTheme } = await import('./themes');
    theme.applyThemePrefs(
      prefs({ follow_system_appearance: true, light_theme: 'vellum', dark_theme: 'one-dark' }),
    );
    expect(rootAttrs['data-theme']).toBe('rubric');
    expect(await cached()).toEqual({
      v: 1,
      follow: true,
      light: theme.themePaintSide(findTheme('rubric')),
      dark: theme.themePaintSide(findTheme('one-half-dark')),
      more: {
        light: theme.themePaintSide(findTheme('high-contrast-light')),
        dark: theme.themePaintSide(findTheme('high-contrast')),
      },
    });
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
    theme.applyTheme('rubric');
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
    theme.applyThemePrefs(prefs({ theme: 'rubric' }));
    expect(backdrops()).toEqual([
      { background: themeTokens(findTheme('rubric')).bg, appearance: 'light' },
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
      { background: themeTokens(findTheme('rubric')).bg, appearance: null },
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
    first.applyThemePrefs(prefs({ theme: 'rubric' }));

    // The next window starts from that cache. The terminal reads the
    // id when it mounts, before the config arrives.
    vi.resetModules();
    const { prepaintTheme } = await import('./themePaint');
    prepaintTheme();
    const theme = await import('./theme');
    expect(theme.getCurrentThemeId()).toBe('rubric');
  });

  it('says whether the startup paint already shows the active theme', async () => {
    const first = await import('./theme');
    first.applyThemePrefs(prefs({ theme: 'rubric' }));

    // The next window starts from that cache.
    vi.resetModules();
    const { prepaintTheme } = await import('./themePaint');
    const theme = await import('./theme');
    expect(prepaintTheme()?.id).toBe('rubric');
    expect(theme.paintMatchesBoot()).toBe(false);
    theme.applyThemePrefs(prefs({ theme: 'rubric' }));
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
    theme.applyThemePrefs(prefs({ theme: 'rubric' }));
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
    theme.applyThemePrefs(prefs({ theme: 'rubric' }));
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
    theme.applyThemePrefs(prefs({ theme: 'rubric' }));
    invoke.mockClear();
    theme.applyThemePrefs(prefs({ theme: 'gone' }));
    expect(rootAttrs['data-theme']).toBe('obsidian-ember');
    expect(await manualId()).toBe('rubric');
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
    expect(paint?.follow === true && paint.light.id).toBe('rubric');
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

  // A custom theme's Background field takes any color CSS can draw, not
  // just hex. The backend still has to hear the appearance, or the next
  // Settings window opens on the old one, and the ground whenever it is
  // a solid color.
  type Pixel = [number, number, number, number];
  // A stand in for a 2D canvas. It reads the colors in `known` and
  // ignores anything else, as a canvas ignores a color it cannot read.
  const canvasReading = (known: Record<string, Pixel>) => {
    const colors: Record<string, Pixel> = { 'rgba(0, 0, 0, 0)': [0, 0, 0, 0], ...known };
    const root = (document as unknown as { documentElement: unknown }).documentElement;
    vi.stubGlobal('document', {
      documentElement: root,
      createElement: () => {
        let fill: Pixel = [0, 0, 0, 255];
        let pixel: Pixel = [0, 0, 0, 0];
        return {
          width: 300,
          height: 150,
          getContext: () => ({
            set fillStyle(css: string) {
              if (colors[css]) fill = colors[css];
            },
            fillRect: () => {
              pixel = fill;
            },
            getImageData: () => ({ data: Uint8ClampedArray.from(pixel) }),
          }),
        };
      },
    });
  };

  const groundedOn = (bg: string) => ({
    id: 'grounded',
    label: 'Grounded',
    description: '',
    xterm: { background: '#101010', foreground: '#dddddd' },
    chrome: { bg },
  });

  const reportFor = async (bg: string) => {
    const theme = await import('./theme');
    const { customToAppTheme, setCustomThemes } = await import('./themes');
    theme.applyThemePrefs(prefs({ theme: 'rubric' }));
    setCustomThemes([customToAppTheme(groundedOn(bg))]);
    invoke.mockClear();
    theme.applyThemePrefs(prefs({ theme: 'grounded' }));
    return backdrops();
  };

  it('reports a solid ground written in any CSS color', async () => {
    canvasReading({ 'rgb(16, 16, 16)': [16, 16, 16, 255], black: [0, 0, 0, 255] });
    expect(await reportFor('rgb(16, 16, 16)')).toEqual([
      { background: '#101010', appearance: 'dark' },
    ]);
    vi.resetModules();
    expect(await reportFor('black')).toEqual([{ background: '#000000', appearance: 'dark' }]);
  });

  it('reports the appearance without a ground it cannot use', async () => {
    // Translucent: the window color under it would not match the page.
    canvasReading({ '#10101080': [16, 16, 16, 128] });
    expect(await reportFor('#10101080')).toEqual([{ background: null, appearance: 'dark' }]);
    // A color the canvas cannot read.
    vi.resetModules();
    expect(await reportFor('var(--nowhere)')).toEqual([{ background: null, appearance: 'dark' }]);
  });

  it('reports the appearance where no canvas can read the ground', async () => {
    expect(await reportFor('rgb(16, 16, 16)')).toEqual([{ background: null, appearance: 'dark' }]);
  });
});

describe('following the game', () => {
  let stored: Record<string, string> = {};
  let rootAttrs: Record<string, string> = {};
  /** What daylight_get answers, by the session it names. */
  let daylight: Record<string, string | null> = {};

  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

  const turn = (phase: string, session: number) => {
    for (const cb of handlers.get('vosh://daylight-changed') ?? [])
      cb({ payload: { phase, session } });
  };

  const GAME = prefs({
    theme: 'nord',
    theme_follow: 'game',
    day_theme: 'gruvbox',
    night_theme: 'obsidian-ember',
  });

  beforeEach(() => {
    vi.resetModules();
    handlers.clear();
    setTheme.mockClear();
    emit.mockClear();
    stored = {};
    rootAttrs = {};
    daylight = { selected: 'night', '1': 'night', '2': 'day' };
    invoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === 'daylight_get') {
        const session = (args as { session?: number } | undefined)?.session;
        return Promise.resolve(daylight[session === undefined ? 'selected' : String(session)]);
      }
      if (cmd === 'sessions_list') {
        return Promise.resolve([
          { id: 1, name: null, character: 'Tolliver', selected: true },
          { id: 2, name: null, character: 'Maren', selected: false },
        ]);
      }
      return Promise.resolve();
    });
    vi.stubGlobal('window', {
      matchMedia: () => ({
        matches: false,
        addEventListener: () => {},
        removeEventListener: () => {},
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
    invoke.mockReset();
    invoke.mockImplementation(() => Promise.resolve());
    vi.unstubAllGlobals();
  });

  const cached = async () => {
    const { pageStorage, readThemePaint } = await import('./themePaint');
    return readThemePaint(pageStorage());
  };

  it('shows the night theme the game says, then the day theme at dawn', async () => {
    const theme = await import('./theme');
    theme.applyThemePrefs(GAME, { broadcastFlips: true });
    await settle();
    expect(theme.getCurrentThemeId()).toBe('obsidian-ember');
    // The window takes the theme's own appearance, not the system's.
    expect(setTheme).toHaveBeenLastCalledWith('dark');
    turn('day', 1);
    expect(theme.getCurrentThemeId()).toBe('gruvbox');
    expect(rootAttrs['data-theme']).toBe('gruvbox');
    // The main window tells the Terminal, which repaints its palette.
    expect(emit).toHaveBeenCalledWith('vosh://theme-changed', 'gruvbox');
    const paint = await cached();
    expect(paint?.follow === 'game' && [paint.phase, paint.day.id, paint.night.id]).toEqual([
      'day',
      'gruvbox',
      'obsidian-ember',
    ]);
  });

  it('changes nothing for a turn in a session behind', async () => {
    const theme = await import('./theme');
    theme.applyThemePrefs(GAME);
    await settle();
    turn('day', 2);
    expect(theme.getCurrentThemeId()).toBe('obsidian-ember');
  });

  it('holds the side last shown while the game has not said', async () => {
    stored['vosh.cache.themePaint'] = JSON.stringify({
      v: 1,
      follow: 'game',
      day: { id: 'gruvbox', appearance: 'dark', vars: { '--bg': '#282828' } },
      night: { id: 'obsidian-ember', appearance: 'dark', vars: { '--bg': '#0f0e0d' } },
      phase: 'day',
    });
    daylight = { selected: null, '1': null };
    const { prepaintTheme } = await import('./themePaint');
    expect(prepaintTheme()?.id).toBe('gruvbox');
    const theme = await import('./theme');
    expect(theme.applyThemePrefs(GAME)).toBe('gruvbox');
    await settle();
    expect(theme.getCurrentThemeId()).toBe('gruvbox');
    // A pick fills the side that shows.
    expect(theme.pickTheme(GAME, 'rubric')).toEqual({ ...GAME, day_theme: 'rubric' });
  });

  it('stops following when Switch themes goes off', async () => {
    const theme = await import('./theme');
    theme.applyThemePrefs(GAME);
    await settle();
    theme.applyThemePrefs(prefs());
    turn('day', 1);
    expect(theme.getCurrentThemeId()).toBe('nord');
  });
});
