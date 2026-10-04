import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type EventCallback } from '@tauri-apps/api/event';
import uiDefaults from '../../fixtures/ui-config/defaults.json';
import {
  AFFECTS_MARKERS,
  AFFECTS_STYLES,
  affectsDisplayOf,
  broadcastUiConfigChanges,
  decodeOutputPayload,
  followReplacedUiConfig,
  freeBuiltinThemeIds,
  GAME_TIMES,
  getUiConfig,
  isOwnAffectsDisplayEcho,
  isOwnThemeEcho,
  migrationAnalyze,
  migrationApply,
  subscribeMigrationApplied,
  normalizeAffectsDisplay,
  normalizeAffectsMarker,
  normalizeAffectsStyle,
  normalizeAffectsThresholds,
  normalizeChipStyle,
  normalizeGameTime,
  normalizePromptShow,
  normalizeTerminalLineHeight,
  normalizeTickCount,
  normalizeUiConfig,
  normalizeVitalsDensity,
  normalizeVitalsMeter,
  normalizeVitalsOptions,
  normalizeVitalsValues,
  onGmcpPackage,
  primeUiConfigThemePrefs,
  seedDarkTheme,
  sendInput,
  sendMaskedInput,
  setAffectsDisplay,
  stopWalk,
  setUiConfig,
  terminalLocalWrite,
  TERMINAL_LINE_HEIGHTS,
  TICK_COUNTS,
  type CustomTheme,
  type RawUiConfig,
  type UiConfig,
} from './session';
import { galleryThemes } from './themeThumb';
import { BUILTIN_THEMES, customToAppTheme, findTheme, setCustomThemes } from './themes';
import gmcpEvents from '../../fixtures/ipc/gmcp-events.json';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

const raw = (patch: Partial<RawUiConfig> = {}): RawUiConfig => ({
  theme: 'nord',
  auto_update: false,
  font_family: 'Menlo',
  font_size: 14,
  tracked_affects: [],
  enabled_presets: [],
  ...patch,
});

const custom = (id: string, background: string): CustomTheme => ({
  id,
  label: id,
  description: '',
  xterm: { background, foreground: background === '#000000' ? '#ffffff' : '#000000' },
  chrome: {},
});

describe('normalizeUiConfig appearance fields', () => {
  it('fills the defaults for a config saved before they existed', () => {
    const ui = normalizeUiConfig(raw());
    expect(ui.follow_system_appearance).toBe(false);
    expect(ui.light_theme).toBe('vellum');
    expect(ui.dark_theme).toBe('nord');
    expect(ui.terminal_line_height).toBe('default');
  });

  it('keeps the saved values', () => {
    const ui = normalizeUiConfig(
      raw({
        follow_system_appearance: true,
        light_theme: 'classic-vivid',
        dark_theme: 'tokyo-night',
        terminal_line_height: 'loose',
      }),
    );
    expect(ui.follow_system_appearance).toBe(true);
    expect(ui.light_theme).toBe('classic-vivid');
    expect(ui.dark_theme).toBe('tokyo-night');
    expect(ui.terminal_line_height).toBe('loose');
  });

  it('coerces an unknown line height to the default', () => {
    expect(normalizeUiConfig(raw({ terminal_line_height: 'roomy' })).terminal_line_height).toBe(
      'default',
    );
  });
});

describe('seedDarkTheme', () => {
  it('takes the current theme when it is dark', () => {
    expect(seedDarkTheme('nord', [])).toBe('nord');
    expect(seedDarkTheme('obsidian-ember', [])).toBe('obsidian-ember');
  });

  it('falls back to Obsidian Ember for a light or unknown theme', () => {
    expect(seedDarkTheme('vellum', [])).toBe('obsidian-ember');
    expect(seedDarkTheme('system', [])).toBe('obsidian-ember');
    expect(seedDarkTheme('gone', [])).toBe('obsidian-ember');
  });

  it('reads custom themes by their own background', () => {
    const themes = [custom('night-ink', '#000000'), custom('paper', '#ffffff')];
    expect(seedDarkTheme('night-ink', themes)).toBe('night-ink');
    expect(seedDarkTheme('paper', themes)).toBe('obsidian-ember');
  });
});

