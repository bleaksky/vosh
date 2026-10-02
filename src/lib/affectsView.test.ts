import { describe, expect, it } from 'vitest';
import {
  affectMark,
  affectsPaneRows,
  affectsSummary,
  affectsView,
  gaugeFraction,
  hoursTone,
  isTrackedRow,
  type AffectInput,
  type AffectRow,
} from './affectsView';
import { HARMFUL_AFFECTS } from './harmfulAffects';

const aff = (name: string, duration: number | null): AffectInput => ({ name, duration });
const track = (...names: string[]) => names.map((name) => ({ name, label: null }));
const brief = (rows: AffectRow[]) => rows.map((r) => [r.name, r.state, r.ticks]);

// Ilsabet on the approved board: his eight tracked affects in his order,
// and the list the game sends while bless has worn off.
const ILSABET_TRACKED = track(
  'mounted',
  'sanctuary',
  'bless',
  'armor',
  'shield',
  'stone skin',
  'fly',
  'levitate',
);
const ILSABET_AFFECTS = [
  aff('pass door', 8),
  aff('levitate', 44),
  aff('detect invis', 47),
  aff('sanctuary', 1),
  aff('haste', 14),
  aff('stone skin', 38),
  aff('shield', 31),
  aff('armor', 31),
  aff('fly', 2),
  aff('the Triumph of One God', 188),
  aff('mounted', -1),
  aff('virtues', -1),
  aff('totems canticle', 22),
  aff('bagatelle of bravado', 19),
];

