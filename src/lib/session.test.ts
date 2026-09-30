import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type EventCallback } from '@tauri-apps/api/event';
import {
  AFFECTS_MARKERS,
  AFFECTS_STYLES,
  affectsDisplayOf,
  broadcastUiConfigChanges,
  changedPromptFields,
  decodeOutputPayload,
  followReplacedUiConfig,
  getUiConfig,
  isOwnAffectsDisplayEcho,
  isOwnThemeEcho,
  migrationAnalyze,
  migrationApply,
  subscribeMigrationApplied,
  normalizeAffectsDisplay,
  normalizeAffectsMarker,
  normalizeAffectsStyle,
  normalizeChipStyle,
  normalizePromptShow,
  normalizeTerminalLineHeight,
  normalizeTickCount,
  normalizeUiConfig,
  normalizeVitalsDensity,
  normalizeVitalsMeter,
  normalizeVitalsOptions,
  normalizeVitalsValues,
  primeUiConfigThemePrefs,
  seedDarkTheme,
  sendInput,
  sendMaskedInput,
  setAffectsDisplay,
  setUiConfig,
  TERMINAL_LINE_HEIGHTS,
  TICK_COUNTS,
  type CustomTheme,
  type PromptShow,
  type RawUiConfig,
  type UiConfig,
} from './session';

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

describe('vitals options', () => {
  it('reads the defaults for a config saved before they existed', () => {
    const ui = normalizeUiConfig(raw());
    expect(ui.vitals_values).toBe('current-max');
    expect(ui.vitals_meter).toBe('line');
    expect(ui.vitals_warn_thirds).toBe(false);
  });

  it('keeps the saved values', () => {
    const ui = normalizeUiConfig(
      raw({ vitals_values: 'percent', vitals_meter: 'none', vitals_warn_thirds: true }),
    );
    expect(ui.vitals_values).toBe('percent');
    expect(ui.vitals_meter).toBe('none');
    expect(ui.vitals_warn_thirds).toBe(true);
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
    });
    expect(normalizeVitalsOptions({ values: 'percent', meter: 'bar', warn_thirds: 'yes' })).toEqual(
      { values: 'percent', meter: 'bar', warn_thirds: false },
    );
  });

  it('saves all three with the rest of the config', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await setUiConfig(
      normalizeUiConfig(
        raw({ vitals_values: 'current', vitals_meter: 'bar', vitals_warn_thirds: true }),
      ),
    );
    const [command, args] = sent.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config).toMatchObject({
      vitals_values: 'current',
      vitals_meter: 'bar',
      vitals_warn_thirds: true,
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
    });
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, vitals_meter: 'none' });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://vitals-options-changed');
    await broadcastUiConfigChanges({ ...base, vitals_meter: 'none', vitals_warn_thirds: true });
    expect(sent).toHaveBeenCalledWith('vosh://vitals-options-changed', {
      values: 'current-max',
      meter: 'none',
      warn_thirds: true,
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

describe('affects display', () => {
  it('reads the defaults for a config saved before it existed', () => {
    const ui = normalizeUiConfig(raw());
    expect(ui.affects_style).toBe('timers');
    expect(ui.affects_marker).toBe('dot');
    expect(ui.affects_tint).toBe(false);
    expect(affectsDisplayOf(ui)).toEqual({ style: 'timers', marker: 'dot', tint: false });
  });

  it('keeps every saved choice and coerces anything else to the default', () => {
    expect(AFFECTS_STYLES).toEqual(['timers', 'countdown', 'chips']);
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
    expect(affectsDisplayOf(ui)).toEqual({ style: 'chips', marker: 'plus_minus', tint: true });
    expect(normalizeUiConfig(raw({ affects_style: 'bogus' })).affects_style).toBe('timers');
    expect(normalizeAffectsDisplay(null)).toEqual({ style: 'timers', marker: 'dot', tint: false });
    expect(normalizeAffectsDisplay({ style: 'countdown', marker: 'none', tint: 'yes' })).toEqual({
      style: 'countdown',
      marker: 'none',
      tint: false,
    });
  });

  it('saves all three with the rest of the config', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await setUiConfig(
      normalizeUiConfig(
        raw({ affects_style: 'countdown', affects_marker: 'square', affects_tint: true }),
      ),
    );
    const [command, args] = sent.mock.calls[0] as [string, { config: Record<string, unknown> }];
    expect(command).toBe('ui_set_config');
    expect(args.config).toMatchObject({
      affects_style: 'countdown',
      affects_marker: 'square',
      affects_tint: true,
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
    });
    expect(sent.mock.calls.map(([command]) => command)).not.toContain('ui_set_config');
  });

  it('tells every window when a save changes it, and knows its own echo', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw());
    await broadcastUiConfigChanges(base);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, affects_style: 'chips' });
    const display = { style: 'chips', marker: 'dot', tint: false };
    expect(sent).toHaveBeenCalledWith('vosh://affects-display-changed', display);
    expect(isOwnAffectsDisplayEcho({ style: 'chips', marker: 'dot', tint: false })).toBe(true);
    expect(isOwnAffectsDisplayEcho({ style: 'countdown', marker: 'dot', tint: false })).toBe(false);
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, affects_style: 'chips' });
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://affects-display-changed');
    await broadcastUiConfigChanges({ ...base, affects_style: 'chips', affects_tint: true });
    expect(sent).toHaveBeenCalledWith('vosh://affects-display-changed', {
      style: 'chips',
      marker: 'dot',
      tint: true,
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
      paste_line_delay_ms: 200,
      spellcheck_prompt: true,
      input_cursor_style: 'underline',
      input_echo_color: '#ff8800',
      vitals_density: 'line',
      moons_position: 'before-time',
      prompt_template_enabled: true,
      prompt_template: '<%h>',
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
    expect(payloads.get('vosh://paste-line-delay-changed')).toBe(200);
    expect(payloads.get('vosh://spellcheck-prompt-changed')).toBe(true);
    expect(payloads.get('vosh://input-cursor-style-changed')).toBe('underline');
    expect(payloads.get('vosh://input-echo-color-changed')).toBe('#ff8800');
    expect(payloads.get('vosh://vitals-density-changed')).toBe('line');
    expect(payloads.get('vosh://moons-position-changed')).toBe('before-time');
    expect(payloads.get('vosh://prompt-template-changed')).toEqual({
      enabled: true,
      template: '<%h>',
    });
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
    // while ThemesTab holds one, or right after you typed in the gap
    // before the new copy arrived.
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
});

