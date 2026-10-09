import { describe, expect, it } from 'vitest';
import type { MeasureText } from './vitalsLedgerFit';
import { gaugesFit, marksHeight, pipLights, pipsFit, pipsWidth } from './vitalsMarksFit';

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
const VALUES = ['1020 / 1020', '800 / 800', '930 / 930', '100%'];

describe('gaugesFit', () => {
  it('keeps the pill beside the label and value at 300 and 200 pt', () => {
    expect(gaugesFit(300, 12, LABELS, VALUES, MEASURE)).toBe('beside');
    expect(gaugesFit(200, 12, LABELS, VALUES, MEASURE)).toBe('beside');
    expect(gaugesFit(300, 16, LABELS, VALUES, MEASURE)).toBe('beside');
  });

  it('drops the pill under the label at 200 pt with Panel text 16', () => {
    expect(gaugesFit(200, 16, LABELS, VALUES, MEASURE)).toBe('under');
  });

  it('keeps a pill of 40 at least', () => {
    // Health is 36 wide and 1020 / 1020 71 at 12 px, so the insets, the
    // label, the value and two gaps of 10 leave exactly 40 at 197.
    expect(gaugesFit(197, 12, LABELS, VALUES, MEASURE)).toBe('beside');
    expect(gaugesFit(196, 12, LABELS, VALUES, MEASURE)).toBe('under');
  });

  it('fits nothing while it draws your opponent alone', () => {
    expect(gaugesFit(200, 16, [], ['100%'], MEASURE)).toBe('beside');
  });
});

describe('pipsFit', () => {
  it('draws ten discs at 300 pt, at 12 and at 16 px', () => {
    expect(pipsFit(300, 12, LABELS, VALUES, MEASURE)).toBe('ten');
    expect(pipsFit(300, 16, LABELS, VALUES, MEASURE)).toBe('ten');
  });

  it('draws five discs at 200 pt', () => {
    expect(pipsFit(200, 12, LABELS, VALUES, MEASURE)).toBe('five');
  });

  it('drops the discs under the label at 200 pt with Panel text 16', () => {
    expect(pipsFit(200, 16, LABELS, VALUES, MEASURE)).toBe('under');
  });

  it('measures ten and five discs 3 apart', () => {
    expect(pipsWidth(10)).toBe(87);
    expect(pipsWidth(5)).toBe(42);
  });
});

describe('pipLights', () => {
  const draw = (pct: number | null, count: number) =>
    pipLights(pct, count)
      .map((light) => (light === 'full' ? 'F' : light === 'half' ? 'H' : '.'))
      .join('');

  it('lights board 1 in halves, as the frames draw them', () => {
    expect(draw(75, 10)).toBe('FFFFFFFH..');
    expect(draw(75, 5)).toBe('FFFF.');
    expect(draw((159 / 1020) * 100, 10)).toBe('FH........');
    expect(draw((159 / 1020) * 100, 5)).toBe('F....');
    expect(draw((310 / 800) * 100, 10)).toBe('FFFF......');
    expect(draw((489 / 930) * 100, 10)).toBe('FFFFFH....');
    expect(draw((489 / 930) * 100, 5)).toBe('FFH..');
    expect(draw(100, 10)).toBe('FFFFFFFFFF');
  });

  it('lights none for a value the game hides', () => {
    expect(draw(null, 10)).toBe('..........');
    expect(draw(0, 5)).toBe('.....');
  });
});

describe('marksHeight', () => {
  it('holds a 22 px row for each vital between the pads', () => {
    expect(marksHeight(12, 3)).toBe(1 + 9 + 66 + 11);
    expect(marksHeight(16, 2)).toBe(1 + 12 + 58 + 15);
  });
});
