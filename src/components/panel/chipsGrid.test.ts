import { describe, expect, it } from 'vitest';
import { affectsPaneRows, type AffectInput, type AffectRow } from '../../lib/affectsView';
import {
  chipGroups,
  chipKind,
  chipLabelMode,
  chipPages,
  chipsMinBody,
  chipTone,
  chipWidth,
  type ChipMeasure,
  type ChipPage,
} from './chipsGrid';
import { affectHours } from './paneText';

const aff = (name: string, duration: number | null): AffectInput => ({ name, duration });

const TRACKED = [
  'mounted',
  'sanctuary',
  'bless',
  'armor',
  'shield',
  'stone skin',
  'fly',
  'levitate',
].map((name) => ({ name }));
const FOURTEEN = [
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
const TWENTY = [
  ...FOURTEEN,
  aff('frenzy', 9),
  aff('protective shield', 6),
  aff('giant strength', 40),
  aff('detect magic', 45),
  aff('faerie fire', 3),
];
const rowsOf = (current: AffectInput[], tracked = TRACKED) =>
  affectsPaneRows(current, tracked, false);

// The terminal face is 7.2 px a cell at 12 px. The UI faces get fixed
// widths per character, close to the system face.
const MEASURE: ChipMeasure = {
  mono: (s) => s.length * 7.2,
  label: (s) => s.length * 6.5,
  count: (s) => s.length * 6.6,
};
const hoursOf = (r: AffectRow) => affectHours(r.state, r.ticks);
// The boards' pane body, 494 by 191.
const BODY = 191;

/** Each line as `top group: name name…`, and the count, per page. */
function pagesOf(pages: ChipPage[]): string[][] {
  return pages.map((p) => [
    ...p.lines.map(
      (l) => `${l.top}${l.labelled ? ` ${l.group}` : ''}: ${l.rows.map((r) => r.name).join(', ')}`,
    ),
    ...(p.more > 0 ? [`${p.more} more`] : []),
  ]);
}

describe('chipGroups', () => {
  it('puts what to recast first, then the rest you track, then everything else', () => {
    const groups = chipGroups(rowsOf(TWENTY));
    expect(groups.map((g) => [g.label, g.rows.map((r) => r.name)])).toEqual([
      ['Recast', ['bless', 'sanctuary', 'fly']],
      ['Tracked', ['mounted', 'armor', 'shield', 'stone skin', 'levitate']],
      [
        'Other',
        [
          'faerie fire',
          'protective shield',
          'pass door',
          'frenzy',
          'haste',
          'bagatelle of bravado',
          'totems canticle',
          'giant strength',
          'detect magic',
          'detect invis',
          'the Triumph of One God',
          'virtues',
        ],
      ],
    ]);
  });

  it('drops empty groups and sorts running out by the hours left', () => {
    const rows = rowsOf(
      [aff('fly', 2), aff('sanctuary', 0)],
      [{ name: 'fly' }, { name: 'sanctuary' }],
    );
    expect(chipGroups(rows).map((g) => [g.id, g.rows.map((r) => r.name)])).toEqual([
      ['recast', ['sanctuary', 'fly']],
    ]);
    expect(chipGroups([])).toEqual([]);
  });

  it('puts what runs out by the hours you set in Recast, fewest hours first', () => {
    const rows = affectsPaneRows(
      [aff('fly', 4), aff('sanctuary', 2), aff('armor', 6), aff('haste', 3)],
      [{ name: 'fly' }, { name: 'sanctuary' }, { name: 'armor' }],
      false,
      { runningOut: 5, almostGone: 2 },
    );
    expect(chipGroups(rows).map((g) => [g.id, g.rows.map((r) => r.name)])).toEqual([
      ['recast', ['sanctuary', 'fly']],
      ['tracked', ['armor']],
      ['other', ['haste']],
    ]);
    const by = (name: string) => rows.find((r) => r.name === name)!;
    expect(chipTone(by('fly'))).toBe('warn');
    expect(chipTone(by('sanctuary'))).toBe('danger');
    expect(chipTone(by('armor'))).toBeNull();
    // An affect you do not track colors only its hours.
    expect(by('haste').tone).toBe('warn');
    expect(chipTone(by('haste'))).toBeNull();
  });

  it('names each chip kind and gives running out its tone only when tracked', () => {
    const rows = rowsOf(
      [aff('fly', 2), aff('haste', 1), aff('faerie fire', 1)],
      [{ name: 'fly' }, { name: 'bless' }],
    );
    const by = (name: string) => rows.find((r) => r.name === name)!;
    expect(chipKind(by('bless'))).toBe('missing');
    expect(chipKind(by('fly'))).toBe('tracked');
    expect(chipKind(by('haste'))).toBe('other');
    expect(chipKind(by('faerie fire'))).toBe('harmful');
    expect(chipTone(by('fly'))).toBe('warn');
    expect(chipTone(by('haste'))).toBeNull();
    expect(chipTone(by('faerie fire'))).toBeNull();
  });
});

describe('chipWidth', () => {
  it('matches board C to the pixel', () => {
    // 7 + name + 6 + hours + 7, rounded up.
    expect(chipWidth('bless', '-', MEASURE)).toBe(64);
    expect(chipWidth('sanctuary', '1', MEASURE)).toBe(92);
    expect(chipWidth('fly', '2', MEASURE)).toBe(49);
    expect(chipWidth('detect magic', '', MEASURE)).toBe(101);
  });

  it('measures the hours at the weight they draw in, when a face tells them apart', () => {
    // A face whose bold digits run wider: 1 px more a digit.
    const bold = { ...MEASURE, hours: (s: string) => s.length * 8.2 };
    expect(chipWidth('sanctuary', '1', bold)).toBe(93);
    expect(chipWidth('the Triumph of One God', '188', bold)).toBe(203);
  });
});

describe('chipPages', () => {
  it('packs board C at 494: Recast on one line, Tracked on two, Other on three', () => {
    const rows = rowsOf(FOURTEEN);
    const pages = chipPages(chipGroups(rows), 494, hoursOf, MEASURE, 'gutter', BODY);
    expect(pagesOf(pages)).toEqual([
      [
        '4 recast: bless, sanctuary, fly',
        '32 tracked: mounted, armor, shield, stone skin',
        '56: levitate',
        '84 other: pass door, haste, bagatelle of bravado',
        '108: totems canticle, detect invis',
        '132: the Triumph of One God, virtues',
      ],
    ]);
  });

  it('counts what does not fit on the last line of the page, as the board counts twenty', () => {
    const pages = chipPages(chipGroups(rowsOf(TWENTY)), 494, hoursOf, MEASURE, 'gutter', BODY);
    expect(pagesOf(pages)).toEqual([
      [
        '4 recast: bless, sanctuary, fly',
        '32 tracked: mounted, armor, shield, stone skin',
        '56: levitate',
        '84 other: faerie fire, protective shield, pass door',
        '108: frenzy, haste, bagatelle of bravado',
        '132: totems canticle, giant strength',
        '156: detect magic, detect invis',
        '2 more',
      ],
      // The next page starts inside Other and names it again.
      ['4 other: the Triumph of One God, virtues'],
    ]);
  });

  it('moves the last chips on until the count fits beside them', () => {
    // A body one line short: the fourth Other line cannot show, and its
    // count needs the room detect invis took.
    const narrow = chipPages(
      chipGroups(rowsOf(TWENTY)),
      494,
      hoursOf,
      { ...MEASURE, count: (s) => s.length * 30 },
      'gutter',
      BODY,
    );
    expect(pagesOf(narrow)[0].slice(-2)).toEqual(['156: detect magic', '3 more']);
  });

  it('runs the group names in under 360 px', () => {
    const pages = chipPages(chipGroups(rowsOf(FOURTEEN)), 247, hoursOf, MEASURE, 'runin', 207);
    const lines = pagesOf(pages)[0];
    expect(lines[0]).toBe('4 recast: bless, sanctuary');
    expect(lines[1]).toBe('28: fly');
    expect(lines).toContain('56 tracked: mounted, armor');
  });

  it('gives a run in name a line of its own when the first chip does not fit after it', () => {
    // Split right, 247 wide: Tracked leads with a chip 200 px wide, and
    // the name and its 8 px would leave it 163.
    const tracked = [{ name: 'the Triumph of One God' }, ...TRACKED];
    const pages = chipPages(
      chipGroups(rowsOf(FOURTEEN, tracked)),
      247,
      hoursOf,
      MEASURE,
      'runin',
      400,
    );
    expect(pagesOf(pages)[0].slice(0, 5)).toEqual([
      '4 recast: bless, sanctuary',
      '28: fly',
      '56 tracked: ',
      '80: the Triumph of One God',
      '104: mounted, armor',
    ]);
  });

  it('gives a run in name a line of its own at the top of a page too', () => {
    const current = [aff('armor', 31), aff('haste', 9), aff('the Triumph of One God', 188)];
    const groups = chipGroups(rowsOf(current, [{ name: 'armor' }]));
    expect(pagesOf(chipPages(groups, 247, hoursOf, MEASURE, 'runin', 60))).toEqual([
      ['4 tracked: armor', '32 other: haste', '1 more'],
      ['4 other: ', '28: the Triumph of One God'],
    ]);
    // A page with room for one line keeps the name and the chip together.
    expect(pagesOf(chipPages(groups, 247, hoursOf, MEASURE, 'runin', 30)).at(-1)).toEqual([
      '4 other: the Triumph of One God',
    ]);
  });

  it('starts the next page with a group whose name and first chip need two lines', () => {
    const current = [aff('armor', 31), aff('the Triumph of One God', 188)];
    const groups = chipGroups(rowsOf(current, [{ name: 'armor' }]));
    expect(pagesOf(chipPages(groups, 247, hoursOf, MEASURE, 'runin', 60))).toEqual([
      ['4 tracked: armor', '1 more'],
      ['4 other: ', '28: the Triumph of One God'],
    ]);
  });

  it('never ends a page on a name alone', () => {
    // The long chip leaves no room for the count, so it moves on, and
    // its name with it.
    const current = [aff('armor', 31), aff('the Triumph of One God', 1), aff('haste', 9)];
    const groups = chipGroups(rowsOf(current, [{ name: 'armor' }]));
    expect(pagesOf(chipPages(groups, 247, hoursOf, MEASURE, 'runin', 80))).toEqual([
      ['4 tracked: armor', '2 more'],
      ['4 other: ', '28: the Triumph of One God', '52: haste'],
    ]);
  });

  it('draws no names when you track nothing', () => {
    const rows = rowsOf(FOURTEEN, []);
    expect(chipLabelMode(rows, 494)).toBe('none');
    expect(chipLabelMode(rowsOf(FOURTEEN), 494)).toBe('gutter');
    expect(chipLabelMode(rowsOf(FOURTEEN), 359)).toBe('runin');
    const pages = chipPages(chipGroups(rows), 494, hoursOf, MEASURE, 'none', BODY);
    // Every chip is Other, by hours left, from the text edge.
    expect(pagesOf(pages)[0][0]).toBe('4: sanctuary, fly, pass door, haste');
  });

  it('keeps a chip wider than its line on a line of its own', () => {
    const rows = rowsOf([aff('the Triumph of One God', 188), aff('haste', 9)], []);
    const pages = chipPages(chipGroups(rows), 200, hoursOf, MEASURE, 'none', BODY);
    expect(pagesOf(pages)).toEqual([['4: haste', '28: the Triumph of One God']]);
  });

  it('shows one line in a body too short for it, and stops', () => {
    const pages = chipPages(chipGroups(rowsOf(FOURTEEN)), 494, hoursOf, MEASURE, 'gutter', 10);
    expect(pages[0].lines).toHaveLength(1);
    expect(pages[0].more).toBeGreaterThan(0);
  });
});

describe('chipsMinBody', () => {
  it('holds every Recast, Tracked, and harmful chip and the count after them', () => {
    const wide = chipGroups(rowsOf(TWENTY));
    // Faerie fire on the first Other line: 84 + 20.
    expect(chipsMinBody(wide, 494, hoursOf, MEASURE, 'gutter', 264)).toBe(104);
    const narrow = chipGroups(rowsOf(TWENTY));
    const h = chipsMinBody(narrow, 247, hoursOf, MEASURE, 'runin', 264);
    const [first] = chipPages(narrow, 247, hoursOf, MEASURE, 'runin', h);
    const shown = first.lines.flatMap((l) => l.rows.map((r) => r.name));
    expect(shown).toContain('faerie fire');
    expect(shown).toContain('levitate');
    // One px less loses one of them.
    const [short] = chipPages(narrow, 247, hoursOf, MEASURE, 'runin', h - 1);
    expect(short.lines.flatMap((l) => l.rows.map((r) => r.name))).not.toContain('faerie fire');
  });

  it('asks nothing when nothing needs holding, and stops at the cap', () => {
    const calm = chipGroups(rowsOf([aff('haste', 9)], []));
    expect(chipsMinBody(calm, 494, hoursOf, MEASURE, 'none', 264)).toBe(0);
    const lost = chipGroups(
      rowsOf(
        [],
        Array.from({ length: 60 }, (_, i) => ({ name: `a long tracked affect ${i}` })),
      ),
    );
    expect(chipsMinBody(lost, 247, hoursOf, MEASURE, 'runin', 264)).toBe(264);
  });
});