describe('the prompt fields a command changed', () => {
  const fields = (on: boolean, template: string, show: PromptShow = 'text') => ({
    prompt_template_enabled: on,
    prompt_template: template,
    prompt_show: show,
  });

  it('takes where your prompt shows when #prompt show moved it', () => {
    const known = fields(true, '%hp');
    expect(changedPromptFields(known, fields(true, '%hp', 'pinned'))).toEqual({
      prompt_show: 'pinned',
    });
  });

  it('takes only what changed since the window read or saved it', () => {
    const known = fields(true, '%hp');
    expect(changedPromptFields(known, fields(true, '%hp'))).toEqual({});
    expect(changedPromptFields(known, fields(false, '%hp'))).toEqual({
      prompt_template_enabled: false,
    });
    expect(changedPromptFields(known, fields(true, '[%hp]'))).toEqual({
      prompt_template: '[%hp]',
    });
  });

  it('takes both when the window knows neither', () => {
    expect(changedPromptFields(null, fields(false, '%mana'))).toEqual(fields(false, '%mana'));
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

  it('defaults to the text in a config that does not say', () => {
    expect(normalizeUiConfig(raw({})).prompt_show).toBe('text');
    expect(normalizeUiConfig(raw({ prompt_show: 'lifted' })).prompt_show).toBe('lifted');
  });
});
