import { describe, expect, it } from 'vitest';
import {
  GameSizeReport,
  gameSize,
  keepTail,
  keptRows,
  nativeBottomBounds,
  spareAbove,
  type TailView,
} from './terminalRows';

// While your prompt shows pinned the grid keeps to the bottom of its
// pane, so the pixels its whole rows leave over sit above its first row
// and never between your newest line and the band.
describe('the grid at the bottom of its pane', () => {
  it('moves xterm down by the pixels its rows leave over, on whole device pixels', () => {
    // 753 px at 21.5 a row is 35 rows and half a pixel over.
    expect(spareAbove(753, 35, 21.5, 2)).toBe(0.5);
    // 752.75 px fits only 34 rows, so most of a row is over.
    expect(spareAbove(752.75, 34, 21.5, 2)).toBe(21.5);
    expect(spareAbove(752.6, 34, 21.5, 2)).toBe(21.5);
    expect(spareAbove(752.6, 34, 21.5, 1)).toBe(21);
    // A grid that fits exactly moves nowhere, and one taller never up.
    expect(spareAbove(700, 40, 17.5, 2)).toBe(0);
    expect(spareAbove(690, 40, 17.5, 2)).toBe(0);
  });

  it('moves the native grid down by what its device rows leave over', () => {
    // 753 CSS px at 2x is 1506 device px, 35 rows of 43 and 1 px over.
    const b = nativeBottomBounds(38, 753, 2, 43);
    expect(b).toEqual({ top: 38.5, height: 752.5, spare: 0.5 });
    // The grid fits the same rows, and its bottom stays the pane's.
    const px = (v: number) => Math.round(v * 2);
    expect(Math.floor(px(b.height) / 43)).toBe(35);
    expect(px(b.top) + px(b.height)).toBe(px(38) + px(753));
    // Most of a row over, at 1x and at 2x.
    expect(nativeBottomBounds(10, 752, 2, 43).spare).toBe(21);
    expect(nativeBottomBounds(10, 700, 1, 21).spare).toBe(7);
    expect(nativeBottomBounds(10, 700, 1, 0).spare).toBe(0);
  });
});

describe('the rows the live pane keeps', () => {
  it('gives up the rows it lends, and keeps at least one', () => {
    expect(keptRows(20, 0)).toBe(20);
    expect(keptRows(20, 1)).toBe(19);
    expect(keptRows(20, 5)).toBe(15);
    expect(keptRows(3, 5)).toBe(1);
  });
});

// The live pane follows its newest rows. In a window xterm takes a row
// resize into its scrollbar only on the next frame, and a scroll asked of
// it before then is measured on the old rows, a row short at 2x. A pane
// made with `lagging` set does what that scrollbar does.
function pane(viewportY: number, baseY: number, lagging: boolean): TailView & { asked: number } {
  const view = {
    asked: 0,
    buffer: { active: { viewportY, baseY } },
    scrollToBottom() {
      view.asked += 1;
      const b = view.buffer.active;
      b.viewportY = lagging ? b.baseY - 1 : b.baseY;
    },
  };
  return view;
}

describe('the newest rows of the live pane', () => {
  it('asks nothing of xterm while the pane shows them', () => {
    // A fight just took a row: xterm moved the screen with the text, and
    // its scrollbar has not caught up yet.
    const view = pane(77, 77, true);
    keepTail(view);
    expect(view.asked).toBe(0);
    expect(view.buffer.active.viewportY).toBe(77);
  });

  it('brings a pane that left them back', () => {
    const view = pane(70, 77, false);
    keepTail(view);
    expect(view.asked).toBe(1);
    expect(view.buffer.active.viewportY).toBe(77);
  });
});

// The window size the game hears of through NAWS. The pinned band borrows
// a row from the terminal in a fight and gives it back after, and the game
// never hears of that row: it keeps the rows the terminal holds with a one
// row band, so what it wraps and pages stays the same through a fight.
describe('the size the game hears of', () => {
  it('counts the rows the band borrows as the terminal rows they are', () => {
    expect(gameSize(120, 40, 0)).toEqual({ cols: 120, rows: 40 });
    expect(gameSize(120, 39, 1)).toEqual({ cols: 120, rows: 40 });
    expect(gameSize(120, 38, 2)).toEqual({ cols: 120, rows: 40 });
  });

  it('sends nothing when a fight starts and ends, however quickly', () => {
    const report = new GameSizeReport();
    expect(report.next(120, 40, 0)).toEqual({ cols: 120, rows: 40 });
    // Four fights in a second: the terminal loses and takes back a row
    // each time, and the game hears nothing.
    for (let i = 0; i < 4; i++) {
      expect(report.next(120, 39, 1)).toBeNull();
      expect(report.next(120, 40, 0)).toBeNull();
    }
  });

  it('still sends a new size when the window changes, in a fight or out', () => {
    const report = new GameSizeReport();
    report.next(120, 40, 0);
    expect(report.next(120, 39, 1)).toBeNull();
    // A taller window in the fight.
    expect(report.next(120, 41, 1)).toEqual({ cols: 120, rows: 42 });
    // The fight ends in the taller window.
    expect(report.next(120, 42, 0)).toBeNull();
    // A narrower window wraps differently, so the game hears of it.
    expect(report.next(100, 42, 0)).toEqual({ cols: 100, rows: 42 });
    expect(report.next(100, 41, 1)).toBeNull();
  });
});
