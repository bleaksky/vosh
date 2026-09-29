import { describe, expect, it } from 'vitest';
import {
  affectsPaneRows,
  affectsView,
  isTrackedRow,
  type AffectInput,
  type AffectRow,
} from './affectsView';
import { HARMFUL_AFFECTS } from './harmfulAffects';

const aff = (name: string, duration: number | null): AffectInput => ({ name, duration });
const track = (...names: string[]) => names.map((name) => ({ name, label: null }));
const brief = (rows: AffectRow[]) => rows.map((r) => [r.name, r.state, r.ticks]);

describe('affectsView', () => {
  it('reproduces the approved Affects pane for Erelei', () => {
    const rows = affectsView(
      [
        aff('armor', 24),
        aff('bless', 6),
        aff('fly', 2),
        aff('giant strength', 18),
        aff('haste', 8),
        aff('poison', 3),
        aff('sanctuary', 12),
      ],
      track('sanctuary', 'haste', 'giant strength', 'fly', 'protection evil', 'detect invisible'),
    );
    expect(brief(rows)).toEqual([
      ['Protection evil', 'missing', null],
      ['Detect invisible', 'missing', null],
      ['Fly', 'expiring', 2],
      ['Haste', 'present', 8],
      ['Sanctuary', 'present', 12],
      ['Giant strength', 'present', 18],
      ['Poison', 'harmful', 3],
      ['Bless', 'untracked', 6],
      ['Armor', 'untracked', 24],
    ]);
  });

  it('lists missing tracked affects first, in tracked order', () => {
    const rows = affectsView([aff('haste', 5)], track('sanctuary', 'haste', 'armor', 'bless'));
    expect(brief(rows)).toEqual([
      ['Sanctuary', 'missing', null],
      ['Armor', 'missing', null],
      ['Bless', 'missing', null],
      ['Haste', 'present', 5],
    ]);
  });

  it('sorts present tracked affects by ticks, permanent after timed, unknown last', () => {
    const rows = affectsView(
      [aff('a', null), aff('b', -1), aff('c', 30), aff('d', 4), aff('e', 11)],
      track('a', 'b', 'c', 'd', 'e'),
    );
    expect(rows.map((r) => r.name)).toEqual(['D', 'E', 'C', 'B', 'A']);
    expect(rows.find((r) => r.key === 'b')?.ticks).toBe(-1);
    expect(rows.find((r) => r.key === 'a')?.ticks).toBeNull();
  });

  it('keeps tracked order between present affects with equal ticks', () => {
    const rows = affectsView(
      [aff('armor', 6), aff('bless', 6), aff('fly', 6)],
      track('fly', 'armor', 'bless'),
    );
    expect(rows.map((r) => r.key)).toEqual(['fly', 'armor', 'bless']);
  });

  it('marks tracked affects at two ticks or fewer as expiring', () => {
    const rows = affectsView(
      [aff('a', 0), aff('b', 1), aff('c', 2), aff('d', 3), aff('e', -1)],
      track('a', 'b', 'c', 'd', 'e'),
    );
    expect(rows.map((r) => [r.key, r.state])).toEqual([
      ['a', 'expiring'],
      ['b', 'expiring'],
      ['c', 'expiring'],
      ['d', 'present'],
      ['e', 'present'],
    ]);
  });

  it('puts harmful untracked affects before the rest, each run by ticks', () => {
    const rows = affectsView(
      [
        aff('armor', 3),
        aff('bless', 12),
        aff('curse', 20),
        aff('poison', 5),
        aff('detect magic', -1),
        aff('plague', -1),
      ],
      [],
    );
    expect(brief(rows)).toEqual([
      ['Poison', 'harmful', 5],
      ['Curse', 'harmful', 20],
      ['Plague', 'harmful', -1],
      ['Armor', 'untracked', 3],
      ['Bless', 'untracked', 12],
      ['Detect magic', 'untracked', -1],
    ]);
  });

  it('breaks untracked ties by name', () => {
    const rows = affectsView([aff('haste', 7), aff('armor', 7), aff('fly', 7)], []);
    expect(rows.map((r) => r.key)).toEqual(['armor', 'fly', 'haste']);
  });

  it('never marks an untracked affect as expiring', () => {
    const rows = affectsView([aff('armor', 1), aff('poison', 0)], []);
    expect(rows.map((r) => r.state)).toEqual(['harmful', 'untracked']);
  });

  it('matches tracked names without regard to case or spacing', () => {
    const rows = affectsView([aff('giant  strength', 9)], track('Giant Strength'));
    expect(brief(rows)).toEqual([['Giant strength', 'present', 9]]);
  });

  it('shows your label in place of the name', () => {
    const rows = affectsView(
      [aff('field of discord', 4)],
      [
        { name: 'field of discord', label: 'Shroud' },
        { name: 'sanctuary', label: 'Sanc' },
        { name: 'haste', label: '  ' },
      ],
    );
    expect(brief(rows)).toEqual([
      ['Sanc', 'missing', null],
      ['Haste', 'missing', null],
      ['Shroud', 'present', 4],
    ]);
  });

  it('leaves names that already carry capitals as written', () => {
    const rows = affectsView([aff('Detect Invisible', 9)], track('Protection EVIL'));
    expect(rows.map((r) => r.name)).toEqual(['Protection EVIL', 'Detect Invisible']);
  });

  it('collapses duplicate tracked entries', () => {
    const rows = affectsView([aff('haste', 3)], track('haste', 'Haste', 'fly', 'fly'));
    expect(brief(rows)).toEqual([
      ['Fly', 'missing', null],
      ['Haste', 'present', 3],
    ]);
  });

  it('keeps the longer of two current affects that match the same name', () => {
    expect(affectsView([aff('haste', 3), aff('Haste', 9)], [])[0].ticks).toBe(9);
    expect(affectsView([aff('haste', 9), aff('haste', -1)], [])[0].ticks).toBe(-1);
    expect(affectsView([aff('haste', 4), aff('haste', null)], [])[0].ticks).toBe(4);
  });

  it('floors fractional ticks and folds every negative into permanent', () => {
    const rows = affectsView([aff('armor', 2.7), aff('bless', -5)], []);
    expect(brief(rows)).toEqual([
      ['Armor', 'untracked', 2],
      ['Bless', 'untracked', -1],
    ]);
  });

  it('reads the harmful list you pass in place of the default', () => {
    const rows = affectsView([aff('poison', 5), aff('bless', 3)], [], ['Bless']);
    expect(brief(rows)).toEqual([
      ['Bless', 'harmful', 3],
      ['Poison', 'untracked', 5],
    ]);
  });

  it('keeps a tracked harmful affect in the tracked run', () => {
    const rows = affectsView([aff('poison', 1)], track('poison'));
    expect(brief(rows)).toEqual([['Poison', 'expiring', 1]]);
  });

  it('skips blank names and returns nothing for empty input', () => {
    expect(affectsView([], [])).toEqual([]);
    expect(affectsView([aff('  ', 3)], track(' '))).toEqual([]);
  });

  it('splits tracked rows from the rest', () => {
    const rows = affectsView([aff('armor', 3), aff('haste', 4)], track('haste', 'fly'));
    expect(rows.map(isTrackedRow)).toEqual([true, true, false]);
  });
});