describe('a custom theme on a built-in id', () => {
  // An Alacritty solarized_light.yml imported before Solarized Light
  // shipped, picked as your theme and as the light theme.
  const imported = custom('solarized-light', '#ffffff');
  const before = () =>
    raw({
      theme: 'solarized-light',
      light_theme: 'solarized-light',
      dark_theme: 'nord',
      custom_themes: [custom('night-ink', '#000000'), imported],
      generation: 4,
    });

  afterEach(() => {
    setCustomThemes([]);
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
  });

  it('leaves a config with no collision as it is', () => {
    const cfg = raw({ custom_themes: [custom('night-ink', '#000000')] });
    expect(freeBuiltinThemeIds(cfg)).toBe(cfg);
    expect(freeBuiltinThemeIds(raw())).toEqual(raw());
  });

  it('moves the custom theme to a free id and follows it with every choice', () => {
    const ui = normalizeUiConfig(before());
    expect(ui.custom_themes.map((t) => t.id)).toEqual(['night-ink', 'solarized-light-2']);
    expect(ui.custom_themes[1].xterm).toEqual(imported.xterm);
    expect(ui.theme).toBe('solarized-light-2');
    expect(ui.light_theme).toBe('solarized-light-2');
    expect(ui.dark_theme).toBe('nord');
  });

  it('frees the Everforest and Green Screen ids too', () => {
    // An Everforest file you imported, or a theme you named Green Screen.
    const ids = ['everforest-dark', 'everforest-light', 'green-screen'];
    const out = freeBuiltinThemeIds(
      raw({
        theme: 'green-screen',
        light_theme: 'everforest-light',
        dark_theme: 'everforest-dark',
        custom_themes: ids.map((id) => custom(id, '#000000')),
      }),
    );
    expect(out.custom_themes?.map((t) => t.id)).toEqual(ids.map((id) => `${id}-2`));
    expect(out).toMatchObject({
      theme: 'green-screen-2',
      light_theme: 'everforest-light-2',
      dark_theme: 'everforest-dark-2',
    });
  });

  it('frees the id of every built-in theme', () => {
    const ids = BUILTIN_THEMES.map((t) => t.id);
    const out = freeBuiltinThemeIds(raw({ custom_themes: ids.map((id) => custom(id, '#000000')) }));
    expect(out.custom_themes?.map((t) => t.id)).toEqual(ids.map((id) => `${id}-2`));
  });

  it('seeds the dark theme from the moved custom theme', () => {
    const ui = normalizeUiConfig(
      raw({ theme: 'solarized-dark', custom_themes: [custom('solarized-dark', '#000000')] }),
    );
    expect(ui.theme).toBe('solarized-dark-2');
    expect(ui.dark_theme).toBe('solarized-dark-2');
  });

  it('keeps every id distinct and points a choice at the first holder', () => {
    const ui = normalizeUiConfig(
      raw({
        theme: 'solarized-light',
        custom_themes: [
          custom('solarized-light-2', '#000000'),
          custom('solarized-light', '#ffffff'),
          custom('solarized-light', '#eeeeee'),
        ],
      }),
    );
    expect(ui.custom_themes.map((t) => t.id)).toEqual([
      'solarized-light-2',
      'solarized-light-3',
      'solarized-light-4',
    ]);
    expect(ui.theme).toBe('solarized-light-3');
  });

  it('shows one gallery tile per id and keeps the custom colors reachable', () => {
    const ui = normalizeUiConfig(before());
    setCustomThemes(ui.custom_themes.map(customToAppTheme));
    const ids = galleryThemes(BUILTIN_THEMES, ui.custom_themes.map(customToAppTheme)).map(
      (t) => t.id,
    );
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toContain('solarized-light');
    expect(ids).toContain('solarized-light-2');
    expect(findTheme(ui.theme).xterm.background).toBe('#ffffff');
    expect(findTheme('solarized-light').xterm.background).toBe('#fdf6e3');
  });

  it('saves the move once, at the generation it read', async () => {
    let stored: Record<string, unknown> = { ...before() };
    const saves: Record<string, unknown>[] = [];
    vi.mocked(invoke).mockImplementation(((
      command: string,
      args?: { config: Record<string, unknown> },
    ) => {
      if (command === 'ui_get_config') return Promise.resolve(stored);
      if (command === 'ui_set_config' && args) {
        saves.push(args.config);
        stored = { ...args.config };
        return Promise.resolve(true);
      }
      return Promise.resolve();
    }) as typeof invoke);

    const first = await getUiConfig();
    expect(saves).toHaveLength(1);
    expect(saves[0]).toMatchObject({
      theme: 'solarized-light-2',
      light_theme: 'solarized-light-2',
      dark_theme: 'nord',
      generation: 4,
    });
    expect((saves[0].custom_themes as CustomTheme[]).map((t) => t.id)).toEqual([
      'night-ink',
      'solarized-light-2',
    ]);

    // The next read finds nothing to move, so picking the built-in
    // later keeps meaning the built-in.
    const again = await getUiConfig();
    expect(saves).toHaveLength(1);
    expect(again.theme).toBe(first.theme);
    expect(again.custom_themes).toEqual(first.custom_themes);
  });

  it('still reads the config when the save fails', async () => {
    const quiet = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(invoke).mockImplementation(((command: string) =>
      command === 'ui_get_config'
        ? Promise.resolve(before())
        : Promise.reject(new Error('disk full'))) as typeof invoke);
    const ui = await getUiConfig();
    expect(ui.theme).toBe('solarized-light-2');
    quiet.mockRestore();
  });
});

describe('terminal line heights', () => {
  it('maps each id to the xterm line height', () => {
    expect(TERMINAL_LINE_HEIGHTS).toEqual({ compact: 1.1, default: 1.2, loose: 1.35 });
  });

  it('keeps the known ids only', () => {
    expect(normalizeTerminalLineHeight('compact')).toBe('compact');
    expect(normalizeTerminalLineHeight('loose')).toBe('loose');
    expect(normalizeTerminalLineHeight('default')).toBe('default');
    expect(normalizeTerminalLineHeight(1.35)).toBe('default');
    expect(normalizeTerminalLineHeight(undefined)).toBe('default');
  });
});

describe('broadcastUiConfigChanges theme events', () => {
  it('sends the resolved theme and the four theme fields when they change', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ theme: 'nord' }));
    await broadcastUiConfigChanges(base);
    sent.mockClear();

    // Follow on, and outside a browser the OS reads as light.
    await broadcastUiConfigChanges({ ...base, follow_system_appearance: true });
    expect(sent).toHaveBeenCalledWith('vosh://theme-changed', 'vellum');
    expect(sent).toHaveBeenCalledWith('vosh://theme-prefs-changed', {
      theme: 'nord',
      follow_system_appearance: true,
      light_theme: 'vellum',
      dark_theme: 'nord',
    });
  });

  it('stays quiet when a pair entry the OS is not showing changes', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ theme: 'nord', follow_system_appearance: true }));
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, dark_theme: 'dracula' });
    const events = sent.mock.calls.map(([event]) => event);
    expect(events).toContain('vosh://theme-prefs-changed');
    expect(events).not.toContain('vosh://theme-changed');
  });
});

