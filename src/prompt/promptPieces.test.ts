import { describe, expect, it } from 'vitest';
import {
  breakHint,
  BY_VALUE_HINT,
  byValueName,
  caretAfter,
  colorHex,
  colorHint,
  customColor,
  customText,
  deleteOp,
  dockMapper,
  endPlace,
  insertOps,
  insertPlace,
  layoutMarks,
  MORE_STYLES,
  MORE_UNDERLINES,
  moreLabel,
  moveBack,
  moveOp,
  pickAnnouncement,
  pickable,
  rawLayout,
  rawMarks,
  rowsOf,
  step,
  STEPS_HINT,
  swatchOf,
  textMapper,
  THEME_HINT,
  UNDERLINE_KINDS,
  type Grid,
} from './promptPieces';
import type { PromptPiece, PromptPieceKind } from '../ipc/promptDesign';

/** A design as pieces: `[`, the hp value, `]` with a trailing space,
 *  a condition, a line break, its end, and codes. */
function piece(index: number, kind: PromptPieceKind, literal: string | null = null): PromptPiece {
  const shows = kind !== 'codes' && kind !== 'if' && kind !== 'end' && kind !== 'nl';
  return {
    piece: index,
    kind,
    text: '',
    field: kind === 'value' ? 'hp' : null,
    label: '',
    format: kind === 'value' ? 'value' : null,
    width: null,
    when: 'always',
    when_fixed: false,
    color: { kind: 'default' },
    background: { kind: 'default' },
    bold: false,
    dim: false,
    italic: false,
    underline: false,
    underline_style: null,
    underline_color: { kind: 'default' },
    inverse: false,
    strike: false,
    blink: false,
    literal,
    meta: null,
    forms: [],
    by_value: false,
    shows,
  };
}

const DESIGN: PromptPiece[] = [
  piece(0, 'text', '['),
  piece(1, 'value'),
  piece(2, 'text', '] '),
  piece(3, 'if'),
  piece(4, 'nl'),
  piece(5, 'end'),
  piece(6, 'value'),
  piece(7, 'codes'),
];

describe('picking a part', () => {
  it('picks parts that take cells and line breaks', () => {
    expect(pickable(DESIGN)).toEqual([0, 1, 2, 4, 6]);
  });

  it('picks a push to the right edge as it picks a line break', () => {
    const pushed = [piece(0, 'value'), piece(1, 'right'), piece(2, 'value'), piece(3, 'codes')];
    expect(pickable(pushed)).toEqual([0, 1, 2]);
    expect(step(pushed, { picked: 0, caret: null }, 1)).toEqual({ picked: 1, caret: null });
  });

  it('steps with Left and Right and stops at the ends', () => {
    expect(step(DESIGN, { picked: 1, caret: null }, 1)).toEqual({ picked: 2, caret: null });
    expect(step(DESIGN, { picked: 2, caret: null }, 1)).toEqual({ picked: 4, caret: null });
    expect(step(DESIGN, { picked: 6, caret: null }, 1)).toEqual({ picked: 6, caret: null });
    expect(step(DESIGN, { picked: 0, caret: null }, -1)).toEqual({ picked: 0, caret: null });
    // Nothing picked: Right takes the first, Left the last.
    expect(step(DESIGN, { picked: null, caret: null }, 1).picked).toBe(0);
    expect(step(DESIGN, { picked: null, caret: null }, -1).picked).toBe(6);
    // From the caret: the part after it, or before it.
    expect(step(DESIGN, { picked: null, caret: 2 }, 1).picked).toBe(2);
    expect(step(DESIGN, { picked: null, caret: 2 }, -1).picked).toBe(1);
    expect(step(DESIGN, { picked: null, caret: 8 }, -1).picked).toBe(6);
  });

  it('moves the picked part past its neighbor with Option', () => {
    expect(moveOp(DESIGN, 1, -1)).toEqual({ op: 'move', piece: 1, to: 0 });
    expect(moveOp(DESIGN, 1, 1)).toEqual({ op: 'move', piece: 1, to: 3 });
    expect(moveOp(DESIGN, 0, -1)).toBeNull();
    expect(moveOp(DESIGN, 6, 1)).toBeNull();
    expect(moveOp(DESIGN, null, 1)).toBeNull();
  });

  it('takes a move back exactly with the opposite key', () => {
    // Colored by how full with the max shown as a bar: Option with Right
    // moves the bar past hp, and the slash and hp run together into one
    // part. Option with Left puts the design back as it was, slash and
    // all, rather than moving the bar past the whole run.
    const before = '[%c_hp%hp%c_default/%{maxhp:bar:10}hp ';
    const after = '[%c_hp%hp%c_default/hp %{maxhp:bar:10}';
    const right = { before, after, from: 3, landed: 3, dir: 1 as const, mirror: false };
    expect(moveBack([right], after, 3, -1)).toEqual(right);
    // The same way again is a move of its own.
    expect(moveBack([right], after, 3, 1)).toBeNull();
    // Once the design changed, or another part is picked, the key moves.
    expect(moveBack([right], `${after}x`, 3, -1)).toBeNull();
    expect(moveBack([right], after, 1, -1)).toBeNull();
    expect(moveBack([], after, 3, -1)).toBeNull();
    // Two moves come back newest first.
    const again = {
      before: after,
      after: 'later',
      from: 3,
      landed: 4,
      dir: 1 as const,
      mirror: false,
    };
    expect(moveBack([right, again], 'later', 4, -1)).toEqual(again);
    expect(moveBack([right], after, 3, -1)).toEqual(right);
  });
});

