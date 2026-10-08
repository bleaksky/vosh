import { describe, expect, it } from 'vitest';
import {
  ledgerFigure,
  ledgerFigurePx,
  ledgerFit,
  ledgerHeight,
  type MeasureText,
} from './vitalsLedgerFit';

/** A face with tabular digits 0.62 em wide, near the system face at
 *  weight 500, so the steps land at the widths below. */
const MEASURE: MeasureText = (text, px) => {
  let em = 0;
  for (const ch of text.replace(/[0-9]/g, '0')) {
    em += ch === '0' ? 0.62 : ch === ' ' ? 0.28 : ch === '/' ? 0.32 : ch === '%' ? 0.85 : 0.58;
  }
  return Math.ceil(em * px);
};

// The board's maxes: Health 1020, Mana 800, Moves 930.
const WIDEST = [
  ledgerFigure('current-max', 1020, 1020, false),
  ledgerFigure('current-max', 800, 800, false),
  ledgerFigure('current-max', 930, 930, false),
];

describe('ledgerFigure', () => {
  it('sets the max beside the number only for Current and max', () => {
    expect(ledgerFigure('current-max', 765, 1020, false)).toEqual({
      current: '765',
      max: '/ 1020',
    });
    expect(ledgerFigure('current', 765, 1020, false)).toEqual({ current: '765', max: null });
    expect(ledgerFigure('percent', 765, 1020, false)).toEqual({ current: '75%', max: null });
  });

  it('reads ? for a vital the game hides, in its Values form', () => {
    expect(ledgerFigure('current-max', 0, 0, true)).toEqual({ current: '?', max: '/ ?' });
    expect(ledgerFigure('current', 0, 0, true)).toEqual({ current: '?', max: null });
    expect(ledgerFigure('percent', 0, 0, true)).toEqual({ current: '?%', max: null });
  });
});

describe('ledgerFit', () => {
  it('keeps the max at 300 pt', () => {
    expect(ledgerFit(300, 12, WIDEST, MEASURE)).toBe('full');
  });

  it('drops the max at 200 pt and keeps the 16 px figure', () => {
    expect(ledgerFit(200, 12, WIDEST, MEASURE)).toBe('bare');
  });

  it('drops the max at 300 pt with Panel text 16', () => {
    expect(ledgerFit(300, 16, WIDEST, MEASURE)).toBe('bare');
  });

  it('sets the figures at the text size at 200 pt with Panel text 16', () => {
    expect(ledgerFit(200, 16, WIDEST, MEASURE)).toBe('text');
  });

  it('steps the figure to 14 px before the text size', () => {
    // 1020 at 16 px is 40 wide, at 14 px 35.
    expect(ledgerFit(30 + 37 * 3 + 32, 12, WIDEST, MEASURE)).toBe('smaller');
  });

  it('measures each column at its max, so a fight never moves it', () => {
    const percent = WIDEST.map((f) => ({ ...f, max: null }));
    expect(ledgerFit(200, 12, percent, MEASURE)).toBe('full');
  });

  it('gives each column more room as vitals go off', () => {
    expect(ledgerFit(200, 12, WIDEST.slice(0, 2), MEASURE)).toBe('full');
  });
});

describe('ledgerFigurePx', () => {
  it('scales 16 and 14 with your panel size and steps to the text size', () => {
    expect(ledgerFigurePx('full', 12)).toBe(16);
    expect(ledgerFigurePx('bare', 16)).toBe(21);
    expect(ledgerFigurePx('smaller', 12)).toBe(14);
    expect(ledgerFigurePx('smaller', 16)).toBe(19);
    expect(ledgerFigurePx('text', 16)).toBe(16);
  });
});

describe('ledgerHeight', () => {
  it('holds the columns with their line, and without it under None', () => {
    // 1 + 10 + 12 + 3 + 20 + 4 + 2 + 12
    expect(ledgerHeight(12, 2)).toBe(64);
    expect(ledgerHeight(12, 0)).toBe(58);
    expect(ledgerHeight(16, 4)).toBe(1 + 13 + 16 + 4 + 27 + 5 + 4 + 16);
  });
});
