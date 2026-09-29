import { describe, expect, it } from 'vitest';
import {
  buildSections,
  filterSections,
  groupKeyOf,
  neighborUid,
  PRESET_SECTION_HEADING,
  searchText,
  sectionOrder,
  type ListEntry,
} from './automationList';

const entry = (uid: string, name: string, group = '', extra: Partial<ListEntry> = {}) => ({
  uid,
  name,
  group,
  enabled: true,
  text: searchText(name, group, extra.meta),
  ...extra,
});

describe('buildSections', () => {
  it('puts ungrouped items first, then groups in alphabetical order as typed', () => {
    const sections = buildSections([
      entry('a', 'Open doors on the way', 'travel'),
      entry('b', 'Sleep when mana is low', 'idle'),
      entry('c', 'Flee below 20 percent', 'combat'),
      entry('d', 'Echo my deaths'),
      entry('e', 'Loot every kill', 'combat'),
      entry('f', 'Highlight WiZNET', 'Chat'),
    ]);
    expect(sections.map((s) => s.heading)).toEqual([null, 'Chat', 'combat', 'idle', 'travel']);
    expect(sections[2].entries.map((e) => e.uid)).toEqual(['c', 'e']);
  });

  it('keeps groups that differ in case apart', () => {
    const sections = buildSections([entry('a', 'One', 'Combat'), entry('b', 'Two', 'combat')]);
    expect(sections.map((s) => s.heading)).toEqual(['Combat', 'combat']);
  });

  it('closes the list with ungrouped presets and files grouped presets with yours', () => {
    const sections = buildSections([
      entry('p1', 'healing.cure', '', { preset: true }),
      entry('p2', 'disarm.primary', 'combat', { preset: true }),
      entry('u', 'Loot every kill', 'combat'),
    ]);
    expect(sections.map((s) => s.heading)).toEqual(['combat', PRESET_SECTION_HEADING]);
    expect(sections[0].entries.map((e) => e.uid)).toEqual(['p2', 'u']);
  });

  it('trims the group it files under', () => {
    expect(groupKeyOf('  idle ')).toBe('idle');
    expect(groupKeyOf(null)).toBe('');
  });
});

describe('filterSections', () => {
  const sections = buildSections([
    entry('a', 'Sleep when mana is low', 'idle', { meta: 'sleep' }),
    entry('b', 'Flee below 20 percent', 'combat', { meta: 'flee' }),
    entry('c', 'Loot every kill', 'combat', { meta: 'get all corpse' }),
  ]);

  it('matches every word against the name, group, and meta', () => {
    expect(sectionOrder(filterSections(sections, 'combat corpse'))).toEqual(['c']);
    expect(sectionOrder(filterSections(sections, 'MANA'))).toEqual(['a']);
    expect(filterSections(sections, 'nothing here')).toEqual([]);
  });

  it('keeps everything for an empty query', () => {
    expect(sectionOrder(filterSections(sections, '  '))).toEqual(['b', 'c', 'a']);
  });

  it('stays quick with 500 entries', () => {
    const many = Array.from({ length: 500 }, (_, i) =>
      entry(`u${i}`, `Trigger ${i}`, `group ${i % 20}`, { meta: `pattern ${i}` }),
    );
    const started = performance.now();
    for (let i = 0; i < 20; i += 1) filterSections(buildSections(many), `trigger ${i}`);
    expect(performance.now() - started).toBeLessThan(500);
  });
});

describe('neighborUid', () => {
  it('picks the next row, then the one before, then none', () => {
    expect(neighborUid(['a', 'b', 'c'], 'b')).toBe('c');
    expect(neighborUid(['a', 'b', 'c'], 'c')).toBe('b');
    expect(neighborUid(['a'], 'a')).toBeNull();
  });
});
