import { describe, expect, it } from 'vitest';
import type { AffectRow, AffectRowState } from '../../lib/affectsView';
import {
  AFFECTS_RULE_PX,
  AFFECTS_TWO_COLUMNS_W,
  affectsColumns,
  affectsGrid,
  type AffectsGrid,
} from './affectsGrid';

const row = (name: string, state: AffectRowState, ticks: number | null = 10): AffectRow => ({
  key: name,
  name,
  state,
  ticks,
});

// Erelei on the approved board: eight tracked slots, bless missing,
// then seven he does not track, by hours left.
const TRACKED = [
  row('mounted', 'present', -1),
  row('sanctuary', 'expiring', 1),
  row('bless', 'missing', null),
  row('armor', 'present', 31),
  row('shield', 'present', 31),
  row('stone skin', 'present', 38),
  row('fly', 'expiring', 2),
  row('levitate', 'present', 44),
];
const REST = [
  row('pass door', 'untracked', 8),
  row('haste', 'untracked', 14),
  row('bagatelle of bravado', 'untracked', 19),
  row('totems canticle', 'untracked', 22),
  row('detect invis', 'untracked', 47),
  row('the Triumph of One God', 'untracked', 188),
  row('virtues', 'untracked', -1),
];
// The board's pane is 494 by 219, a 191 px body under the header.
const BOARD_BOX = { width: 494, height: 191 };

/** Each cell as `name@row,column`, or `N more@row,column`. */
function cells(grid: AffectsGrid): string[] {
  return grid.rest.map((c) =>
    c.kind === 'affect'
      ? `${c.row.name}@${c.gridRow},${c.gridColumn}`
      : `${c.count} more@${c.gridRow},${c.gridColumn}`,
  );
}

describe('affectsColumns', () => {
  it('draws two columns in a pane wide enough for the names, else one', () => {
    expect(affectsColumns(494)).toBe(2);
    expect(affectsColumns(AFFECTS_TWO_COLUMNS_W)).toBe(2);
    expect(affectsColumns(AFFECTS_TWO_COLUMNS_W - 1)).toBe(1);
    expect(affectsColumns(247)).toBe(1);
  });
});

