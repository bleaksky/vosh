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

  it('hides GPU rendering on macOS', () => {
    expect(labels('gpu')).toEqual([]);
    expect(labels('gpu', { pathB: false, mac: false })).toEqual(['GPU rendering']);
  });
});
