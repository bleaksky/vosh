import { describe, expect, it } from 'vitest';
import { layoutPlain, placeLabel, plainMetrics, type PlainCell } from './mapPlain';

// The village in the approved Main, MainEmber, and MainVellum boards:
// 26 rooms on the 22 px grid, the bank (your room) just north of the
// square. Columns and rows count from the board's top left room.
const ROOMS: Array<[number, number]> = [
  [4, 0],
  [4, 1],
  [5, 1],
  [6, 1],
  [7, 1],
  [0, 2],
  [1, 2],
  [4, 2],
  [1, 3],
  [4, 3],
  [5, 3],
  [1, 4],
  [2, 4],
  [3, 4],
  [4, 4],
  [5, 4],
  [6, 4],
  [7, 4],
  [8, 4],
  [9, 4],
  [5, 5],
  [8, 5],
  [9, 5],
  [4, 6],
  [5, 6],
  [8, 6],
  [5, 7],
];
// Two way links, as [col, row, col, row].
const LINKS: Array<[number, number, number, number]> = [
  [4, 0, 4, 1],
  [4, 1, 4, 2],
  [4, 2, 4, 3],
  [4, 3, 4, 4],
  [4, 1, 5, 1],
  [5, 1, 6, 1],
  [6, 1, 7, 1],
  [1, 3, 1, 4],
  [1, 2, 1, 3],
  [0, 2, 1, 2],
  [1, 4, 2, 4],
  [2, 4, 3, 4],
  [3, 4, 4, 4],
  [4, 4, 5, 4],
  [5, 4, 6, 4],
  [6, 4, 7, 4],
  [7, 4, 8, 4],
  [8, 4, 9, 4],
  [5, 3, 5, 4],
  [5, 4, 5, 5],
  [5, 5, 5, 6],
  [5, 6, 5, 7],
  [4, 6, 5, 6],
  [8, 4, 8, 5],
  [8, 5, 8, 6],
  [9, 4, 9, 5],
];
// Exits that leave the drawing: [col, row, letter].
const OFF: Array<[number, number, string]> = [
  [4, 0, 'N'],
  [1, 4, 'W'],
  [9, 4, 'E'],
  [5, 7, 'S'],
];

function village(): PlainCell[] {
  const exits = new Map<string, string>();
  const add = (c: number, r: number, d: string) =>
    exits.set(`${c},${r}`, (exits.get(`${c},${r}`) ?? '') + d);
  for (const [c1, r1, c2, r2] of LINKS) {
    if (c2 > c1) {
      add(c1, r1, 'e');
      add(c2, r2, 'w');
    } else {
      add(c1, r1, 's');
      add(c2, r2, 'n');
    }
  }
  for (const [c, r, d] of OFF) add(c, r, d);
  // Grid indices start at 1 on the server, so shift by one.
  return ROOMS.map(([c, r]) => ({
    col: c + 1,
    row: r + 1,
    exits: exits.get(`${c},${r}`) ?? '',
    flags: c === 5 && r === 3 ? 'sb' : '',
  }));
}

const measure = (text: string) => text.length * 5.5;
const BANK = { col: 6, row: 4 };

// The board's drawing box is 284 by 242 and its group is moved down
// 16, so every board y below reads 16 more than the board's source.
const board = (x: number, y: number) => ({ x, y: y + 16 });

describe('layoutPlain', () => {
  const scene = layoutPlain({
    cells: village(),
    current: BANK,
    width: 284,
    height: 242,
    zoom: 1,
    measure,
  });

  it('draws the rooms where the approved board does', () => {
    expect(scene.size).toBe(12);
    expect(scene.radius).toBe(3);
    const got = scene.rooms.map((r) => `${r.x},${r.y}`).sort();
    const want = ROOMS.filter(([c, r]) => !(c === 5 && r === 3))
      .map(([c, r]) => board(37 + 22 * c, 22 + 22 * r))
      .map((p) => `${p.x},${p.y}`)
      .sort();
    expect(got).toEqual(want);
  });

  it('fills your room and rings it 3 px outside', () => {
    expect(scene.current).toEqual(board(147, 88));
    expect(scene.ring).toEqual({ ...board(143, 84), w: 20, h: 20, radius: 7 });
  });

  it('runs the lines through the gaps and ends the stubs 6 px out', () => {
    const fmt = (n: number) => String(Math.round(n * 100) / 100);
    const got = scene.lines
      .map((s) => {
        const [x1, x2] = [s.x1, s.x2];
        const [y1, y2] = [s.y1 - 16, s.y2 - 16];
        return x1 === x2
          ? `M${fmt(x1)} ${fmt(y1)}V${fmt(y2)}`
          : `M${fmt(x1)} ${fmt(y1)}H${fmt(x2)}`;
      })
      .sort();
    // The board's path, one segment per entry. The board draws a few
    // lines from the far end, so those are flipped to match.
    const boardPath =
      'M131 21.25V16.75M131 34.75V43.25M131 56.75V65.25M131 78.75V87.25M131 100.75V109.25' +
      'M137.75 50H146.25M159.75 50H168.25M181.75 50H190.25M65 100.75V109.25M65 78.75V87.25' +
      'M49.75 72H58.25M58.25 116H53.75M71.75 116H80.25M93.75 116H102.25M115.75 116H124.25' +
      'M137.75 116H146.25M159.75 116H168.25M181.75 116H190.25M203.75 116H212.25' +
      'M225.75 116H234.25M247.75 116H252.25M153 105.75V109.25M153 122.75V131.25' +
      'M153 144.75V153.25M153 166.75V175.25M153 188.75V193.25M137.75 160H146.25' +
      'M219 122.75V131.25M219 144.75V153.25M241 122.75V131.25';
    const want = boardPath
      .split('M')
      .filter(Boolean)
      .map((seg) => `M${seg}`)
      .sort();
    expect(got).toEqual(want);
    expect(scene.hidden).toEqual([]);
  });

  it('names the bank beside your room', () => {
    expect(scene.labels).toEqual([{ text: 'Bank', ...board(168, 97.5) }]);
  });

  it('moves a label under its room when the spot beside it is taken', () => {
    const cells = village().map((c) => (c.col === 6 && c.row === 5 ? { ...c, flags: 'h' } : c));
    const s = layoutPlain({ cells, current: BANK, width: 284, height: 242, zoom: 1, measure });
    expect(s.labels).toContainEqual({ text: 'Healer', ...board(168, 133) });
  });
});