describe('broadcastUiConfigChanges font event', () => {
  it('sends the panel font with the terminal font when only the panel font changes', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ font_size: 14 }));
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, panel_font: 'system' });
    expect(sent).toHaveBeenCalledWith('vosh://font-changed', {
      family: base.font_family,
      size: 14,
      panel: 'system',
      panelSize: 12,
    });
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, panel_font: 'system' });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://font-changed');
  });

  it('sends the panel size with the fonts when only the panel size changes', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ font_size: 14 }));
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, panel_font_size: 0 });
    expect(sent).toHaveBeenCalledWith('vosh://font-changed', {
      family: base.font_family,
      size: 14,
      panel: '',
      panelSize: 0,
    });
  });
});

describe('own theme echoes', () => {
  it('knows the theme this window just sent', async () => {
    const base = normalizeUiConfig(raw({ theme: 'nord' }));
    await broadcastUiConfigChanges(base);
    const next = { ...base, theme: 'gruvbox' };
    await broadcastUiConfigChanges(next);
    expect(isOwnThemeEcho('gruvbox')).toBe(true);
    expect(isOwnThemeEcho({ ...next })).toBe(true);
    expect(isOwnThemeEcho('dracula')).toBe(false);
    expect(isOwnThemeEcho({ ...next, theme: 'dracula' })).toBe(false);
  });

  it('forgets an echo after a second', async () => {
    vi.useFakeTimers();
    try {
      const base = normalizeUiConfig(raw({ theme: 'nord' }));
      await broadcastUiConfigChanges(base);
      await broadcastUiConfigChanges({ ...base, theme: 'monokai' });
      expect(isOwnThemeEcho('monokai')).toBe(true);
      vi.advanceTimersByTime(1000);
      expect(isOwnThemeEcho('monokai')).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not send theme fields another window already sent', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ theme: 'nord' }));
    await broadcastUiConfigChanges(base);
    const picked = { ...base, follow_system_appearance: true, dark_theme: 'dracula' };
    primeUiConfigThemePrefs(picked);
    sent.mockClear();
    await broadcastUiConfigChanges(picked);
    const events = sent.mock.calls.map(([event]) => event);
    expect(events).not.toContain('vosh://theme-prefs-changed');
    expect(events).not.toContain('vosh://theme-changed');
  });
});

describe('vitals density', () => {
  it('reads rows for a config saved before it existed', () => {
    expect(normalizeUiConfig(raw()).vitals_density).toBe('rows');
  });

  it('keeps one line', () => {
    expect(normalizeUiConfig(raw({ vitals_density: 'line' })).vitals_density).toBe('line');
  });

  it('coerces anything else to rows', () => {
    expect(normalizeVitalsDensity('grid')).toBe('rows');
    expect(normalizeVitalsDensity(undefined)).toBe('rows');
    expect(normalizeVitalsDensity('rows')).toBe('rows');
  });

  it('tells every window when it changes', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw());
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, vitals_density: 'line' });
    expect(sent).toHaveBeenCalledWith('vosh://vitals-density-changed', 'line');
  });
});

describe('Keep highlight colors readable', () => {
  it('reads on for a config saved before it existed, and keeps it off once off', () => {
    expect(normalizeUiConfig(raw()).readable_highlights).toBe(true);
    expect(normalizeUiConfig(raw({ readable_highlights: false })).readable_highlights).toBe(false);
  });

  it('saves with the rest of the config and tells every window when it changes', async () => {
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    const off = { ...normalizeUiConfig(raw()), readable_highlights: false };
    await setUiConfig(off);
    const [command, args] = invoked.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config).toMatchObject({ readable_highlights: false });

    const sent = vi.mocked(emit);
    await broadcastUiConfigChanges(off);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...off, readable_highlights: true });
    expect(sent).toHaveBeenCalledWith('vosh://readable-highlights-changed', true);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...off, readable_highlights: true });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain(
      'vosh://readable-highlights-changed',
    );
  });
});

describe('Collapse repeated lines', () => {
  it('reads off for a config saved before it existed, and keeps it on once on', () => {
    expect(normalizeUiConfig(raw()).collapse_repeats).toBe(false);
    expect(normalizeUiConfig(raw({ collapse_repeats: true })).collapse_repeats).toBe(true);
  });

  it('saves with the rest of the config, which the session reads for the next line', async () => {
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    const on = { ...normalizeUiConfig(raw()), collapse_repeats: true };
    await setUiConfig(on);
    const [command, args] = invoked.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config).toMatchObject({ collapse_repeats: true });
  });

  it('reads In a fight on and Attack lines off for a config saved before them', () => {
    const before = normalizeUiConfig(raw({ collapse_repeats: true }));
    expect(before.collapse_fight_lines).toBe(true);
    expect(before.collapse_attack_lines).toBe(false);
    const chosen = normalizeUiConfig(
      raw({ collapse_fight_lines: false, collapse_attack_lines: true }),
    );
    expect(chosen.collapse_fight_lines).toBe(false);
    expect(chosen.collapse_attack_lines).toBe(true);
  });

  it('saves In a fight and Attack lines with the rest of the config', async () => {
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    const chosen = {
      ...normalizeUiConfig(raw()),
      collapse_repeats: true,
      collapse_fight_lines: false,
      collapse_attack_lines: true,
    };
    await setUiConfig(chosen);
    const [command, args] = invoked.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config).toMatchObject({
      collapse_fight_lines: false,
      collapse_attack_lines: true,
    });
  });
});

