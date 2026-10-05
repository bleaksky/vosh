import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import {
  AFFECTS_MARKERS,
  AFFECTS_STYLES,
  affectsDisplayOf,
  normalizeAffectsDisplay,
  normalizeAffectsMarker,
  normalizeAffectsStyle,
  normalizeAffectsThresholds,
  setAffectsDisplay,
} from './affects';
import { normalizeUiConfig, type RawUiConfig } from './uiConfig';
import { broadcastUiConfigChanges, isOwnAffectsDisplayEcho, setUiConfig } from './uiConfigSave';

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
