import { describe, expect, it } from 'vitest';
import { affectsPaneRows, type AffectInput, type AffectRow } from '../../lib/affectsView';
import {
  COUNTDOWN_ROW_PX,
  countdownGrid,
  countdownMinRows,
  countdownOrder,
  type CountdownGrid,
} from './countdownGrid';

const aff = (name: string, duration: number | null): AffectInput => ({ name, duration });

// Ilsabet on the approved boards: his eight tracked slots, and what the
// game sends while bless has worn off.
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
const rowsOf = (current: AffectInput[]) => affectsPaneRows(current, TRACKED, false);
// The boards' pane, 494 by 219: a 191 px body under the header.
const BOARD_BOX = { width: 494, height: 191 };

/** Each cell as `name@row,column`, or `N more@row,column`. */
function cells(grid: CountdownGrid): string[] {
  return grid.cells.map((c) =>
    c.kind === 'affect'
      ? `${c.row.name}@${c.gridRow},${c.gridColumn}`
      : `${c.count} more@${c.gridRow},${c.gridColumn}`,
  );
}

describe('countdownOrder', () => {
  it('puts what you miss first in your order, then everything by hours left', () => {
    const order = countdownOrder(rowsOf(FOURTEEN)).map((r: AffectRow) => r.name);
    expect(order).toEqual([
      'bless',
      'sanctuary',
      'fly',
      'pass door',
      'haste',
      'bagatelle of bravado',
      'totems canticle',
      'armor',
      'shield',
      'stone skin',
      'levitate',
      'detect invis',
      'the Triumph of One God',
      'mounted',
      'virtues',
    ]);
  });

  it('sorts harmful rows in by their hours, permanent after timed and unknown last', () => {
    const rows = affectsPaneRows(
      [aff('faerie fire', 3), aff('haste', 2), aff('fly', 3), aff('detect magic', null)],
      [{ name: 'fly' }, { name: 'shield' }, { name: 'mounted' }],
      false,
    );
    expect(countdownOrder(rows).map((r) => `${r.name} ${r.state}`)).toEqual([
      'shield missing',
      'mounted missing',
      'haste untracked',
      // At equal hours your tracked affect comes before the rest.
      'fly present',
      'faerie fire harmful',
      'detect magic untracked',
    ]);
  });
});

describe('countdownGrid', () => {
  it('balances a short list across both columns, as board B draws it', () => {
    const grid = countdownGrid(rowsOf(FOURTEEN), BOARD_BOX);
    expect(grid.columns).toBe(2);
    expect(grid.pageRows).toBe(8);
    expect(grid.pages).toBe(1);
    expect(cells(grid).slice(0, 3)).toEqual(['bless@1,1', 'sanctuary@2,1', 'fly@3,1']);
    expect(cells(grid)[8]).toBe('shield@1,2');
    expect(cells(grid)).toHaveLength(15);
  });

  it('fills what fits and counts the rest in the last cell, down the left column first', () => {
    const grid = countdownGrid(rowsOf(TWENTY), BOARD_BOX);
    expect(grid.pageRows).toBe(Math.floor(191 / COUNTDOWN_ROW_PX));
    expect(grid.pages).toBe(2);
    const first = cells(grid).slice(0, 16);
    expect(first[15]).toBe('5 more@8,2');
    // The end of the countdown waits on the next page, never its middle.
    expect(cells(grid).slice(16)).toEqual([
      'detect magic@9,1',
      'detect invis@10,1',
      'the Triumph of One God@11,1',
      'mounted@12,1',
      'virtues@13,1',
    ]);
  });

  it('draws one column under 360 px, and at least two cells a page', () => {
    expect(countdownGrid(rowsOf(FOURTEEN), { width: 359, height: 191 }).columns).toBe(1);
    expect(countdownGrid(rowsOf(FOURTEEN), { width: 360, height: 191 }).columns).toBe(2);
    const tiny = countdownGrid(rowsOf(FOURTEEN), { width: 247, height: 10 });
    expect(tiny.pageRows).toBe(2);
    expect(cells(tiny).slice(0, 2)).toEqual(['bless@1,1', '14 more@2,1']);
  });

  it('holds everything on one page before the body is measured', () => {
    const grid = countdownGrid(rowsOf(TWENTY), null);
    expect(grid.columns).toBe(2);
    expect(grid.pages).toBe(1);
    expect(grid.pageRows).toBe(10);
  });
});

describe('countdownMinRows', () => {
  it('holds down to the last row that asks something of you, and the count after it', () => {
    // Bless, sanctuary, fly, then the count: two rows of two columns.
    expect(countdownMinRows(rowsOf(FOURTEEN), 2)).toBe(2);
    expect(countdownMinRows(rowsOf(FOURTEEN), 1)).toBe(4);
    // Faerie fire at 3 hours sorts in right after fly: four rows and
    // the count.
    expect(countdownMinRows(rowsOf(TWENTY), 2)).toBe(3);
  });

  it('asks for no rows when nothing needs you, and never more than the list', () => {
    const calm = affectsPaneRows([aff('armor', 30), aff('haste', 9)], [{ name: 'armor' }], false);
    expect(countdownMinRows(calm, 2)).toBe(0);
    const missing = affectsPaneRows([], [{ name: 'armor' }], false);
    expect(countdownMinRows(missing, 2)).toBe(1);
  });
});