describe('vitals options', () => {
  it('reads the defaults for a config saved before they existed', () => {
    const ui = normalizeUiConfig(raw());
    expect(ui.vitals_values).toBe('current-max');
    expect(ui.vitals_meter).toBe('line');
    expect(ui.vitals_warn_thirds).toBe(false);
    expect(ui.vitals_hide_when_pinned).toBe(true);
  });

  it('keeps the saved values', () => {
    const ui = normalizeUiConfig(
      raw({ vitals_values: 'percent', vitals_meter: 'none', vitals_warn_thirds: true }),
    );
    expect(ui.vitals_values).toBe('percent');
    expect(ui.vitals_meter).toBe('none');
    expect(ui.vitals_warn_thirds).toBe(true);
    expect(normalizeUiConfig(raw({ vitals_hide_when_pinned: false })).vitals_hide_when_pinned).toBe(
      false,
    );
    expect(normalizeUiConfig(raw({ vitals_values: 'current' })).vitals_values).toBe('current');
    expect(normalizeUiConfig(raw({ vitals_meter: 'bar' })).vitals_meter).toBe('bar');
  });

  it('coerces anything else to the defaults', () => {
    expect(normalizeVitalsValues('both')).toBe('current-max');
    expect(normalizeVitalsValues(undefined)).toBe('current-max');
    expect(normalizeVitalsMeter('gauge')).toBe('line');
    expect(normalizeVitalsMeter(undefined)).toBe('line');
    expect(normalizeVitalsOptions(null)).toEqual({
      values: 'current-max',
      meter: 'line',
      warn_thirds: false,
      hide_when_pinned: true,
    });
    expect(
      normalizeVitalsOptions({
        values: 'percent',
        meter: 'bar',
        warn_thirds: 'yes',
        hide_when_pinned: 'no',
      }),
    ).toEqual({ values: 'percent', meter: 'bar', warn_thirds: false, hide_when_pinned: true });
    expect(normalizeVitalsOptions({ hide_when_pinned: false }).hide_when_pinned).toBe(false);
  });

  it('saves each one with the rest of the config', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await setUiConfig(
      normalizeUiConfig(
        raw({
          vitals_values: 'current',
          vitals_meter: 'bar',
          vitals_warn_thirds: true,
          vitals_hide_when_pinned: false,
        }),
      ),
    );
    const [command, args] = sent.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config).toMatchObject({
      vitals_values: 'current',
      vitals_meter: 'bar',
      vitals_warn_thirds: true,
      vitals_hide_when_pinned: false,
    });
  });

  it('tells every window when one changes, and only then', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw());
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, vitals_meter: 'none' });
    expect(sent).toHaveBeenCalledWith('vosh://vitals-options-changed', {
      values: 'current-max',
      meter: 'none',
      warn_thirds: false,
      hide_when_pinned: true,
    });
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, vitals_meter: 'none' });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://vitals-options-changed');
    await broadcastUiConfigChanges({ ...base, vitals_meter: 'none', vitals_warn_thirds: true });
    expect(sent).toHaveBeenCalledWith('vosh://vitals-options-changed', {
      values: 'current-max',
      meter: 'none',
      warn_thirds: true,
      hide_when_pinned: true,
    });
    sent.mockClear();
    await broadcastUiConfigChanges({
      ...base,
      vitals_meter: 'none',
      vitals_warn_thirds: true,
      vitals_hide_when_pinned: false,
    });
    expect(sent).toHaveBeenCalledWith('vosh://vitals-options-changed', {
      values: 'current-max',
      meter: 'none',
      warn_thirds: true,
      hide_when_pinned: false,
    });
  });
});

describe('tick count', () => {
  it('reads unknown stored counts as counting up', () => {
    expect(TICK_COUNTS).toEqual(['up', 'down', 'down_past_zero']);
    for (const count of TICK_COUNTS) expect(normalizeTickCount(count)).toBe(count);
    expect(normalizeTickCount('sideways')).toBe('up');
    expect(normalizeTickCount(undefined)).toBe('up');
    expect(normalizeTickCount(3)).toBe('up');
    expect(normalizeUiConfig(raw()).tick_count).toBe('up');
    expect(normalizeUiConfig(raw({ tick_count: 'bogus' })).tick_count).toBe('up');
    expect(normalizeUiConfig(raw({ tick_count: 'down_past_zero' })).tick_count).toBe(
      'down_past_zero',
    );
  });

  it('saves with the rest of the config', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await setUiConfig(normalizeUiConfig(raw({ tick_count: 'down' })));
    const [command, args] = sent.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config.tick_count).toBe('down');
  });

  it('tells every window when a save changes it', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ tick_count: 'up' }));
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, tick_count: 'down' });
    expect(sent).toHaveBeenCalledWith('vosh://tick-count-changed', 'down');
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, tick_count: 'down' });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://tick-count-changed');
  });
});

describe('game time', () => {
  it('reads unknown stored clocks as the 24 hour clock', () => {
    expect(GAME_TIMES).toEqual(['24h', '12h']);
    for (const clock of GAME_TIMES) expect(normalizeGameTime(clock)).toBe(clock);
    expect(normalizeGameTime('noon')).toBe('24h');
    expect(normalizeGameTime(undefined)).toBe('24h');
    expect(normalizeGameTime(12)).toBe('24h');
    expect(normalizeUiConfig(raw()).game_time).toBe('24h');
    expect(normalizeUiConfig(raw({ game_time: '12h' })).game_time).toBe('12h');
  });

  it('saves with the rest of the config', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await setUiConfig(normalizeUiConfig(raw({ game_time: '12h' })));
    const [command, args] = sent.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config.game_time).toBe('12h');
  });

  it('tells every window when a save changes it', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw());
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, game_time: '12h' });
    expect(sent).toHaveBeenCalledWith('vosh://game-time-changed', '12h');
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, game_time: '12h' });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://game-time-changed');
  });
});

