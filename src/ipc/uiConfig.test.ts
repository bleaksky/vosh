import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import uiDefaults from '../../fixtures/ui-config/defaults.json';
import uiFields from '../../fixtures/ui-config/fields.json';
import type { CustomTheme } from './theme';
import {
  DEFAULT_VITALS_CUSTOM,
  DEFAULT_VITALS_OPTIONS,
  GAME_TIMES,
  getUiConfig,
  normalizeChipStyle,
  normalizeGameTime,
  normalizeTerminalLineHeight,
  normalizeTickCount,
  normalizeUiConfig,
  normalizeVitalsDensity,
  normalizeVitalsMeter,
  normalizeVitalsOptions,
  normalizeVitalsValues,
  setUiFields,
  shownStyle,
  TERMINAL_LINE_HEIGHTS,
  TICK_COUNTS,
  VITALS_STYLES,
  vitalsOptionsOf,
  type RawUiConfig,
  type UiConfig,
  type UiFields,
} from './uiConfig';
import { broadcastUiConfigChanges } from './uiConfigBroadcast';
import { galleryThemes } from '../theme/themeThumb';
import {
  BUILTIN_THEMES,
  customToAppTheme,
  findTheme,
  freeBuiltinThemeIds,
  setCustomThemes,
} from '../theme/themes';

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

/** What setUiFields hands ui_set_fields. */
interface SetFieldsArgs {
  fields: { field: string; value: unknown }[];
  profile: string | null;
}

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