describe('affectsPaneRows', () => {
  it('draws nothing before the first list or while the game hides your affects', () => {
    expect(affectsPaneRows(null, track('sanctuary'), false)).toEqual([]);
    // Lamented tears sends an empty list with the hidden flag. No
    // tracked affect reads missing then.
    expect(affectsPaneRows([], track('sanctuary', 'haste'), true)).toEqual([]);
  });

  it('draws the checklist once the game shows your affects again', () => {
    const rows = affectsPaneRows([aff('haste', 4)], track('sanctuary', 'haste'), false);
    expect(rows.map((r) => [r.key, r.state])).toEqual([
      ['sanctuary', 'missing'],
      ['haste', 'present'],
    ]);
  });
});

describe('HARMFUL_AFFECTS', () => {
  it('names the stock ROM debuffs in lower case', () => {
    for (const name of ['poison', 'plague', 'curse', 'blindness', 'weaken', 'slow', 'sleep']) {
      expect(HARMFUL_AFFECTS).toContain(name);
    }
    for (const name of ['faerie fire', 'chill touch', 'energy drain', 'calm', 'charm person']) {
      expect(HARMFUL_AFFECTS).toContain(name);
    }
    for (const name of HARMFUL_AFFECTS) expect(name).toBe(name.toLowerCase().trim());
  });

  it('holds no duplicates', () => {
    expect(new Set(HARMFUL_AFFECTS).size).toBe(HARMFUL_AFFECTS.length);
  });
});