describe('affects display', () => {
  it('reads the defaults for a config saved before it existed', () => {
    const ui = normalizeUiConfig(raw());
    expect(ui.affects_style).toBe('timers');
    expect(ui.affects_marker).toBe('dot');
    expect(ui.affects_tint).toBe(false);
    expect(ui.affects_running_out_hours).toBe(2);
    expect(ui.affects_almost_gone_hours).toBe(1);
    expect(affectsDisplayOf(ui)).toEqual({
      style: 'timers',
      marker: 'dot',
      tint: false,
      running_out: 2,
      almost_gone: 1,
    });
  });

  it('keeps every saved choice and coerces anything else to the default', () => {
    expect(AFFECTS_STYLES).toEqual(['timers', 'countdown', 'chips', 'chips_drain']);
    expect(AFFECTS_MARKERS).toEqual(['dot', 'square', 'plus_minus', 'none']);
    for (const style of AFFECTS_STYLES) expect(normalizeAffectsStyle(style)).toBe(style);
    for (const marker of AFFECTS_MARKERS) expect(normalizeAffectsMarker(marker)).toBe(marker);
    expect(normalizeAffectsStyle('grid')).toBe('timers');
    expect(normalizeAffectsStyle(undefined)).toBe('timers');
    expect(normalizeAffectsMarker('check')).toBe('dot');
    expect(normalizeAffectsMarker(3)).toBe('dot');
    const ui = normalizeUiConfig(
      raw({ affects_style: 'chips', affects_marker: 'plus_minus', affects_tint: true }),
    );
    expect(affectsDisplayOf(ui)).toEqual({
      style: 'chips',
      marker: 'plus_minus',
      tint: true,
      running_out: 2,
      almost_gone: 1,
    });
    expect(normalizeUiConfig(raw({ affects_style: 'bogus' })).affects_style).toBe('timers');
    expect(normalizeAffectsDisplay(null)).toEqual({
      style: 'timers',
      marker: 'dot',
      tint: false,
      running_out: 2,
      almost_gone: 1,
    });
    expect(normalizeAffectsDisplay({ style: 'countdown', marker: 'none', tint: 'yes' })).toEqual({
      style: 'countdown',
      marker: 'none',
      tint: false,
      running_out: 2,
      almost_gone: 1,
    });
  });

  it('reads the hours at which an affect runs out and is almost gone as the backend saves them', () => {
    // Whole hours from 0 to 99, almost gone never over running out, and
    // the defaults for anything that is no number.
    expect(normalizeAffectsThresholds(undefined, undefined)).toEqual({
      running_out: 2,
      almost_gone: 1,
    });
    expect(normalizeAffectsThresholds(5, 2)).toEqual({ running_out: 5, almost_gone: 2 });
    expect(normalizeAffectsThresholds(0, 0)).toEqual({ running_out: 0, almost_gone: 0 });
    expect(normalizeAffectsThresholds(3, 3)).toEqual({ running_out: 3, almost_gone: 3 });
    expect(normalizeAffectsThresholds(3, 7)).toEqual({ running_out: 3, almost_gone: 3 });
    expect(normalizeAffectsThresholds(400, -2)).toEqual({ running_out: 99, almost_gone: 0 });
    expect(normalizeAffectsThresholds(4.6, 1.2)).toEqual({ running_out: 5, almost_gone: 1 });
    expect(normalizeAffectsThresholds('6', NaN)).toEqual({ running_out: 2, almost_gone: 1 });
    expect(normalizeAffectsThresholds(0, undefined)).toEqual({ running_out: 0, almost_gone: 0 });
    const ui = normalizeUiConfig(
      raw({ affects_running_out_hours: 6, affects_almost_gone_hours: 9 }),
    );
    expect(ui.affects_running_out_hours).toBe(6);
    expect(ui.affects_almost_gone_hours).toBe(6);
    expect(normalizeAffectsDisplay({ running_out: 5, almost_gone: 2 })).toMatchObject({
      running_out: 5,
      almost_gone: 2,
    });
  });

  it('saves every field with the rest of the config', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await setUiConfig(
      normalizeUiConfig(
        raw({
          affects_style: 'countdown',
          affects_marker: 'square',
          affects_tint: true,
          affects_running_out_hours: 5,
          affects_almost_gone_hours: 2,
        }),
      ),
    );
    const [command, args] = sent.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config).toMatchObject({
      affects_style: 'countdown',
      affects_marker: 'square',
      affects_tint: true,
      affects_running_out_hours: 5,
      affects_almost_gone_hours: 2,
    });
  });

  it('saves a pick from the pane menu alone, never the whole config', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await setAffectsDisplay({ marker: 'none' });
    expect(sent).toHaveBeenCalledWith('ui_set_affects_display', {
      style: null,
      marker: 'none',
      tint: null,
      runningOut: null,
      almostGone: null,
    });
    expect(sent.mock.calls.map(([command]) => command)).not.toContain('ui_set_config');
  });

  it('tells every window when a save changes it, and knows its own echo', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw());
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, affects_style: 'chips' });
    const display = {
      style: 'chips',
      marker: 'dot',
      tint: false,
      running_out: 2,
      almost_gone: 1,
    } as const;
    expect(sent).toHaveBeenCalledWith('vosh://affects-display-changed', display);
    expect(isOwnAffectsDisplayEcho(display)).toBe(true);
    expect(isOwnAffectsDisplayEcho({ ...display, style: 'countdown' })).toBe(false);
    expect(isOwnAffectsDisplayEcho({ ...display, running_out: 3 })).toBe(false);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, affects_style: 'chips' });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://affects-display-changed');
    await broadcastUiConfigChanges({ ...base, affects_style: 'chips', affects_tint: true });
    expect(sent).toHaveBeenCalledWith('vosh://affects-display-changed', {
      style: 'chips',
      marker: 'dot',
      tint: true,
      running_out: 2,
      almost_gone: 1,
    });
    // A new threshold tells every window too.
    sent.mockClear();
    await broadcastUiConfigChanges({
      ...base,
      affects_style: 'chips',
      affects_tint: true,
      affects_running_out_hours: 4,
    });
    expect(sent).toHaveBeenCalledWith('vosh://affects-display-changed', {
      style: 'chips',
      marker: 'dot',
      tint: true,
      running_out: 4,
      almost_gone: 1,
    });
  });
});