describe('a retired theme id', () => {
  it('stays as you saved it, and so does the light default that names Vellum', async () => {
    const saved = normalizeUiConfig(
      raw({ theme: 'one-dark', light_theme: 'vellum', dark_theme: 'everforest-light' }),
    );
    expect(saved).toMatchObject({
      theme: 'one-dark',
      light_theme: 'vellum',
      dark_theme: 'everforest-light',
    });
    expect(normalizeUiConfig(raw()).light_theme).toBe('vellum');
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    const { theme, light_theme, dark_theme } = saved;
    await setUiFields({ theme, light_theme, dark_theme });
    expect(invoked.mock.calls).toEqual([
      [
        'ui_set_fields',
        {
          fields: [
            { field: 'theme', value: 'one-dark' },
            { field: 'light_theme', value: 'vellum' },
            { field: 'dark_theme', value: 'everforest-light' },
          ],
          profile: null,
        },
      ],
    ]);
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
    const ids = ['everforest-dark', 'green-screen'];
    const out = freeBuiltinThemeIds(
      raw({
        theme: 'green-screen',
        dark_theme: 'everforest-dark',
        custom_themes: ids.map((id) => custom(id, '#000000')),
      }),
    );
    expect(out.custom_themes?.map((t) => t.id)).toEqual(ids.map((id) => `${id}-2`));
    expect(out).toMatchObject({
      theme: 'green-screen-2',
      dark_theme: 'everforest-dark-2',
    });
  });

  it('leaves a custom theme on a retired id where it is, since it wins over the successor', () => {
    const cfg = raw({ theme: 'vellum', custom_themes: [custom('vellum', '#ffffff')] });
    expect(freeBuiltinThemeIds(cfg)).toBe(cfg);
  });

  it('frees the ids of the schemes Vosh added after Green Screen', () => {
    // A Srcery or Modus Vivendi file you imported reads under the id the
    // built in theme now has.
    const ids = ['srcery', 'nightfly', 'melange-dark', 'melange-light', 'modus-vivendi'];
    const out = freeBuiltinThemeIds(
      raw({
        theme: 'modus-vivendi',
        light_theme: 'melange-light',
        dark_theme: 'srcery',
        custom_themes: ids.map((id) => custom(id, '#000000')),
      }),
    );
    expect(out.custom_themes?.map((t) => t.id)).toEqual(ids.map((id) => `${id}-2`));
    expect(out).toMatchObject({
      theme: 'modus-vivendi-2',
      light_theme: 'melange-light-2',
      dark_theme: 'srcery-2',
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

  it('saves the move once, through the setters of the custom themes and the theme fields', async () => {
    let stored: Record<string, unknown> = { ...before() };
    const saves: Record<string, unknown>[] = [];
    vi.mocked(invoke).mockImplementation(((command: string, args?: SetFieldsArgs) => {
      if (command === 'ui_get_config') return Promise.resolve(stored);
      if (command === 'ui_set_fields' && args) {
        expect(args.profile).toBeNull();
        const saved = Object.fromEntries(args.fields.map(({ field, value }) => [field, value]));
        saves.push(saved);
        stored = { ...stored, ...saved };
      }
      return Promise.resolve();
    }) as typeof invoke);

    const first = await getUiConfig();
    expect(saves).toHaveLength(1);
    expect(Object.keys(saves[0]).sort()).toEqual([
      'custom_themes',
      'dark_theme',
      'follow_system_appearance',
      'light_theme',
      'theme',
    ]);
    expect(saves[0]).toMatchObject({
      theme: 'solarized-light-2',
      follow_system_appearance: false,
      light_theme: 'solarized-light-2',
      dark_theme: 'nord',
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

  it('tells every window when it changes, as the style the options carry', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw());
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, vitals_density: 'line' }, base);
    expect(sent.mock.calls).toEqual([
      ['vosh://vitals-options-changed', { ...DEFAULT_VITALS_OPTIONS, style: 'line' }],
    ]);
  });
});

describe('vitals style', () => {
  it('lists the styles in the order of the gallery', () => {
    expect(VITALS_STYLES).toEqual([
      'rows',
      'line',
      'ledger',
      'gauges',
      'pips',
      'bands',
      'ladders',
      'blocks',
      'traces',
      'dials',
      'rings',
      'vials',
      'text',
    ]);
  });

  it('shows your density until you pick a style, then the style', () => {
    expect(shownStyle(normalizeUiConfig(raw()))).toBe('rows');
    expect(shownStyle(normalizeUiConfig(raw({ vitals_density: 'line' })))).toBe('line');
    expect(
      shownStyle(normalizeUiConfig(raw({ vitals_density: 'line', vitals_style: 'gauges' }))),
    ).toBe('gauges');
    // Rows and One line live in vitals_density, so a saved one reads as
    // unset and the density shows.
    expect(
      shownStyle(normalizeUiConfig(raw({ vitals_density: 'line', vitals_style: 'rows' }))),
    ).toBe('line');
  });

  it('moves the footer, the status line and the menu with one event for a pick', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw());
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, vitals_style: 'pips' }, base);
    expect(sent.mock.calls).toEqual([
      ['vosh://vitals-options-changed', { ...DEFAULT_VITALS_OPTIONS, style: 'pips' }],
    ]);
  });
});

describe('Fit game colors', () => {
  it('reads on for a config saved before it existed, and keeps it off once off', () => {
    expect(normalizeUiConfig(raw()).fit_game_colors).toBe(true);
    expect(normalizeUiConfig(raw({ fit_game_colors: false })).fit_game_colors).toBe(false);
  });

  it('saves alone through ui_set_fields and tells every window when it changes', async () => {
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    const off = { ...normalizeUiConfig(raw()), fit_game_colors: false };
    await setUiFields({ fit_game_colors: false });
    expect(invoked.mock.calls).toEqual([
      ['ui_set_fields', { fields: [{ field: 'fit_game_colors', value: false }], profile: null }],
    ]);

    const sent = vi.mocked(emit);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...off, fit_game_colors: true }, off);
    expect(sent).toHaveBeenCalledWith('vosh://fit-game-colors-changed', true);
    sent.mockClear();
    await broadcastUiConfigChanges(
      { ...off, fit_game_colors: true },
      { ...off, fit_game_colors: true },
    );
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://fit-game-colors-changed');
  });
});