describe('adding and taking away', () => {
  it('adds at the caret, after the picked part, or at the end before its codes', () => {
    expect(insertPlace(DESIGN, { picked: null, caret: 3 })).toBe(3);
    expect(insertPlace(DESIGN, { picked: 1, caret: null })).toBe(2);
    // The design ends on codes, which stay at its end.
    expect(endPlace(DESIGN)).toBe(7);
    expect(insertPlace(DESIGN, { picked: null, caret: null })).toBe(7);
    expect(endPlace([])).toBe(0);
  });

  it('keeps a space after a value added at the end of a design that ends in one', () => {
    // His design ends `] %c_reset`, so the place is before the reset.
    const design = [
      piece(0, 'text', '['),
      piece(1, 'value'),
      piece(2, 'text', '] '),
      piece(3, 'codes'),
    ];
    expect(insertOps(design, 3, 'hp', { format: 'bar' })).toEqual([
      { op: 'insert_field', at: 3, field: 'hp', format: { format: 'bar' } },
      { op: 'insert_text', at: 4, text: ' ' },
    ]);
    expect(insertOps(design, 1, 'gold')).toEqual([{ op: 'insert_field', at: 1, field: 'gold' }]);
    const tight = [piece(0, 'text', '['), piece(1, 'value'), piece(2, 'codes')];
    expect(insertOps(tight, 2, 'gold')).toHaveLength(1);
  });

  it('takes a picked part, or a character of text next to the caret', () => {
    expect(deleteOp(DESIGN, { picked: 1, caret: null }, -1)).toEqual({ op: 'remove', piece: 1 });
    expect(deleteOp(DESIGN, { picked: null, caret: 3 }, -1)).toEqual({
      op: 'set_text',
      piece: 2,
      text: ']',
    });
    expect(deleteOp(DESIGN, { picked: null, caret: 2 }, 1)).toEqual({
      op: 'set_text',
      piece: 2,
      text: ' ',
    });
    expect(deleteOp(DESIGN, { picked: null, caret: 1 }, -1)).toEqual({ op: 'remove', piece: 0 });
    expect(deleteOp(DESIGN, { picked: null, caret: 0 }, -1)).toBeNull();
    expect(deleteOp(DESIGN, { picked: null, caret: null }, -1)).toBeNull();
  });

  it('puts the caret past what an edit added, or where a part went', () => {
    expect(caretAfter({ op: 'insert_text', at: 2, text: 'x' }, 2)).toBe(3);
    expect(caretAfter({ op: 'remove', piece: 4 }, null)).toBe(4);
    expect(caretAfter({ op: 'set_text', piece: 2, text: '' }, null)).toBeNull();
  });
});

