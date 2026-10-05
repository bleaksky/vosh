import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type EventCallback } from '@tauri-apps/api/event';
import { pendingWrites } from '../lib/pendingWrites';
import { queueSettingsChange } from '../settings/useSettingsAutoSave';
import { isOwnThemeEcho } from '../theme/theme';
import { getUiConfig, normalizeUiConfig, type RawUiConfig, type UiConfig } from './uiConfig';
import { broadcastUiConfigChanges, followReplacedUiConfig, setUiConfig } from './uiConfigSave';

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

describe('broadcastUiConfigChanges theme events', () => {
  it('sends the resolved theme and the four theme fields when they change', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ theme: 'nord' }));
    sent.mockClear();

    // Follow on, and outside a browser the OS reads as light.
    await broadcastUiConfigChanges({ ...base, follow_system_appearance: true }, base);
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
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, dark_theme: 'dracula' }, base);
    const events = sent.mock.calls.map(([event]) => event);
    expect(events).toContain('vosh://theme-prefs-changed');
    expect(events).not.toContain('vosh://theme-changed');
  });
});

describe('broadcastUiConfigChanges font event', () => {
  it('sends the panel font with the terminal font when only the panel font changes', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ font_size: 14 }));
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, panel_font: 'system' }, base);
    expect(sent).toHaveBeenCalledWith('vosh://font-changed', {
      family: base.font_family,
      size: 14,
      panel: 'system',
      panelSize: 12,
    });
    sent.mockClear();
    await broadcastUiConfigChanges(
      { ...base, panel_font: 'system' },
      { ...base, panel_font: 'system' },
    );
    expect(sent.mock.calls.map(([event]) => event)).not.toContain('vosh://font-changed');
  });

  it('sends the panel size with the fonts when only the panel size changes', async () => {
    const sent = vi.mocked(emit);
    const base = normalizeUiConfig(raw({ font_size: 14 }));
    sent.mockClear();
    await broadcastUiConfigChanges({ ...base, panel_font_size: 0 }, base);
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
    const next = { ...base, theme: 'gruvbox' };
    await broadcastUiConfigChanges(next, base);
    expect(isOwnThemeEcho('gruvbox')).toBe(true);
    expect(isOwnThemeEcho({ ...next })).toBe(true);
    expect(isOwnThemeEcho('dracula')).toBe(false);
    expect(isOwnThemeEcho({ ...next, theme: 'dracula' })).toBe(false);
  });

  it('forgets an echo after a second', async () => {
    vi.useFakeTimers();
    try {
      const base = normalizeUiConfig(raw({ theme: 'nord' }));
      await broadcastUiConfigChanges({ ...base, theme: 'monokai' }, base);
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
    const picked = { ...base, follow_system_appearance: true, dark_theme: 'dracula' };
    sent.mockClear();
    await broadcastUiConfigChanges(picked, picked);
    const events = sent.mock.calls.map(([event]) => event);
    expect(events).not.toContain('vosh://theme-prefs-changed');
    expect(events).not.toContain('vosh://theme-changed');
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

  it('saves only the font size when Settings changes it after a #profile load', async () => {
    // Settings opened on a profile that counts down with the icon.
    const opened = normalizeUiConfig(
      raw({ tick_count: 'down', chip_style: 'icon_value', vitals_density: 'line' }),
    );
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
    const failed = vi.fn();
    queueSettingsChange(config, { font_size: 16 }, 0, { saved: () => {}, failed });
    await pendingWrites.flushAll();
    expect(failed).not.toHaveBeenCalled();
    expect(saved.mock.calls).toEqual([
      ['ui_set_fields', { fields: [{ field: 'font_size', value: 16 }], profile: null }],
    ]);
    // The main window sends every loaded value to every window after
    // the replace, so the save tells them only what you changed.
    expect(sent.mock.calls.map(([event]) => event)).toEqual(['vosh://font-changed']);
  });

  it('has the main window send every loaded field, even one it sent before', async () => {
    // The main window last sent these values at a profile switch.
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