describe('chip style', () => {
  it('reads unknown stored styles as the value alone', () => {
    expect(normalizeChipStyle('caption_value')).toBe('caption_value');
    expect(normalizeChipStyle('icon_value')).toBe('icon_value');
    expect(normalizeChipStyle('value_only')).toBe('value_only');
    expect(normalizeChipStyle('emoji')).toBe('value_only');
    expect(normalizeChipStyle(undefined)).toBe('value_only');
    expect(normalizeUiConfig(raw({ chip_style: 'bogus' })).chip_style).toBe('value_only');
    expect(normalizeUiConfig(raw({ chip_style: 'icon_value' })).chip_style).toBe('icon_value');
  });

  it('tells every window when a save changes it', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ chip_style: 'value_only' }));
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, chip_style: 'icon_value' });
    expect(sent).toHaveBeenCalledWith('vosh://chip-style-changed', 'icon_value');
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, chip_style: 'icon_value' });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://chip-style-changed');
  });
});

describe('a replaced UI config', () => {
  const REPLACED = 'vosh://ui-config-replaced';

  afterEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
  });

  /** Follow the replace notice the way the Settings window does, or
   *  the main window with `broadcast`, and hand back a way to send it. */
  async function follow(
    apply: (config: UiConfig) => void,
    options: { broadcast?: boolean } = {},
  ): Promise<() => void> {
    let heard: EventCallback<unknown> | undefined;
    vi.mocked(listen).mockImplementationOnce((event, handler) => {
      if (event === REPLACED) heard = handler as EventCallback<unknown>;
      return Promise.resolve(() => {});
    });
    await followReplacedUiConfig(apply, () => {}, options);
    return () => heard?.({ event: REPLACED, id: 0, payload: null });
  }

  /** Answer every ui_get_config with `config`. */
  function answer(config: RawUiConfig): void {
    vi.mocked(invoke).mockImplementation(((command: string) =>
      Promise.resolve(command === 'ui_get_config' ? config : undefined)) as typeof invoke);
  }

  /** Answer the next ui_get_config when the test says so. */
  function answerLater(): (config: RawUiConfig) => void {
    let resolve: (config: RawUiConfig) => void = () => {};
    vi.mocked(invoke).mockImplementationOnce(
      (() =>
        new Promise<RawUiConfig>((done) => {
          resolve = done;
        })) as typeof invoke,
    );
    return (config) => resolve(config);
  }

  const settle = () => new Promise((done) => setTimeout(done, 0));

  it('keeps the loaded values when Settings saves after a #profile load', async () => {
    // Settings opened on a profile that counts down with the icon, and
    // its last save sent those.
    const opened = normalizeUiConfig(
      raw({ tick_count: 'down', chip_style: 'icon_value', vitals_density: 'line' }),
    );
    await setUiConfig(opened);
    let config = opened;
    const replace = await follow((next) => {
      config = next;
    });

    // #profile load brings a profile that counts up with the value alone.
    const loaded = raw({ tick_count: 'up', chip_style: 'value_only', vitals_density: 'rows' });
    answer(loaded);
    replace();
    await vi.waitFor(() => expect(config.tick_count).toBe('up'));
    expect(config).toEqual(normalizeUiConfig(loaded));

    // You change the font size.
    const saved = vi.mocked(invoke);
    const sent = vi.mocked(emit);
    saved.mockClear();
    sent.mockClear();
    await setUiConfig({ ...config, font_size: 16 });
    const [command, args] = saved.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config).toMatchObject({
      font_size: 16,
      tick_count: 'up',
      chip_style: 'value_only',
      vitals_density: 'rows',
    });
    // The main window sends every loaded value to every window after
    // the replace, so the save sends only what you changed.
    const events = sent.mock.calls.map(([event]) => event);
    expect(events).toContain('vosh://font-changed');
    expect(events).not.toContain('vosh://tick-count-changed');
    expect(events).not.toContain('vosh://chip-style-changed');
    expect(events).not.toContain('vosh://vitals-density-changed');
  });

  it('has the main window send every loaded field, even one it sent before', async () => {
    // The main window last sent these values at a profile switch.
    // Settings then saved others, and its saves never move the main
    // window's last broadcast, so a diff there would skip them.
    const loaded = raw({
      echo_macros: false,
      input_echo_caret: false,
      paste_line_delay_ms: 200,
      spellcheck_prompt: true,
      input_cursor_style: 'underline',
      input_echo_color: '#ff8800',
      vitals_density: 'line',
    });
    await broadcastUiConfigChanges(normalizeUiConfig(loaded));
    const sent = vi.mocked(emit);
    let sentBeforeApply = -1;
    const applied: UiConfig[] = [];
    const replace = await follow(
      (next) => {
        sentBeforeApply = sent.mock.calls.length;
        applied.push(next);
      },
      { broadcast: true },
    );

    // #profile load brings that profile back.
    sent.mockClear();
    answer(loaded);
    replace();
    await vi.waitFor(() =>
      expect(sent.mock.calls.map(([event]) => event)).toContain('vosh://tick-count-changed'),
    );
    expect(applied).toHaveLength(1);
    // The window takes the config itself before it tells the others.
    expect(sentBeforeApply).toBe(0);
    const payloads = new Map(sent.mock.calls.map(([event, payload]) => [event, payload]));
    expect(payloads.get('vosh://echo-macros-changed')).toBe(false);
    expect(payloads.get('vosh://input-echo-caret-changed')).toBe(false);
    expect(payloads.get('vosh://paste-line-delay-changed')).toBe(200);
    expect(payloads.get('vosh://spellcheck-prompt-changed')).toBe(true);
    expect(payloads.get('vosh://input-cursor-style-changed')).toBe('underline');
    expect(payloads.get('vosh://input-echo-color-changed')).toBe('#ff8800');
    expect(payloads.get('vosh://vitals-density-changed')).toBe('line');
    // Your prompt travels through the prompt commands, not the config.
    expect(payloads.has('vosh://prompt-template-changed')).toBe(false);
  });

  /** A backend that hands out its generation with the config and turns
   *  away a save read at another one, as ui_set_config does. */
  function fakeBackend(initial: RawUiConfig) {
    let generation = 1;
    let stored: Record<string, unknown> = { ...initial };
    vi.mocked(invoke).mockImplementation(((
      command: string,
      args?: { config: Record<string, unknown> },
    ) => {
      if (command === 'ui_get_config') return Promise.resolve({ ...stored, generation });
      if (command === 'ui_set_config' && args) {
        const next = args.config;
        if (next.generation != null && next.generation !== generation) {
          return Promise.resolve(false);
        }
        stored = { ...next };
        return Promise.resolve(true);
      }
      return Promise.resolve();
    }) as typeof invoke);
    return {
      /** #profile load, #profile reset, an import, or a switch. */
      replace(next: RawUiConfig) {
        stored = { ...next };
        generation += 1;
      },
      stored: () => stored,
    };
  }

  it('turns away a save built on the old profile after a #profile load', async () => {
    const backend = fakeBackend(raw({ tick_count: 'down', chip_style: 'icon_value' }));
    const opened = await getUiConfig();
    expect(opened.generation).toBe(1);
    let config = opened;
    const replace = await follow((next) => {
      config = next;
    });

    // #profile load lands while a save built on the old copy waits, or
    // while a Settings page holds one, or right after you typed in the
    // gap before the new copy arrived.
    backend.replace(raw({ tick_count: 'up', chip_style: 'value_only' }));
    const sent = vi.mocked(emit);
    sent.mockClear();
    expect(await setUiConfig({ ...opened, font_size: 16 })).toBe(false);
    expect(backend.stored()).toMatchObject({ tick_count: 'up', chip_style: 'value_only' });
    expect(backend.stored().font_size).toBe(14);
    expect(sent).not.toHaveBeenCalled();

    // The new copy arrives, and an edit made on it saves.
    replace();
    await vi.waitFor(() => expect(config.generation).toBe(2));
    expect(config.tick_count).toBe('up');
    expect(await setUiConfig({ ...config, font_size: 16 })).toBe(true);
    expect(backend.stored()).toMatchObject({
      font_size: 16,
      tick_count: 'up',
      chip_style: 'value_only',
    });
  });

  it('reads the config again when a save is turned away, notice or not', async () => {
    const backend = fakeBackend(raw({ tick_count: 'down' }));
    const opened = await getUiConfig();
    const applied: UiConfig[] = [];
    await follow((next) => applied.push(next));
    backend.replace(raw({ tick_count: 'up' }));
    expect(await setUiConfig({ ...opened, font_size: 16 })).toBe(false);
    await vi.waitFor(() => expect(applied).toHaveLength(1));
    expect(applied[0].tick_count).toBe('up');
    expect(applied[0].generation).toBe(2);
  });

  it('applies only the newest read when two replaces come close together', async () => {
    const applied: UiConfig[] = [];
    const replace = await follow((next) => applied.push(next));
    const answerReset = answerLater();
    const answerLoad = answerLater();
    replace();
    replace();
    answerLoad(raw({ tick_count: 'down' }));
    await vi.waitFor(() => expect(applied).toHaveLength(1));
    answerReset(raw({ tick_count: 'up' }));
    await settle();
    expect(applied.map((c) => c.tick_count)).toEqual(['down']);
  });

  it('reads the config again rather than share a read from before the replace', async () => {
    const applied: UiConfig[] = [];
    const replace = await follow((next) => applied.push(next));
    const answerEarly = answerLater();
    const early = getUiConfig();
    answer(raw({ tick_count: 'down' }));
    replace();
    await vi.waitFor(() => expect(applied).toHaveLength(1));
    expect(applied[0].tick_count).toBe('down');
    answerEarly(raw({ tick_count: 'up' }));
    await early;
    await settle();
    expect(applied.map((c) => c.tick_count)).toEqual(['down']);
  });
});