describe('Color vision', () => {
  it('reads Typical for a config saved before it existed or with a vision it does not know', () => {
    expect(normalizeUiConfig(raw()).color_vision).toBe('typical');
    expect(normalizeUiConfig(raw({ color_vision: 'deutan' })).color_vision).toBe('typical');
    expect(normalizeUiConfig(raw({ color_vision: 'tritanopia' })).color_vision).toBe('tritanopia');
  });

  it('saves alone through ui_set_fields and tells every window when it changes', async () => {
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    const picked = { ...normalizeUiConfig(raw()), color_vision: 'deuteranopia' as const };
    await setUiFields({ color_vision: 'deuteranopia' });
    expect(invoked.mock.calls).toEqual([
      [
        'ui_set_fields',
        { fields: [{ field: 'color_vision', value: 'deuteranopia' }], profile: null },
      ],
    ]);

    const sent = vi.mocked(emit);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...picked, color_vision: 'protanopia' }, picked);
    expect(sent).toHaveBeenCalledWith('vosh://color-vision-changed', 'protanopia');
    sent.mockClear();
    await broadcastUiConfigChanges(
      { ...picked, color_vision: 'protanopia' },
      { ...picked, color_vision: 'protanopia' },
    );
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://color-vision-changed');
  });
});

describe('Keep highlight colors readable', () => {
  it('reads on for a config saved before it existed, and keeps it off once off', () => {
    expect(normalizeUiConfig(raw()).readable_highlights).toBe(true);
    expect(normalizeUiConfig(raw({ readable_highlights: false })).readable_highlights).toBe(false);
  });

  it('saves alone through ui_set_fields and tells every window when it changes', async () => {
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    const off = { ...normalizeUiConfig(raw()), readable_highlights: false };
    await setUiFields({ readable_highlights: false });
    expect(invoked.mock.calls).toEqual([
      [
        'ui_set_fields',
        { fields: [{ field: 'readable_highlights', value: false }], profile: null },
      ],
    ]);

    const sent = vi.mocked(emit);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...off, readable_highlights: true }, off);
    expect(sent).toHaveBeenCalledWith('vosh://readable-highlights-changed', true);
    sent.mockClear();
    await broadcastUiConfigChanges(
      { ...off, readable_highlights: true },
      { ...off, readable_highlights: true },
    );
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

  it('saves alone through ui_set_fields, and the session reads it for the next line', async () => {
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    await setUiFields({ collapse_repeats: true });
    expect(invoked.mock.calls).toEqual([
      ['ui_set_fields', { fields: [{ field: 'collapse_repeats', value: true }], profile: null }],
    ]);
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

  it('saves In a fight and Attack lines alone through ui_set_fields', async () => {
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    await setUiFields({ collapse_fight_lines: false });
    await setUiFields({ collapse_attack_lines: true });
    expect(invoked.mock.calls).toEqual([
      [
        'ui_set_fields',
        { fields: [{ field: 'collapse_fight_lines', value: false }], profile: null },
      ],
      [
        'ui_set_fields',
        { fields: [{ field: 'collapse_attack_lines', value: true }], profile: null },
      ],
    ]);
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

  it('reads junk in the vitals styles keys as something Vosh draws', () => {
    const ui = normalizeUiConfig(
      raw({
        vitals_style: 'rows',
        vitals_place: 'footer',
        vitals_order: ['move', 'move', 'tp'],
        vitals_off: ['opponent', 'tp', 'move'],
        vitals_opponent: 'middle',
        vitals_colors: { hp: 16, mana: 4, move: 'red', tp: 2 },
        vitals_text_previous: ['', 'a', 'a', 'b', 'c'],
      }),
    );
    expect(ui.vitals_style).toBeNull();
    expect(ui.vitals_place).toBe('panel');
    expect(ui.vitals_order).toEqual(['move', 'hp', 'mana']);
    expect(ui.vitals_off).toEqual(['move', 'opponent']);
    expect(ui.vitals_opponent).toBe('top');
    expect(ui.vitals_colors).toEqual({ mana: 4 });
    expect(ui.vitals_text_previous).toEqual(['a', 'b']);
    expect(normalizeUiConfig(raw({ vitals_style: 'pips' })).vitals_style).toBe('pips');
    expect(normalizeUiConfig(raw({ vitals_hit: true })).vitals_hit).toBe(true);
  });

  it('coerces anything else to the defaults', () => {
    expect(normalizeVitalsValues('both')).toBe('current-max');
    expect(normalizeVitalsValues(undefined)).toBe('current-max');
    expect(normalizeVitalsMeter('gauge')).toBe('line');
    expect(normalizeVitalsMeter(undefined)).toBe('line');
    expect(normalizeVitalsOptions(null)).toEqual({
      style: 'rows',
      place: 'panel',
      order: ['hp', 'mana', 'move'],
      off: [],
      opponent: 'top',
      colors: {},
      values: 'current-max',
      meter: 'line',
      warn_thirds: false,
      hide_when_pinned: true,
      hit: false,
    });
    expect(normalizeVitalsOptions(null)).toEqual(DEFAULT_VITALS_OPTIONS);
    expect(
      normalizeVitalsOptions({
        style: 'strip',
        place: 'footer',
        order: ['move', 'tp'],
        off: ['tp', 'opponent', 'opponent'],
        opponent: 'middle',
        colors: { hp: 'red', mana: 12 },
        values: 'percent',
        meter: 'bar',
        warn_thirds: 'yes',
        hide_when_pinned: 'no',
        hit: 'on',
      }),
    ).toEqual({
      style: 'rows',
      place: 'panel',
      order: ['move', 'hp', 'mana'],
      off: ['opponent'],
      opponent: 'top',
      colors: { mana: 12 },
      values: 'percent',
      meter: 'bar',
      warn_thirds: false,
      hide_when_pinned: true,
      hit: false,
    });
    expect(normalizeVitalsOptions({ hide_when_pinned: false }).hide_when_pinned).toBe(false);
    expect(normalizeVitalsOptions({ style: 'text', place: 'status' })).toMatchObject({
      style: 'text',
      place: 'status',
    });
  });

  it('resets Customize vitals to every vital on in todays order, and nothing above it', () => {
    const picked = normalizeUiConfig(
      raw({
        vitals_style: 'gauges',
        vitals_place: 'status',
        vitals_hide_when_pinned: false,
        vitals_order: ['move', 'hp', 'mana'],
        vitals_off: ['mana', 'opponent'],
        vitals_colors: { mana: 12 },
        vitals_opponent: 'bottom',
        vitals_values: 'percent',
        vitals_meter: 'bar',
        vitals_warn_thirds: true,
      }),
    );
    const reset = vitalsOptionsOf({ ...picked, ...DEFAULT_VITALS_CUSTOM });
    expect(reset).toEqual({
      ...DEFAULT_VITALS_OPTIONS,
      style: 'gauges',
      place: 'status',
      hide_when_pinned: false,
    });
  });

  it('saves each one alone through ui_set_fields', async () => {
    const sent = vi.mocked(invoke);
    const chosen: UiFields = {
      vitals_values: 'current',
      vitals_meter: 'bar',
      vitals_warn_thirds: true,
      vitals_hide_when_pinned: false,
    };
    for (const [field, value] of Object.entries(chosen)) {
      sent.mockClear();
      await setUiFields({ [field]: value });
      expect(sent.mock.calls).toEqual([
        ['ui_set_fields', { fields: [{ field, value }], profile: null }],
      ]);
    }
  });

  it('tells every window when one changes, and only then', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw());
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, vitals_meter: 'none' }, base);
    expect(sent).toHaveBeenCalledWith('vosh://vitals-options-changed', {
      ...DEFAULT_VITALS_OPTIONS,
      meter: 'none',
    });
    sent.mockClear();
    await broadcastUiConfigChanges(
      { ...base, vitals_meter: 'none' },
      { ...base, vitals_meter: 'none' },
    );
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://vitals-options-changed');
    await broadcastUiConfigChanges(
      { ...base, vitals_meter: 'none', vitals_warn_thirds: true },
      { ...base, vitals_meter: 'none' },
    );
    expect(sent).toHaveBeenCalledWith('vosh://vitals-options-changed', {
      ...DEFAULT_VITALS_OPTIONS,
      meter: 'none',
      warn_thirds: true,
    });
    sent.mockClear();
    await broadcastUiConfigChanges(
      {
        ...base,
        vitals_meter: 'none',
        vitals_warn_thirds: true,
        vitals_hide_when_pinned: false,
      },
      { ...base, vitals_meter: 'none', vitals_warn_thirds: true },
    );
    expect(sent).toHaveBeenCalledWith('vosh://vitals-options-changed', {
      ...DEFAULT_VITALS_OPTIONS,
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

  it('saves alone through ui_set_fields', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await setUiFields({ tick_count: 'down' });
    expect(sent.mock.calls).toEqual([
      ['ui_set_fields', { fields: [{ field: 'tick_count', value: 'down' }], profile: null }],
    ]);
  });

  it('tells every window when a save changes it', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ tick_count: 'up' }));
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, tick_count: 'down' }, base);
    expect(sent).toHaveBeenCalledWith('vosh://tick-count-changed', 'down');
    sent.mockClear();
    await broadcastUiConfigChanges(
      { ...base, tick_count: 'down' },
      { ...base, tick_count: 'down' },
    );
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

  it('saves alone through ui_set_fields', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await setUiFields({ game_time: '12h' });
    expect(sent.mock.calls).toEqual([
      ['ui_set_fields', { fields: [{ field: 'game_time', value: '12h' }], profile: null }],
    ]);
  });

  it('tells every window when a save changes it', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw());
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, game_time: '12h' }, base);
    expect(sent).toHaveBeenCalledWith('vosh://game-time-changed', '12h');
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, game_time: '12h' }, { ...base, game_time: '12h' });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://game-time-changed');
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
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, chip_style: 'icon_value' }, base);
    expect(sent).toHaveBeenCalledWith('vosh://chip-style-changed', 'icon_value');
    sent.mockClear();
    await broadcastUiConfigChanges(
      { ...base, chip_style: 'icon_value' },
      { ...base, chip_style: 'icon_value' },
    );
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://chip-style-changed');
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

