import { describe, expect, it } from 'vitest';
import {
  BOX_ROWS_MAX,
  BOX_ROWS_MIN,
  CARD_MARGIN,
  KEEPS_ITS_PRESS,
  boxRowsFor,
  clampPlace,
  dragRows,
  fitRows,
  fitsMoved,
  movedFit,
  savedPlace,
  startsMove,
} from './cardPlace';

const view = { w: 1280, h: 800 };
const card = { w: 760, h: 420 };

describe('savedPlace', () => {
  it('keeps the place only once both edges are saved', () => {
    expect(savedPlace(null, null)).toBeNull();
    expect(savedPlace(140, null)).toBeNull();
    expect(savedPlace(null, 96)).toBeNull();
    expect(savedPlace(140, 96)).toEqual({ left: 140, top: 96 });
  });
});

describe('clampPlace', () => {
  it('leaves a place that is whole on screen as it is', () => {
    expect(clampPlace({ left: 140, top: 96 }, card, view)).toEqual({ left: 140, top: 96 });
  });

  it('pulls a card dragged past an edge back inside the margin', () => {
    expect(clampPlace({ left: -300, top: -50 }, card, view)).toEqual({
      left: CARD_MARGIN,
      top: CARD_MARGIN,
    });
    expect(clampPlace({ left: 900, top: 700 }, card, view)).toEqual({
      left: 1280 - 760 - CARD_MARGIN,
      top: 800 - 420 - CARD_MARGIN,
    });
  });

  it('follows a window that shrinks and keeps the saved place for one that grows back', () => {
    const saved = { left: 480, top: 300 };
    const small = clampPlace(saved, card, { w: 1000, h: 600 });
    expect(small).toEqual({ left: 1000 - 760 - CARD_MARGIN, top: 600 - 420 - CARD_MARGIN });
    expect(clampPlace(saved, card, view)).toEqual(saved);
  });

  it('keeps the top left corner at the margin for a card bigger than the window', () => {
    expect(clampPlace({ left: 200, top: 200 }, { w: 1400, h: 900 }, view)).toEqual({
      left: CARD_MARGIN,
      top: CARD_MARGIN,
    });
  });

  it('lands on whole pixels', () => {
    expect(clampPlace({ left: 140.6, top: 96.2 }, card, view)).toEqual({ left: 141, top: 96 });
  });
});

describe('fitsMoved', () => {
  it('moves a card the window holds at its own width, and spans a narrow one', () => {
    expect(fitsMoved(760, 1280)).toBe(true);
    expect(fitsMoved(760, 784)).toBe(true);
    expect(fitsMoved(760, 783)).toBe(false);
  });
});

describe('rows', () => {
  it('counts the whole rows that fit beside the chrome', () => {
    expect(fitRows(400, 148, 17)).toBe(14);
    expect(fitRows(100, 148, 17)).toBeLessThan(0);
    expect(movedFit(800, 148, 17)).toBe(Math.floor((800 - 2 * CARD_MARGIN - 148) / 17));
  });

  it('grows with the text from six rows up to what fits while you set none', () => {
    expect(boxRowsFor(null, 0, 30)).toBe(BOX_ROWS_MIN);
    expect(boxRowsFor(null, 12, 30)).toBe(12);
    expect(boxRowsFor(null, 80, 30)).toBe(30);
  });

  it('holds the rows you set, within six and what fits', () => {
    expect(boxRowsFor(14, 0, 30)).toBe(14);
    expect(boxRowsFor(14, 80, 30)).toBe(14);
    expect(boxRowsFor(40, 0, 30)).toBe(30);
    expect(boxRowsFor(14, 0, 3)).toBe(BOX_ROWS_MIN);
  });

  it('turns grip travel into rows, held to six and to what the window allows', () => {
    expect(dragRows(10, 34, 17, 30)).toBe(12);
    expect(dragRows(10, -34, 17, 30)).toBe(8);
    expect(dragRows(10, -400, 17, 30)).toBe(BOX_ROWS_MIN);
    expect(dragRows(10, 4000, 17, 30)).toBe(30);
    expect(dragRows(10, 100_000, 17, 10_000)).toBe(BOX_ROWS_MAX);
    // The grip on the top edge grows as you pull up.
    expect(dragRows(10, -34, 17, 30, -1)).toBe(12);
    expect(dragRows(10, 8, 17, 30)).toBe(10);
  });
});

describe('startsMove', () => {
  // A stand in for an element: it sits inside the elements `within`
  // names, as closest reads them.
  const at = (...within: string[]) =>
    ({
      closest: (selector: string) =>
        within.some((tag) => selector.split(', ').includes(tag)) ? {} : null,
    }) as unknown as EventTarget;

  it('moves from the header itself and never from its buttons or menus', () => {
    expect(KEEPS_ITS_PRESS.split(', ')).toContain('button');
    expect(startsMove(at())).toBe(true);
    expect(startsMove(at('button'))).toBe(false);
    expect(startsMove(at('[role="radiogroup"]'))).toBe(false);
    expect(startsMove(at('[role="menu"]'))).toBe(false);
    expect(startsMove(null)).toBe(false);
  });
});