describe('sending a line', () => {
  // Made up value only. It is nobody's password.
  const SECRET = 'Tr0ub4dor&3';

  afterEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it('sends a line from the masked field through the masked send only', async () => {
    vi.mocked(invoke).mockClear();
    await sendMaskedInput(SECRET);
    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('session_send_masked', { line: SECRET });
  });

  it('runs a typed command through the input pipeline', async () => {
    vi.mocked(invoke).mockClear();
    await sendInput('look');
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('session_send_input', { line: 'look' });
  });

  it('stops a walk on Esc with a call of its own, which sends the game nothing', async () => {
    vi.mocked(invoke).mockClear();
    await stopWalk();
    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('session_walk_stop');
  });
});

describe('the shared catalog wizard calls', () => {
  // The backend knows the preset library only from these calls. A preset
  // the library no longer has stays off for characters whose file lacks
  // it.
  it('send the preset library with the preview and the apply', async () => {
    vi.mocked(invoke).mockClear();
    await migrationAnalyze(['healing_basics', 'herb_labels']);
    expect(invoke).toHaveBeenCalledWith('migration_analyze', {
      library: ['healing_basics', 'herb_labels'],
    });
    await migrationApply([], ['healing_basics']);
    expect(invoke).toHaveBeenCalledWith('migration_apply', {
      resolutions: [],
      library: ['healing_basics'],
    });
  });

  it('hear when the move to loadouts is done', async () => {
    const heard = vi.fn();
    vi.mocked(listen).mockClear();
    await subscribeMigrationApplied(heard);
    const [event, handler] = vi.mocked(listen).mock.calls[0];
    expect(event).toBe('vosh://migration-applied');
    (handler as EventCallback<unknown>)({ event, id: 1, payload: null });
    expect(heard).toHaveBeenCalledTimes(1);
  });
});

