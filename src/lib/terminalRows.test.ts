import { describe, expect, it } from 'vitest';
import { GameSizeReport, gameSize, keptRows } from './terminalRows';

describe('the rows the live pane keeps', () => {
  it('gives up the rows it lends, and keeps at least one', () => {
    expect(keptRows(20, 0)).toBe(20);
    expect(keptRows(20, 1)).toBe(19);
    expect(keptRows(20, 5)).toBe(15);
    expect(keptRows(3, 5)).toBe(1);
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