describe('the rows of a part', () => {
  it('checks the swatch a color names', () => {
    expect(swatchOf({ kind: 'default' })).toBe('default');
    expect(swatchOf({ kind: 'by_value' })).toBe('by_value');
    expect(swatchOf({ kind: 'by_value', field: 'mana' })).toBe('custom');
    expect(swatchOf({ kind: 'by_value', steps: true })).toBe('custom');
    expect(swatchOf({ kind: 'named', index: 4 })).toBe(4);
    expect(swatchOf({ kind: 'named', index: 9 })).toBe('custom');
    expect(swatchOf({ kind: 'rgb', r: 128, g: 200, b: 255 })).toBe('custom');
  });

  it('shows a custom color as hex and reads one back', () => {
    const palette = (i: number) => (i === 42 ? '#00d787' : '#000000');
    expect(customText({ kind: 'rgb', r: 128, g: 200, b: 255 }, palette)).toBe('#80c8ff');
    expect(customText({ kind: 'index', index: 42 }, palette)).toBe('#00d787');
    expect(customText({ kind: 'named', index: 2 }, palette)).toBe('');
    expect(customColor('#80C8FF')).toEqual({ kind: 'rgb', r: 128, g: 200, b: 255 });
    expect(customColor('blue')).toBeNull();
  });

  it('says what By value does while it is on', () => {
    expect(colorHint({ kind: 'by_value' })).toBe(BY_VALUE_HINT);
    expect(colorHint({ kind: 'default' })).toBe(THEME_HINT);
    // A ground by value, and a color by another value, follow the same
    // rule, so the hint says it too. The game's own colors follow the
    // game's.
    expect(colorHint({ kind: 'default' }, { kind: 'by_value' })).toBe(BY_VALUE_HINT);
    expect(colorHint({ kind: 'by_value', field: 'mana' })).toBe(BY_VALUE_HINT);
    expect(colorHint({ kind: 'default' }, { kind: 'by_value', field: 'mana' })).toBe(BY_VALUE_HINT);
    expect(colorHint({ kind: 'by_value', game: true })).toBe(THEME_HINT);
    // The steps keep their own colors, so they say their own rule.
    expect(colorHint({ kind: 'by_value', field: 'hp', steps: true })).toBe(STEPS_HINT);
    expect(colorHint({ kind: 'default' }, { kind: 'by_value', steps: true })).toBe(STEPS_HINT);
    expect(breakHint('fight')).toBe(
      'The line above shows only in a fight, so out of a fight your prompt is one line.',
    );
    expect(breakHint('always')).toBeNull();
    for (const text of [THEME_HINT, BY_VALUE_HINT, STEPS_HINT, breakHint('not_fight') ?? '']) {
      expect(text).not.toMatch(/[;:–—]| - /);
    }
  });

  it('gives a value Show as, a bar Width, and a line break only When', () => {
    const value = { ...piece(1, 'value'), forms: [{} as never] };
    expect(rowsOf(value)).toEqual({
      showAs: true,
      text: false,
      width: false,
      color: true,
      background: true,
      style: true,
      underline: false,
    });
    expect(rowsOf({ ...value, format: 'bar' })).toMatchObject({
      width: true,
      background: true,
      style: false,
    });
    expect(rowsOf(piece(0, 'text', '['))).toMatchObject({ text: true, showAs: false });
    for (const kind of ['nl', 'right'] as const) {
      expect(rowsOf(piece(4, kind))).toEqual({
        showAs: false,
        text: false,
        width: false,
        color: false,
        background: false,
        style: false,
        underline: false,
      });
    }
  });

  it('adds the Underline row while an underline is on, and not to a bar', () => {
    const value = { ...piece(1, 'value'), forms: [{} as never] };
    expect(rowsOf({ ...value, underline: true })).toMatchObject({ style: true, underline: true });
    expect(rowsOf({ ...value, underline: true, format: 'bar' })).toMatchObject({
      style: false,
      underline: false,
    });
  });

  it('offers the five kinds of underline, single first, in the board order', () => {
    expect(UNDERLINE_KINDS.map((k) => [k.style, k.label, k.line])).toEqual([
      ['underline', 'Single', 'solid'],
      ['double', 'Double', 'double'],
      ['curly', 'Curly', 'wavy'],
      ['dotted', 'Dotted', 'dotted'],
      ['dashed', 'Dashed', 'dashed'],
    ]);
  });

  it('names More styles by the styles and the underline kind it has on', () => {
    const off = { strike: false, dim: false, inverse: false, blink: false, underline_style: null };
    expect(MORE_STYLES.map((s) => [s.style, s.label])).toEqual([
      ['strike', 'Strikethrough'],
      ['dim', 'Dim'],
      ['inverse', 'Reverse'],
      ['blink', 'Blink'],
    ]);
    expect(MORE_UNDERLINES.map((k) => [k.style, k.label])).toEqual([
      ['double', 'Double underline'],
      ['curly', 'Curly underline'],
      ['dotted', 'Dotted underline'],
      ['dashed', 'Dashed underline'],
    ]);
    expect(moreLabel(off)).toBe('More styles');
    expect(moreLabel({ ...off, dim: true })).toBe('Dim');
    expect(moreLabel({ ...off, blink: true })).toBe('Blink');
    expect(moreLabel({ ...off, strike: true, inverse: true })).toBe('Strikethrough, reverse');
    expect(moreLabel({ ...off, strike: true, dim: true, inverse: true, blink: true })).toBe(
      'Strikethrough, dim, reverse, blink',
    );
    // The single line is U's. A kind past it reads after the styles.
    expect(moreLabel({ ...off, underline_style: 'underline' })).toBe('More styles');
    expect(moreLabel({ ...off, dim: true, underline_style: 'curly' })).toBe('Dim, curly underline');
  });

  it('names a color by value that a field shows, since no hex can', () => {
    expect(byValueName({ kind: 'by_value' })).toBe('By value');
    expect(byValueName({ kind: 'by_value', field: 'mana' })).toBe('By mana');
    expect(byValueName({ kind: 'by_value', game: true })).toBe('By game');
    expect(byValueName({ kind: 'by_value', field: 'hp', game: true })).toBe('By game');
    expect(byValueName({ kind: 'by_value', steps: true })).toBe('By steps');
    expect(byValueName({ kind: 'by_value', field: 'hp', steps: true })).toBe('By hp in steps');
    expect(byValueName({ kind: 'default' })).toBeNull();
    expect(byValueName({ kind: 'rgb', r: 1, g: 2, b: 3 })).toBeNull();
  });

  it('shows the underline color as hex, theme colors too, and empty for the text color', () => {
    const palette = (i: number) => (i === 1 ? '#bf616a' : '#00d787');
    expect(colorHex({ kind: 'default' }, palette)).toBe('');
    expect(colorHex({ kind: 'named', index: 1 }, palette)).toBe('#bf616a');
    expect(colorHex({ kind: 'index', index: 42 }, palette)).toBe('#00d787');
    expect(colorHex({ kind: 'rgb', r: 191, g: 97, b: 106 }, palette)).toBe('#bf616a');
    expect(colorHex({ kind: 'by_value' }, palette)).toBe('');
  });
});