describe('a session output payload', () => {
  const b64 = (text: string) => btoa(text);
  const text = (bytes: Uint8Array | undefined) =>
    bytes === undefined ? undefined : new TextDecoder().decode(bytes);

  it('decodes the bytes alone when nothing is replaced', () => {
    const out = decodeOutputPayload({ b64: b64('You are hungry.\r\n') });
    expect(text(out.bytes)).toBe('You are hungry.\r\n');
    expect(out.replace).toBeUndefined();
    expect(out.restore).toBeUndefined();
  });

  it('decodes a replace and a restore beside the bytes', () => {
    const out = decodeOutputPayload({
      b64: '',
      replace: { gen: 7, b64: b64('\x1b]7717;o;8\x07NEW> '), fresh: true },
      restore: b64('LIVE> '),
    });
    expect(out.bytes).toHaveLength(0);
    expect(out.replace?.gen).toBe(7);
    expect(out.replace?.fresh).toBe(true);
    expect(text(out.replace?.bytes)).toBe('\x1b]7717;o;8\x07NEW> ');
    expect(text(out.restore)).toBe('LIVE> ');
  });

  it('keeps where each piece of your design landed on the band', () => {
    const span = {
      piece: 1,
      row: 1,
      col: 1,
      width: 3,
      fg: { kind: 'default' as const },
      bg: { kind: 'default' as const },
      bold: false,
      italic: false,
      underline: false,
    };
    const out = decodeOutputPayload({ b64: '', pin: b64('tank\r\n<765>'), pin_spans: [span] });
    expect(text(out.pin)).toBe('tank\r\n<765>');
    expect(out.pinSpans).toEqual([span]);
    expect(decodeOutputPayload({ b64: '', pin: b64('[765hp]') }).pinSpans).toBeUndefined();
  });

  it('keeps which output of the prompt stage it is', () => {
    expect(decodeOutputPayload({ b64: b64('<1020> '), id: 42 }).id).toBe(42);
    expect(decodeOutputPayload({ b64: b64('[not connected]\r\n') }).id).toBeUndefined();
  });
});

describe('text the page writes to the terminal itself', () => {
  it('tells the session which output xterm took before it', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await terminalLocalWrite('look\r\n', 42);
    expect(sent).toHaveBeenCalledWith('terminal_local_write', { text: 'look\r\n', after: 42 });
  });

  it('leaves it to the native grid while that renderer shows', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await terminalLocalWrite('look\r\n', null);
    expect(sent).toHaveBeenCalledWith('terminal_local_write', { text: 'look\r\n', after: null });
  });
});

describe('where your prompt shows', () => {
  it('reads the three places and the text for anything else', () => {
    expect(normalizePromptShow('text')).toBe('text');
    expect(normalizePromptShow('lifted')).toBe('lifted');
    expect(normalizePromptShow('pinned')).toBe('pinned');
    expect(normalizePromptShow('floating')).toBe('text');
    expect(normalizePromptShow(undefined)).toBe('text');
    expect(normalizePromptShow(3)).toBe('text');
  });

  it('leaves your prompt out of the Settings config', () => {
    // An older backend still sends the three fields. The config drops
    // them, so no save sends them back.
    const config = normalizeUiConfig({
      ...raw({}),
      prompt_template_enabled: true,
      prompt_template: '%hp',
      prompt_show: 'pinned',
    } as RawUiConfig);
    for (const key of ['prompt_template_enabled', 'prompt_template', 'prompt_show']) {
      expect(key in config).toBe(false);
    }
  });
});

// The session sends each GMCP package on the event this shared file
// names for it. The fake MUD test in src-tauri reads the same file, so a
// change to the encoding on one side alone fails one of the two.
describe('onGmcpPackage', () => {
  it('listens on the event the session sends each package on', async () => {
    for (const { package: name, event } of gmcpEvents.cases) {
      vi.mocked(listen).mockClear();
      await onGmcpPackage(name, () => {});
      expect(vi.mocked(listen)).toHaveBeenCalledWith(event, expect.any(Function));
    }
  });
});

// What Rust sends for a profile that sets nothing under [ui]
// (UiConfigPayload in src-tauri/src/ipc/ui_config.rs, whose test reads
// the same file).
describe('the UI config defaults Rust sends', () => {
  const defaults: Record<string, unknown> = uiDefaults.defaults;
  const passedThrough: readonly string[] = uiDefaults.passed_through;

  /** `ui` as plain fields, so each one reads by name. */
  const fields = (ui: UiConfig): Record<string, unknown> => ({ ...ui });

  /** Each default `got` holds as Rust sends it. An empty dark theme
   *  reads as the theme. */
  function expectDefaults(got: Record<string, unknown>) {
    for (const [key, value] of Object.entries(defaults)) {
      if (key === 'dark_theme') expect(got[key], key).toBe(defaults.theme);
      else expect(got[key], key).toEqual(value);
    }
  }

  it('keeps every default Rust sends', () => {
    const sent = { ...defaults, auto_update: false, font_family: 'Menlo', font_size: 14 };
    expectDefaults(fields(normalizeUiConfig(sent as RawUiConfig)));
  });

  it('fills each field Rust leaves out with the default Rust sends', () => {
    const got = fields(normalizeUiConfig({} as RawUiConfig));
    expectDefaults(got);
    // The page has no value of its own for these, so Rust always sends
    // them.
    for (const key of passedThrough) expect(got[key], key).toBeUndefined();
  });
});
