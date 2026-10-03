import { describe, expect, it } from 'vitest';
import {
  buildSections,
  filterSections,
  foldedGroupOf,
  foldedStorageKey,
  foldKeyOf,
  foldsInView,
  groupKeyOf,
  listKeyAction,
  listStops,
  loadFolded,
  neighborUid,
  PRESET_SECTION_HEADING,
  saveFolded,
  searchText,
  sectionKeyOf,
  sectionOrder,
  stopId,
  tabStopId,
  visibleOrder,
  withFold,
  type ListEntry,
  type ListStop,
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

describe('folding groups', () => {
  const sections = buildSections([
    entry('u1', 'Echo my deaths'),
    entry('c1', 'Flee below 20 percent', 'combat'),
    entry('c2', 'Loot every kill', 'combat'),
    entry('i1', 'Sleep when mana is low', 'idle'),
    entry('p1', 'healing.cure', '', { preset: true }),
  ]);
  const combat = sectionKeyOf({ group: 'combat' });
  const idle = sectionKeyOf({ group: 'idle' });
  const presets = sectionKeyOf({ group: '', preset: true });
  const ungrouped = sectionKeyOf({ group: '' });
  const ids = (stops: ListStop[]) => stops.map(stopId);

  it('keys a group by its name and never folds the ungrouped items', () => {
    expect(combat).toBe('g:combat');
    expect(sections.map((s) => s.key)).toEqual([ungrouped, combat, idle, presets]);
    expect(sections.map(foldKeyOf)).toEqual([null, combat, idle, presets]);
    // Folding the ungrouped key changes nothing.
    expect(visibleOrder(sections, new Set([ungrouped]))).toEqual(sectionOrder(sections));
  });

  it('stops on each heading, then its rows while it is open', () => {
    expect(ids(listStops(sections, new Set()))).toEqual([
      'row:u1',
      'heading:g:combat',
      'row:c1',
      'row:c2',
      'heading:g:idle',
      'row:i1',
      'heading:p:',
      'row:p1',
    ]);
  });

  it('drops the rows of a folded group from the stops and the order', () => {
    const folded = new Set([combat, presets]);
    expect(ids(listStops(sections, folded))).toEqual([
      'row:u1',
      'heading:g:combat',
      'heading:g:idle',
      'row:i1',
      'heading:p:',
    ]);
    expect(visibleOrder(sections, folded)).toEqual(['u1', 'i1']);
    expect(foldedGroupOf(sections, folded, 'c2')).toBe(combat);
    expect(foldedGroupOf(sections, folded, 'i1')).toBeNull();
    expect(foldedGroupOf(sections, folded, 'gone')).toBeNull();
  });

  it('moves Up and Down through headings and rows as one list', () => {
    const stops = listStops(sections, new Set([combat]));
    const move = (at: string, key: string) => listKeyAction(stops, at, key);
    expect(move('row:u1', 'ArrowDown')).toEqual({
      type: 'move',
      to: { kind: 'heading', key: combat },
    });
    expect(move('heading:g:combat', 'ArrowDown')).toEqual({
      type: 'move',
      to: { kind: 'heading', key: idle },
    });
    expect(move('heading:g:idle', 'ArrowUp')).toEqual({
      type: 'move',
      to: { kind: 'heading', key: combat },
    });
    expect(move('row:u1', 'ArrowUp')).toEqual({ type: 'move', to: { kind: 'row', uid: 'u1' } });
    expect(move('row:i1', 'End')).toEqual({ type: 'move', to: { kind: 'row', uid: 'p1' } });
    expect(move('row:i1', 'Home')).toEqual({ type: 'move', to: { kind: 'row', uid: 'u1' } });
  });

  it('folds a heading with Left and opens it with Right, and leaves rows alone', () => {
    const stops = listStops(sections, new Set());
    expect(listKeyAction(stops, 'heading:g:idle', 'ArrowLeft')).toEqual({
      type: 'fold',
      key: idle,
      fold: true,
    });
    expect(listKeyAction(stops, 'heading:g:idle', 'ArrowRight')).toEqual({
      type: 'fold',
      key: idle,
      fold: false,
    });
    expect(listKeyAction(stops, 'row:c1', 'ArrowLeft')).toBeNull();
    expect(listKeyAction(stops, 'row:c1', 'Enter')).toBeNull();
    expect(listKeyAction([], null, 'ArrowDown')).toBeNull();
  });

  it('gives Tab to the heading you moved to, the selected row, or the heading that hides it', () => {
    const folded = new Set([combat]);
    const stops = listStops(sections, folded);
    expect(tabStopId(stops, sections, folded, 'i1', null)).toBe('row:i1');
    expect(tabStopId(stops, sections, folded, 'c2', null)).toBe('heading:g:combat');
    expect(tabStopId(stops, sections, folded, 'i1', 'heading:g:idle')).toBe('heading:g:idle');
    // A heading that is gone gives way to the selection.
    expect(tabStopId(stops, sections, folded, 'i1', 'heading:g:travel')).toBe('row:i1');
    expect(tabStopId(stops, sections, folded, null, null)).toBe('row:u1');
    expect(tabStopId([], [], folded, null, null)).toBeNull();
  });

  it('gives Tab to the first match, not its heading, when the filter leaves the selection out', () => {
    const none: ReadonlySet<string> = new Set();
    const matches = filterSections(sections, 'flee');
    const stops = listStops(matches, none);
    expect(ids(stops)).toEqual(['heading:g:combat', 'row:c1']);
    expect(tabStopId(stops, matches, none, 'i1', null)).toBe('row:c1');
    expect(tabStopId(stops, matches, none, null, null)).toBe('row:c1');
    // With every match folded no row shows, so the first heading takes it.
    const folded = new Set([combat]);
    const hidden = listStops(matches, folded);
    expect(tabStopId(hidden, matches, folded, 'i1', null)).toBe('heading:g:combat');
  });

  it('folds and opens one key at a time, and keeps the set when nothing changes', () => {
    const none: ReadonlySet<string> = new Set();
    const one = withFold(none, combat, true);
    expect([...one]).toEqual([combat]);
    expect(withFold(one, combat, true)).toBe(one);
    expect([...withFold(one, combat, false)]).toEqual([]);
    expect(none.size).toBe(0);
  });

  it('opens every group with a match while the filter has text, then restores the folds', () => {
    const kept = new Set([combat, idle]);
    expect(foldsInView(kept, null, '')).toBe(kept);
    expect(foldsInView(kept, null, '   ')).toBe(kept);
    expect(foldsInView(kept, null, 'flee').size).toBe(0);
    // A fold made under one filter lasts while the text stays the same.
    const search = { filter: 'flee', folded: new Set([combat]) };
    expect([...foldsInView(kept, search, 'flee')]).toEqual([combat]);
    expect(foldsInView(kept, search, 'flee b').size).toBe(0);
    expect(foldsInView(kept, search, '')).toBe(kept);
  });
});

describe('remembering folds', () => {
  function memory(initial: Record<string, string> = {}) {
    const store = new Map(Object.entries(initial));
    return {
      store,
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => void store.set(key, value),
      removeItem: (key: string) => void store.delete(key),
    };
  }

  it('keeps each list apart under its own key', () => {
    expect(foldedStorageKey('triggers')).toBe('vosh.automation.folded.triggers');
    const storage = memory();
    saveFolded(storage, 'triggers', new Set(['g:idle', 'g:combat']));
    saveFolded(storage, 'macros', new Set(['g:travel']));
    expect(storage.store.get('vosh.automation.folded.triggers')).toBe('["g:combat","g:idle"]');
    expect([...loadFolded(storage, 'triggers')]).toEqual(['g:combat', 'g:idle']);
    expect([...loadFolded(storage, 'macros')]).toEqual(['g:travel']);
    expect(loadFolded(storage, 'aliases').size).toBe(0);
  });

  it('leaves no key once nothing is folded', () => {
    const storage = memory({ 'vosh.automation.folded.triggers': '["g:idle"]' });
    saveFolded(storage, 'triggers', new Set());
    expect(storage.store.has('vosh.automation.folded.triggers')).toBe(false);
  });

  it('folds nothing when storage holds something else or refuses', () => {
    const odd = memory({
      'vosh.automation.folded.triggers': '{"g:idle":true}',
      'vosh.automation.folded.aliases': 'not json',
      'vosh.automation.folded.macros': '["g:travel", 4, null]',
    });
    expect(loadFolded(odd, 'triggers').size).toBe(0);
    expect(loadFolded(odd, 'aliases').size).toBe(0);
    expect([...loadFolded(odd, 'macros')]).toEqual(['g:travel']);
    expect(loadFolded(null, 'triggers').size).toBe(0);
    const refuse = () => {
      throw new Error('blocked');
    };
    const refusing = { getItem: refuse, setItem: refuse, removeItem: refuse };
    expect(loadFolded(refusing, 'triggers').size).toBe(0);
    expect(() => saveFolded(refusing, 'triggers', new Set(['g:idle']))).not.toThrow();
    expect(() => saveFolded(refusing, 'triggers', new Set())).not.toThrow();
  });
});