// One value for each field ui_set_fields takes. The Rust setter test in
// src-tauri/src/ipc/ui_config.rs reads the same file.
describe('setUiFields', () => {
  const values: Record<string, unknown> = uiFields.fields;

  // The tracked affects have a setter of their own, and the 0.7 style
  // and text are read only, as READ_ONLY in the Rust test says.
  const readOnly = ['tracked_affects', 'vitals_legacy_style', 'vitals_legacy_text'];

  it('can send every field Rust has a setter for', () => {
    const keys = Object.keys(normalizeUiConfig({} as RawUiConfig));
    expect(keys.filter((key) => !readOnly.includes(key)).sort()).toEqual(
      Object.keys(values).sort(),
    );
  });

  it('reads the style your 0.7 vitals grew into, and nothing it does not know', () => {
    const read = (style: unknown) =>
      normalizeUiConfig({ vitals_legacy_style: style } as RawUiConfig).vitals_legacy_style;
    expect(read('text')).toBe('text');
    expect(read('line')).toBe('line');
    expect(read(undefined)).toBeNull();
    expect(read('ember')).toBeNull();
  });

  it('sends each field it names to its setter, for the profile it names', async () => {
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
    await setUiFields(values as UiFields);
    expect(invoke).toHaveBeenLastCalledWith('ui_set_fields', {
      fields: Object.entries(values).map(([field, value]) => ({ field, value })),
      profile: null,
    });
    // A field left undefined stays out, and null clears a color.
    const some: Record<string, unknown> = {
      game_time: '12h',
      split_divider_color: null,
      font_size: undefined,
    };
    await setUiFields(some as UiFields, 'Orla');
    expect(invoke).toHaveBeenLastCalledWith('ui_set_fields', {
      fields: [
        { field: 'game_time', value: '12h' },
        { field: 'split_divider_color', value: null },
      ],
      profile: 'Orla',
    });
  });
});
