import { describe, expect, it } from 'vitest';
import {
  blockCells,
  blockRun,
  columnWidth,
  dialsFit,
  orbsFit,
  ringsFit,
  vialsFit,
  FOE_LADDER,
  LADDER,
  litSegments,
  rowMarkFit,
  TRACE_POINTS,
  traceSeries,
  tracesHeight,
} from './vitalsDrawnFit';
import type { MeasureText } from './vitalsLedgerFit';

/** A face with tabular digits 0.62 em wide, near the system face. */
const MEASURE: MeasureText = (text, px) => {
  let em = 0;
  for (const ch of text.replace(/[0-9]/g, '0')) {
    em += ch === '0' ? 0.62 : ch === ' ' ? 0.28 : ch === '/' ? 0.32 : ch === '%' ? 0.85 : 0.5;
  }
  return Math.ceil(em * px);
};

// The labels and maxes of a full footer, with the guard at 100 percent.
const LABELS = ['Health', 'Mana', 'Moves'];
const VALUES = ['1038 / 1038', '870 / 870', '521 / 521', '100%'];

describe('Blocks', () => {
  it('counts the whole cells that fit, once both are measured', () => {
    // A 124 px bar in a face whose block is 7.2 px holds 17.
    expect(blockCells(124, 7.2)).toBe(17);
    expect(blockCells(3, 7.2)).toBe(1);
    expect(blockCells(0, 7.2)).toBe(0);
    expect(blockCells(124, 0)).toBe(0);
  });

  it('writes a full block for each whole cell and an eighth for the last', () => {
    // 78.2 percent of 17 is 13.3 cells: 13 blocks and two eighths.
    expect(blockRun(78.2, 17)).toBe('█'.repeat(13) + '▎');
    expect(blockRun(100, 17)).toBe('█'.repeat(17));
    expect(blockRun(0, 17)).toBe('');
  });
});

describe('Traces', () => {
  const sample = (at: number, hp: number) => ({
    at,
    values: { hp, maxhp: 1000, mana: 0, maxmana: 0, move: 50, maxmove: 100 },
  });

  it('spans the last 40 Char.Vitals, each a share of its max', () => {
    const history = Array.from({ length: 60 }, (_, i) => sample(i, i * 10));
    const series = traceSeries(history, 'hp', 59);
    expect(series).toHaveLength(TRACE_POINTS);
    expect(series[0]).toBe(0.2);
    expect(series[TRACE_POINTS - 1]).toBe(0.59);
    expect(traceSeries(history, 'mana', 0)[0]).toBe(0);
    expect(traceSeries(history, 'move', 50)[0]).toBe(0.5);
  });

  it('draws the vital now while the history holds none', () => {
    expect(traceSeries([], 'hp', 72)).toEqual([0.72]);
  });

  it('holds 26 for each row while it waits', () => {
    expect(tracesHeight(12, 3)).toBe(1 + 9 + 78 + 11);
  });
});

describe('the column styles', () => {
  it('split the footer into columns 16 apart, between its sides', () => {
    expect(columnWidth(300, 3)).toBeCloseTo(79.33, 2);
    expect(columnWidth(200, 3)).toBe(46);
  });

  it('keep the Rings labels while each fits whole beside its value', () => {
    expect(ringsFit(300, 12, LABELS, VALUES, MEASURE)).toBe('labels');
    expect(ringsFit(200, 12, LABELS, VALUES, MEASURE)).toBe('keys');
  });

  it('set the Vials text beside each vial while it fits its column', () => {
    const widest = LABELS.map((label, i) => ({
      label,
      figure: { current: ['1038', '870', '521'][i] ?? '', max: '/ 1038' },
    }));
    expect(vialsFit(300, 12, widest, MEASURE)).toBe('full');
    expect(vialsFit(200, 12, widest, MEASURE)).toBe('narrow');
  });

  it('drop the Orbs max, then draw the orbs at 40', () => {
    const widest = [{ figure: { current: '1038', max: '/ 1038' } }];
    expect(orbsFit(300, 12, [...widest, ...widest, ...widest], MEASURE)).toBe('full');
    expect(orbsFit(220, 12, [...widest, ...widest, ...widest], MEASURE)).toBe('bare');
    expect(orbsFit(200, 12, [...widest, ...widest, ...widest], MEASURE)).toBe('narrow');
  });

  it('draw Dials at 60 while a column holds one, else at 44', () => {
    expect(dialsFit(300, 3)).toBe('full');
    expect(dialsFit(200, 3)).toBe('narrow');
    expect(dialsFit(200, 2)).toBe('full');
  });
});

describe('litSegments', () => {
  it('lights each segment once the vital reaches its share', () => {
    expect(litSegments(78, LADDER)).toBe(19);
    expect(litSegments(100, LADDER)).toBe(24);
    expect(litSegments(0, LADDER)).toBe(0);
    expect(litSegments(54, FOE_LADDER)).toBe(26);
  });

  it('lights none for a value the game hides', () => {
    expect(litSegments(null, LADDER)).toBe(0);
  });
});

describe('rowMarkFit', () => {
  it('keeps the mark beside the label and value at 300 pt', () => {
    expect(rowMarkFit(300, 12, LABELS, VALUES, MEASURE)).toBe('beside');
    expect(rowMarkFit(300, 16, LABELS, VALUES, MEASURE)).toBe('beside');
  });

  it('drops the mark under the label and value at 200 pt, as board 5 draws', () => {
    expect(rowMarkFit(200, 12, LABELS, VALUES, MEASURE)).toBe('under');
  });

  it('keeps a mark of 72 at least', () => {
    // Health is 36 wide and 1038 / 1038 71 at 12 px, so the insets, the
    // label, the value and two gaps of 10 leave exactly 72 at 229.
    expect(rowMarkFit(229, 12, LABELS, VALUES, MEASURE)).toBe('beside');
    expect(rowMarkFit(228, 12, LABELS, VALUES, MEASURE)).toBe('under');
  });

  it('fits nothing while it draws your opponent alone', () => {
    expect(rowMarkFit(200, 16, [], ['100%'], MEASURE)).toBe('beside');
  });
});