describe('layoutPlain in a small box', () => {
  it('keeps your room in view and leaves no empty band', () => {
    const scene = layoutPlain({
      cells: village(),
      current: { col: 10, row: 5 },
      width: 120,
      height: 90,
      zoom: 1,
      measure,
    });
    const here = scene.current!;
    expect(here.x).toBeGreaterThanOrEqual(0);
    expect(here.x + scene.size).toBeLessThanOrEqual(120);
    expect(here.y).toBeGreaterThanOrEqual(0);
    expect(here.y + scene.size).toBeLessThanOrEqual(90);
    // Your room is the east edge of the village, so the drawing stops
    // 8 px short of the box's right edge instead of centering on you.
    const right = Math.max(...scene.rooms.map((r) => r.x + scene.size), here.x + scene.size + 5);
    expect(right).toBe(120 - 8);
  });

  it('drops a label that would leave the box', () => {
    const scene = layoutPlain({
      cells: village(),
      current: BANK,
      width: 60,
      height: 60,
      zoom: 1,
      measure,
    });
    for (const l of scene.labels) {
      expect(l.x).toBeGreaterThanOrEqual(2);
      expect(l.x + measure(l.text)).toBeLessThanOrEqual(58);
    }
  });
});

describe('layoutPlain edge cases', () => {
  it('returns an empty scene for an empty push', () => {
    const scene = layoutPlain({
      cells: [],
      current: null,
      width: 100,
      height: 100,
      zoom: 1,
      measure,
    });
    expect(scene.rooms).toEqual([]);
    expect(scene.current).toBeNull();
    expect(scene.labels).toEqual([]);
  });

  it('dashes a secret exit into a room the push leaves out', () => {
    const scene = layoutPlain({
      cells: [{ col: 1, row: 1, exits: '', flags: '', doors: { e: 'hidden' } }],
      current: null,
      width: 100,
      height: 100,
      zoom: 1,
      measure,
    });
    expect(scene.lines).toEqual([]);
    expect(scene.hidden).toHaveLength(1);
    const s = scene.hidden[0];
    expect(s.y1).toBe(s.y2);
    expect(s.x2 - s.x1).toBeCloseTo(6 - 1.5);
  });

  it('draws one line for a two way link', () => {
    const scene = layoutPlain({
      cells: [
        { col: 1, row: 1, exits: 'e', flags: '' },
        { col: 2, row: 1, exits: 'w', flags: '' },
      ],
      current: null,
      width: 100,
      height: 100,
      zoom: 1,
      measure,
    });
    expect(scene.lines).toHaveLength(1);
  });

  it('scales the grid with zoom and keeps room edges on whole pixels', () => {
    for (const zoom of [0.5, 0.75, 1, 1.25, 1.5, 2, 3]) {
      const m = plainMetrics(zoom);
      expect(m.size % 2).toBe(0);
      expect(m.pitch).toBeGreaterThan(m.size);
    }
    expect(plainMetrics(2)).toMatchObject({ pitch: 44, size: 24, ringGap: 6, stub: 12 });
  });
});

describe('placeLabel', () => {
  it('names only the places the server flags', () => {
    expect(placeLabel('b')).toBe('Bank');
    expect(placeLabel('sb')).toBe('Bank');
    expect(placeLabel('$t')).toBe('Trainer');
    expect(placeLabel('h')).toBe('Healer');
    expect(placeLabel('$')).toBe('Shop');
    expect(placeLabel('s')).toBeNull();
    expect(placeLabel('')).toBeNull();
    expect(placeLabel(undefined)).toBeNull();
  });
});