describe('where the marks sit', () => {
  // The terminal grid: text at x 16, rows 17.5 apart, cells 7.8 wide.
  const grid: Grid = { left: 16, top: 38, cellW: 7.8, cellH: 17.5 };

  it('rings the picked part on the open row (P5)', () => {
    // [1020(100%)h on the row at y 703, the hp value at cells 1 to 4.
    const region = { gen: 7, row: 38, col: 0, cols: 80, atBottom: true };
    const mapper = textMapper('[1020(100%)h', region, grid);
    const marks = layoutMarks({
      spans: [
        { piece: 0, row: 0, col: 0, width: 1 },
        { piece: 1, row: 0, col: 1, width: 4 },
      ],
      lineBreaks: new Set(),
      mapper,
      grid,
      pointing: { picked: 1, caret: null },
      warn: new Set([0]),
    });
    expect(marks.picked).toEqual([{ left: 16 + 7.8, top: 703, width: 4 * 7.8, height: 17.5 }]);
    expect(marks.warn).toEqual([{ left: 16, top: 703, width: 7.8, height: 17.5 }]);
    expect(marks.caret).toBeNull();
  });

  it('puts the caret past the trailing space (P6)', () => {
    const region = { gen: 7, row: 38, col: 0, cols: 80, atBottom: true };
    const text = '[1020(100%)h 800(100%)m 930(100%)v] ';
    const mapper = textMapper(text, region, grid);
    const marks = layoutMarks({
      spans: [
        { piece: 0, row: 0, col: 0, width: 34 },
        { piece: 1, row: 0, col: 34, width: 2 },
      ],
      lineBreaks: new Set(),
      mapper,
      grid,
      pointing: { picked: null, caret: 3 },
      warn: new Set(),
    });
    expect(marks.caret?.left).toBeCloseTo(16 + 36 * 7.8);
    expect(marks.caret?.top).toBeCloseTo(703.25);
    expect(marks.caret).toMatchObject({ width: 2, height: 17 });
  });

  it('draws a ↵ after a row a line break ends, picked or not (P10)', () => {
    const region = { gen: 7, row: 37, col: 0, cols: 80, atBottom: true };
    const plain = 'Blackwatch Guard\n1020/1020hp';
    const mapper = textMapper(plain, region, grid);
    const spans = [
      { piece: 1, row: 0, col: 0, width: 16 },
      { piece: 2, row: 0, col: 16, width: 0 },
      { piece: 4, row: 1, col: 0, width: 11 },
    ];
    const marks = layoutMarks({
      spans,
      lineBreaks: new Set([2]),
      mapper,
      grid,
      pointing: { picked: 2, caret: null },
      warn: new Set(),
    });
    expect(marks.returns).toEqual([
      {
        piece: 2,
        box: { left: 16 + 16 * 7.8, top: 685.5, width: 15.6, height: 17.5 },
        picked: true,
      },
    ]);
    // A picked break draws its ring on the ↵ alone.
    expect(marks.picked).toEqual([]);
    // The caret after a break waits at the start of the next row.
    const after = layoutMarks({
      spans,
      lineBreaks: new Set([2]),
      mapper,
      grid,
      pointing: { picked: null, caret: 3 },
      warn: new Set(),
    });
    expect(after.caret?.left).toBe(16);
    expect(after.caret?.top).toBeCloseTo(703.25);
  });

  it('follows a row the terminal wraps', () => {
    // Ten cells wide, the value starting at cell 8 wraps onto the next
    // screen row as the terminal moves it.
    const region = { gen: 7, row: 10, col: 0, cols: 10, atBottom: true };
    const mapper = textMapper('ab cd efgh 1020', region, grid);
    const boxes = mapper.boxes(0, 11, 4);
    expect(boxes).toEqual([{ left: 16, top: 38 + 11 * 17.5, width: 4 * 7.8, height: 17.5 }]);
  });

  it('places marks on the pinned band by column', () => {
    const mapper = dockMapper({
      left: 16,
      rowsTop: 700,
      first: 1,
      shown: 1,
      cellW: 7.8,
      cellH: 17.5,
      rows: ['Tester: ██', '[1020hp]'],
    });
    expect(mapper.boxes(1, 1, 4)).toEqual([
      { left: 16 + 7.8, top: 700, width: 31.2, height: 17.5 },
    ]);
    expect(mapper.boxes(0, 0, 4)).toEqual([]);
    expect(mapper.point(1, 99)).toEqual({ left: 16 + 8 * 7.8, top: 700 });
  });
});

