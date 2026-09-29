import { describe, expect, it, vi } from 'vitest';
import { emit } from '@tauri-apps/api/event';
import {
  broadcastUiConfigChanges,
  isOwnThemeEcho,
  normalizeChipStyle,
  normalizeTerminalLineHeight,
  normalizeUiConfig,
  normalizeVitalsDensity,
  primeUiConfigThemePrefs,
  seedDarkTheme,
  TERMINAL_LINE_HEIGHTS,
  type CustomTheme,
  type RawUiConfig,
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