describe('affectsView', () => {
  it('reproduces the approved Affects pane for Ilsabet', () => {
    // The board's affects list, newest first as the game prints it.
    const rows = affectsView(ILSABET_AFFECTS, ILSABET_TRACKED);
    expect(brief(rows)).toEqual([
      ['mounted', 'present', -1],
      ['sanctuary', 'expiring', 1],
      ['bless', 'missing', null],
      ['armor', 'present', 31],
      ['shield', 'present', 31],
      ['stone skin', 'present', 38],
      ['fly', 'expiring', 2],
      ['levitate', 'present', 44],
      ['pass door', 'untracked', 8],
      ['haste', 'untracked', 14],
      ['bagatelle of bravado', 'untracked', 19],
      ['totems canticle', 'untracked', 22],
      ['detect invis', 'untracked', 47],
      ['the Triumph of One God', 'untracked', 188],
      ['virtues', 'untracked', -1],
    ]);
    expect(affectsSummary(rows)).toEqual({ missing: 1, runningOut: 2 });
  });

  it('keeps a missing tracked affect in its slot', () => {
    const rows = affectsView([aff('haste', 5)], track('sanctuary', 'haste', 'armor', 'bless'));
    expect(brief(rows)).toEqual([
      ['sanctuary', 'missing', null],
      ['haste', 'present', 5],
      ['armor', 'missing', null],
      ['bless', 'missing', null],
    ]);
  });

  it('keeps tracked affects in your order whatever their ticks', () => {
    const rows = affectsView(
      [aff('a', null), aff('b', -1), aff('c', 30), aff('d', 4), aff('e', 11)],
      track('a', 'b', 'c', 'd', 'e'),
    );
    expect(rows.map((r) => r.name)).toEqual(['a', 'b', 'c', 'd', 'e']);
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
      ['poison', 'harmful', 5],
      ['curse', 'harmful', 20],
      ['plague', 'harmful', -1],
      ['armor', 'untracked', 3],
      ['bless', 'untracked', 12],
      ['detect magic', 'untracked', -1],
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
    expect(brief(rows)).toEqual([['giant  strength', 'present', 9]]);
  });

  it('shows your label in place of the name', () => {
    const rows = affectsView(
      [aff('field of discord', 4)],
      [
        { name: 'field of discord', label: 'Shroud' },
        { name: 'sanctuary', label: 'sanc' },
        { name: 'haste', label: '  ' },
      ],
    );
    expect(brief(rows)).toEqual([
      ['Shroud', 'present', 4],
      ['sanc', 'missing', null],
      ['haste', 'missing', null],
    ]);
  });

  it('shows every name exactly as the game sends it or you track it', () => {
    const rows = affectsView(
      [aff('stone skin', 9), aff('Battle Hymn', 4), aff('weapon: soul reaver', -1)],
      track('Protection EVIL', 'stone skin'),
    );
    expect(rows.map((r) => r.name)).toEqual([
      'Protection EVIL',
      'stone skin',
      'Battle Hymn',
      'weapon: soul reaver',
    ]);
  });

  it('collapses duplicate tracked entries', () => {
    const rows = affectsView([aff('haste', 3)], track('haste', 'Haste', 'fly', 'fly'));
    expect(brief(rows)).toEqual([
      ['haste', 'present', 3],
      ['fly', 'missing', null],
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
      ['armor', 'untracked', 2],
      ['bless', 'untracked', -1],
    ]);
  });

  it('reads the harmful list you pass in place of the default', () => {
    const rows = affectsView([aff('poison', 5), aff('bless', 3)], [], ['Bless']);
    expect(brief(rows)).toEqual([
      ['bless', 'harmful', 3],
      ['poison', 'untracked', 5],
    ]);
  });

  it('keeps a tracked harmful affect in the tracked run', () => {
    const rows = affectsView([aff('poison', 1)], track('poison'));
    expect(brief(rows)).toEqual([['poison', 'expiring', 1]]);
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

describe('hoursTone', () => {
  it('follows the game, red at one hour or none, and warns at two', () => {
    expect(hoursTone(0)).toBe('danger');
    expect(hoursTone(1)).toBe('danger');
    expect(hoursTone(2)).toBe('warn');
    expect(hoursTone(3)).toBeNull();
    expect(hoursTone(188)).toBeNull();
    expect(hoursTone(-1)).toBeNull();
    expect(hoursTone(null)).toBeNull();
  });
});

describe('affectMark', () => {
  const row = (state: AffectRow['state'], ticks: number | null): AffectRow => ({
    key: 'x',
    name: 'x',
    state,
    ticks,
  });

  it('gives each tracked slot a dot that agrees with its hours', () => {
    expect(affectMark(row('present', 31))).toBe('up');
    expect(affectMark(row('present', -1))).toBe('up');
    expect(affectMark(row('present', null))).toBe('up');
    expect(affectMark(row('expiring', 2))).toBe('warn');
    expect(affectMark(row('expiring', 1))).toBe('danger');
    expect(affectMark(row('expiring', 0))).toBe('danger');
    expect(affectMark(row('missing', null))).toBe('missing');
  });

  it('marks a harmful affect and leaves the rest bare', () => {
    expect(affectMark(row('harmful', 3))).toBe('harmful');
    expect(affectMark(row('harmful', 1))).toBe('harmful');
    expect(affectMark(row('untracked', 8))).toBeNull();
    expect(affectMark(row('untracked', 1))).toBeNull();
  });
});

describe('affectsSummary', () => {
  it('counts nothing when every tracked affect is up for a while', () => {
    const rows = affectsView([aff('armor', 31), aff('poison', 1)], track('armor'));
    expect(affectsSummary(rows)).toEqual({ missing: 0, runningOut: 0 });
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

describe('gaugeFraction', () => {
  const row = (state: AffectRow['state'], ticks: number | null, key = 'armor'): AffectRow => ({
    key,
    name: key,
    state,
    ticks,
  });
  const full = { armor: 48, sanctuary: 10 };

  it('drains from full toward empty as the hours run down', () => {
    expect(gaugeFraction(row('present', 48), full)).toBe(1);
    expect(gaugeFraction(row('present', 24), full)).toBe(0.5);
    expect(gaugeFraction(row('expiring', 1, 'sanctuary'), full)).toBe(0.1);
    expect(gaugeFraction(row('expiring', 0, 'sanctuary'), full)).toBe(0);
    expect(gaugeFraction(row('untracked', 12), full)).toBe(0.25);
  });

  it('reads empty while missing and full while permanent', () => {
    expect(gaugeFraction(row('missing', null), full)).toBe(0);
    expect(gaugeFraction(row('present', -1), full)).toBe(1);
  });

  it('reads full for an affect Vosh has no full for, and null with no hours', () => {
    expect(gaugeFraction(row('present', 30, 'haste'), full)).toBe(1);
    expect(gaugeFraction(row('present', 0, 'haste'), { haste: 0 })).toBe(1);
    expect(gaugeFraction(row('present', null), full)).toBeNull();
  });

  it('never passes full, even before a recast reaches the store', () => {
    expect(gaugeFraction(row('present', 60), full)).toBe(1);
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
