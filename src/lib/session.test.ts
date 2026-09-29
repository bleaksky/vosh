import { describe, expect, it, vi } from 'vitest';
import {
  normalizeTerminalLineHeight,
  normalizeUiConfig,
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
