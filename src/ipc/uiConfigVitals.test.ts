import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { normalizeUiConfig, setUiFields, type RawUiConfig, type UiFields } from './uiConfig';
import { broadcastUiConfigChanges } from './uiConfigBroadcast';
import {
  DEFAULT_VITALS_CUSTOM,
  DEFAULT_VITALS_OPTIONS,
  normalizeVitalsDensity,
  normalizeVitalsMeter,
  normalizeVitalsOptions,
  normalizeVitalsValues,
  shownStyle,
  VITALS_STYLES,
  vitalsOptionsOf,
} from './uiConfigVitals';

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
      'orbs',
      'candles',
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