describe('affectsGrid', () => {
  it('lays out the approved board, tracked row by row, the rest down each column', () => {
    const grid = affectsGrid([...TRACKED, ...REST], BOARD_BOX);
    expect(grid.columns).toBe(2);
    expect(grid.tracked.map((r) => r.name)).toEqual(TRACKED.map((r) => r.name));
    expect(grid.rule).toBe(true);
    expect(grid.pageRows).toBe(4);
    expect(grid.pages).toBe(1);
    expect(cells(grid)).toEqual([
      'pass door@1,1',
      'haste@2,1',
      'bagatelle of bravado@3,1',
      'totems canticle@4,1',
      'detect invis@1,2',
      'the Triumph of One God@2,2',
      'virtues@3,2',
    ]);
    // Four tracked rows, the rule, and four more rows: 185 of 191 px.
    expect(4 * 22 + AFFECTS_RULE_PX + grid.pageRows * 22).toBe(185);
  });

  it('counts what does not fit in the last cell and pages the rest', () => {
    const more = [
      row('faerie fire', 'harmful', 3),
      row('protective shield', 'untracked', 6),
      row('pass door', 'untracked', 8),
      row('frenzy', 'untracked', 9),
      row('haste', 'untracked', 14),
      row('bagatelle of bravado', 'untracked', 19),
      row('totems canticle', 'untracked', 22),
      row('giant strength', 'untracked', 40),
      row('detect magic', 'untracked', 45),
      row('detect invis', 'untracked', 47),
      row('the Triumph of One God', 'untracked', 188),
      row('virtues', 'untracked', -1),
    ];
    const grid = affectsGrid([...TRACKED, ...more], BOARD_BOX);
    expect(grid.pageRows).toBe(4);
    expect(grid.pages).toBe(2);
    expect(cells(grid)).toEqual([
      'faerie fire@1,1',
      'protective shield@2,1',
      'pass door@3,1',
      'frenzy@4,1',
      'haste@1,2',
      'bagatelle of bravado@2,2',
      'totems canticle@3,2',
      '5 more@4,2',
      'giant strength@5,1',
      'detect magic@6,1',
      'detect invis@7,1',
      'the Triumph of One God@8,1',
      'virtues@5,2',
    ]);
    const counter = grid.rest.find((c) => c.kind === 'more');
    expect(counter).toMatchObject({ kind: 'more', count: 5, page: 0 });
  });

  it('counts again at the end of every page but the last', () => {
    const many = Array.from({ length: 9 }, (_, i) => row(`a${i}`, 'untracked', i + 3));
    // One column, a 44 px body: two rows a page, one affect and the count.
    const grid = affectsGrid(many, { width: 300, height: 44 });
    expect(grid.columns).toBe(1);
    expect(grid.pageRows).toBe(2);
    expect(cells(grid)).toEqual([
      'a0@1,1',
      '8 more@2,1',
      'a1@3,1',
      '7 more@4,1',
      'a2@5,1',
      '6 more@6,1',
      'a3@7,1',
      '5 more@8,1',
      'a4@9,1',
      '4 more@10,1',
      'a5@11,1',
      '3 more@12,1',
      'a6@13,1',
      '2 more@14,1',
      'a7@15,1',
      'a8@16,1',
    ]);
    expect(grid.pages).toBe(8);
  });

  it('never shows a partial row', () => {
    // 184 px holds 4 tracked rows, the rule, and 3 whole rows, not 4.
    const grid = affectsGrid([...TRACKED, ...REST], { width: 494, height: 184 });
    expect(grid.pageRows).toBe(3);
    expect(cells(grid).slice(0, 6)).toEqual([
      'pass door@1,1',
      'haste@2,1',
      'bagatelle of bravado@3,1',
      'totems canticle@1,2',
      'detect invis@2,2',
      '2 more@3,2',
    ]);
  });

  it('starts at the top with no rule when you track nothing', () => {
    const grid = affectsGrid(REST, { width: 494, height: 88 });
    expect(grid.tracked).toEqual([]);
    expect(grid.rule).toBe(false);
    expect(grid.pageRows).toBe(4);
    expect(grid.pages).toBe(1);
  });

  it('draws only the slots when nothing else affects you', () => {
    const grid = affectsGrid(TRACKED, BOARD_BOX);
    expect(grid.rule).toBe(false);
    expect(grid.rest).toEqual([]);
    expect(grid.pageRows).toBe(0);
    expect(grid.pages).toBe(0);
  });

  it('keeps one row of the rest in a pane too short for it', () => {
    const grid = affectsGrid([...TRACKED, ...REST], { width: 494, height: 60 });
    expect(grid.pageRows).toBe(1);
    expect(cells(grid).slice(0, 2)).toEqual(['pass door@1,1', '6 more@1,2']);
  });

  it('keeps room for an affect and the count in one column', () => {
    // A 247 px pane with room for one row under the slots still draws
    // two, so the count shows beside the first affect.
    const grid = affectsGrid([...TRACKED, ...REST], { width: 247, height: 207 });
    expect(grid.columns).toBe(1);
    expect(grid.pageRows).toBe(2);
    expect(cells(grid).slice(0, 2)).toEqual(['pass door@1,1', '6 more@2,1']);
  });

  it('shows everything on one page before the pane is measured', () => {
    const grid = affectsGrid([...TRACKED, ...REST], null);
    expect(grid.columns).toBe(2);
    expect(grid.pageRows).toBe(4);
    expect(grid.pages).toBe(1);
    expect(grid.rest.every((c) => c.kind === 'affect')).toBe(true);
  });

  it('marks the first cell of each page for the scroll to stop at', () => {
    const many = Array.from({ length: 3 }, (_, i) => row(`a${i}`, 'untracked', i + 3));
    const grid = affectsGrid(many, { width: 494, height: 22 });
    expect(cells(grid)).toEqual(['a0@1,1', '2 more@1,2', 'a1@2,1', 'a2@2,2']);
    expect(grid.rest.map((c) => c.pageStart)).toEqual([true, false, true, false]);
  });
});