describe('marks on the game own lines', () => {
  const open = { raw_lines: ['[1020/1020hp 800/800mn]'], raw_from: 1 };
  it('marks every line whole while you tell Vosh your prompt (P2)', () => {
    expect(rawMarks(open, null, true)).toEqual([{ row: 0, col: 0, width: 23, warn: false }]);
  });

  it('marks each value the newest read names on a line the row shows (P3)', () => {
    const read = {
      plain: 'Tester: [===|---]\n[1020/1020hp 800/800mn]',
      marks: [
        { line: 0, start: 0, end: 6, warn: false },
        { line: 1, start: 1, end: 5, warn: false },
        { line: 1, start: 13, end: 16, warn: true },
      ],
    };
    expect(rawMarks(open, read, false)).toEqual([
      { row: 0, col: 1, width: 4, warn: false },
      { row: 0, col: 13, width: 3, warn: true },
    ]);
    // A read of another prompt marks nothing.
    expect(rawMarks(open, { ...read, plain: 'x\n[9/9hp 8/8mn]' }, false)).toEqual([]);
    expect(rawMarks(open, null, false)).toEqual([]);
  });
});

describe('the marks on the game own line', () => {
  it('fills each value with the token alone and rings a run Vosh cannot read', () => {
    const grid: Grid = { left: 16, top: 38, cellW: 7.8, cellH: 17.5 };
    const region = { gen: 2, row: 38, col: 0, cols: 80, atBottom: true };
    const mapper = textMapper('<1020800 930mv> ', region, grid);
    const marks = rawLayout(
      [
        { row: 0, col: 1, width: 7, warn: true },
        { row: 0, col: 9, width: 3, warn: false },
      ],
      mapper,
    );
    expect(marks.picked).toEqual([]);
    expect(marks.values).toEqual([{ left: 16 + 9 * 7.8, top: 703, width: 3 * 7.8, height: 17.5 }]);
    expect(marks.warn).toEqual([{ left: 16 + 7.8, top: 703, width: 7 * 7.8, height: 17.5 }]);
  });
});

describe('what a reader hears as you pick a part', () => {
  it('names the part and what it reads, or its words', () => {
    const hp = { ...piece(1, 'value'), label: 'Health', meta: '1020 of 1020' };
    expect(pickAnnouncement(hp)).toBe('Health, 1020 of 1020');
    expect(pickAnnouncement({ ...piece(0, 'text', '['), label: 'Text' })).toBe('Text, [');
    expect(pickAnnouncement({ ...piece(4, 'nl'), label: 'Line break' })).toBe('Line break');
    expect(pickAnnouncement(null)).toBe('');
  });
});
