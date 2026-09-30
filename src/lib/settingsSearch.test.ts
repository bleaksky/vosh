import { describe, expect, it } from 'vitest';
import { formatSettingsTarget, resolveSettingsTarget } from './settingsNav';
import { SETTINGS_ROWS, searchSettingsRows, settingsRowKey } from './settingsSearch';

const mac = { pathB: false, mac: true };
const labels = (query: string, env = mac) => searchSettingsRows(query, env).map((r) => r.label);

describe('SETTINGS_ROWS', () => {
  it('gives every row a unique target that survives the deep link round trip', () => {
    const keys = SETTINGS_ROWS.map(settingsRowKey);
    expect(new Set(keys).size).toBe(keys.length);
    for (const row of SETTINGS_ROWS) {
      expect(resolveSettingsTarget(formatSettingsTarget(row.target))).toEqual(row.target);
    }
  });

  it('covers all six groups', () => {
    const groups = new Set(SETTINGS_ROWS.map((r) => r.target.group));
    expect([...groups]).toEqual([
      'general',
      'appearance',
      'layout',
      'input',
      'automation',
      'characters',
    ]);
  });
});

describe('searchSettingsRows', () => {
  it('finds nothing for an empty query', () => {
    expect(labels('')).toEqual([]);
    expect(labels('   ')).toEqual([]);
  });

  it('ranks a label that starts with the query first', () => {
    expect(labels('line')[0]).toBe('Line height');
    expect(labels('font')[0]).toBe('Font');
    expect(labels('Tracked')[0]).toBe('Tracked affects');
  });

  it('matches descriptions and keywords', () => {
    expect(labels('ghostty')).toEqual(['Import a theme']);
    expect(labels('cursor')).toEqual(['Caret shape']);
    expect(labels('missing')).toContain('Tracked affects');
  });

  it('needs every word, in any field', () => {
    expect(labels('dark theme')[0]).toBe('Dark theme');
    expect(labels('import')).toEqual(['Import a theme', 'Import from another client']);
  });

  it('finds a group by its name', () => {
    const rows = searchSettingsRows('automation', mac);
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.every((r) => r.target.group === 'automation')).toBe(true);
  });

  it('ignores case and accents', () => {
    expect(labels('LINE HEIGHT')).toEqual(['Line height']);
    expect(labels('thème')[0]).toBe('Theme');
  });

  it('shows loadouts only in loadout mode', () => {
    expect(labels('loadouts')).not.toContain('Loadouts');
    expect(labels('loadouts', { pathB: true, mac: true })).toContain('Loadouts');
  });

  it('finds the Advanced appearance rows', () => {
    expect(labels('font stack')).toEqual(['Font stack']);
    expect(labels('fallback')).toEqual(['Font stack']);
    expect(labels('palette')[0]).toBe('Base palette');
  });

  it('finds the Input rows, Advanced ones included', () => {
    expect(labels('paste')).toEqual(['Wait between pasted lines']);
    expect(labels('sent command')[0]).toBe('Sent command color');
    expect(labels('prompt template')).toEqual(['Draw your own prompt']);
    const advanced = searchSettingsRows('prompt', mac).find(
      (r) => r.label === 'Draw your own prompt',
    );
    expect(advanced?.target).toEqual({ group: 'input', section: 'advanced', anchor: 'prompt' });
  });

  it('finds the Layout rows', () => {
    expect(labels('vitals')).toContain('Density');
    expect(labels('one line')).toEqual(['Density']);
    expect(labels('panel width')[0]).toBe('Width');
    expect(labels('divider')).toEqual(['Divider color']);
    const divider = searchSettingsRows('divider', mac)[0];
    expect(divider.target).toEqual({ group: 'layout', section: 'split', anchor: 'divider-color' });
  });

  it('finds the Vitals rows the VitalsOptions board adds', () => {
    expect(labels('vitals')).toEqual(
      expect.arrayContaining(['Density', 'Values', 'Meter', 'Warn before you run low']),
    );
    expect(labels('percent')[0]).toBe('Values');
    expect(labels('meter')[0]).toBe('Meter');
    expect(labels('bar')).toContain('Meter');
    expect(labels('warn')[0]).toBe('Warn before you run low');
    expect(labels('run low')).toEqual(['Warn before you run low']);
    const targets = ['values', 'meter', 'warn-low'].map(
      (anchor) => SETTINGS_ROWS.find((r) => r.target.anchor === anchor)?.target,
    );
    expect(targets).toEqual([
      { group: 'layout', section: 'vitals', anchor: 'values' },
      { group: 'layout', section: 'vitals', anchor: 'meter' },
      { group: 'layout', section: 'vitals', anchor: 'warn-low' },
    ]);
  });

  it('finds the affects style and marker under Layout', () => {
    const [style] = searchSettingsRows('grouped chips', mac);
    expect(style.label).toBe('Style');
    expect(settingsRowKey(style)).toBe('layout:affects#affects-style');
    expect(labels('countdown')).toContain('Style');
    expect(labels('timers first')).toEqual(['Style']);
    const [marker] = searchSettingsRows('affects dot', mac);
    expect(marker.label).toBe('Marker');
    expect(settingsRowKey(marker)).toBe('layout:affects#affects-marker');
    expect(labels('square')).toContain('Marker');
    expect(labels('plus minus')).toContain('Marker');
  });

  it('finds the tick and time style under Layout', () => {
    const [row] = searchSettingsRows('chip style', mac);
    expect(row.label).toBe('Tick and time');
    expect(settingsRowKey(row)).toBe('layout:status#tick-time');
    expect(labels('status line')).toContain('Tick and time');
    expect(labels('icon')).toContain('Tick and time');
    expect(labels('moons')).toContain('Tick and time');
  });

  it('finds which way the tick counts under Layout', () => {
    const [row] = searchSettingsRows('tick count', mac);
    expect(row.label).toBe('Tick counts');
    expect(settingsRowKey(row)).toBe('layout:status#tick-counts');
    // The Countdown affects style matches too, after the tick.
    expect(labels('countdown')).toEqual(['Tick counts', 'Style']);
    expect(labels('below zero')).toEqual(['Tick counts']);
    expect(labels('tick')).toEqual(expect.arrayContaining(['Tick and time', 'Tick counts']));
  });

  it('hides GPU rendering on macOS', () => {
    expect(labels('gpu')).toEqual([]);
    expect(labels('gpu', { pathB: false, mac: false })).toEqual(['GPU rendering']);
  });

  it('opens the GPU row inside General Advanced', () => {
    const [row] = searchSettingsRows('gpu', { pathB: false, mac: false });
    expect(row.target).toEqual({ group: 'general', section: 'advanced', anchor: 'gpu' });
  });

  it('finds the log view by the old tab id target', () => {
    const [row] = searchSettingsRows('search logs', mac);
    expect(row.label).toBe('Search logs');
    expect(row.target).toEqual(resolveSettingsTarget('logs'));
    expect(labels('saved sessions')[0]).toBe('Session logs');
  });
});
