import { describe, expect, it } from 'vitest';
import { Terminal } from '@xterm/xterm';
import fixture from '../../fixtures/prompt/aabahran/pointer/cases.json';
import { OutputShaper } from './outputShaper';
import {
  cellAtPoint,
  dockPieceAt,
  layoutPrompt,
  pieceAt,
  pieceAtCell,
  type PieceSpan,
  type RegionOnScreen,
} from './promptPointer';
import { decodeOutputPayload, type OutputPayload, type PromptOpenRow } from './session';
import { RegionWriter } from './terminalRegion';

const span = (piece: number, row: number, col: number, width: number): PieceSpan => ({
  piece,
  row,
  col,
  width,
});

// The spans' look, which the mapping never reads.
const look = {
  fg: { kind: 'default' as const },
  bg: { kind: 'default' as const },
  bold: false,
  italic: false,
  underline: false,
};

/** Each character of `plain` laid out, as [row, col] pairs per row. */
const cells = (plain: string, startCol: number, cols: number) =>
  layoutPrompt(plain, startCol, cols).map((row) =>
    row.filter((p) => p.width > 0).map((p) => [p.row, p.col]),
  );

describe('laying a drawn prompt out from its region', () => {
  it('puts one row after the other from the region start', () => {
    expect(cells('<1020>', 0, 80)).toEqual([
      [
        [0, 0],
        [0, 1],
        [0, 2],
        [0, 3],
        [0, 4],
        [0, 5],
      ],
    ]);
    // A line break in the design starts the next row at its first column.
    expect(cells('HP 7\n<8>', 0, 80)).toEqual([
      [
        [0, 0],
        [0, 1],
        [0, 2],
        [0, 3],
      ],
      [
        [1, 0],
        [1, 1],
        [1, 2],
      ],
    ]);
  });

  it('breaks between words where the word wrap does, the space gone', () => {
    const layout = layoutPrompt('[1020/1020hp 800/800mn] ', 0, 12);
    // The first word fills the row, and the space after it is the break.
    expect(layout[0][11]).toEqual({ row: 0, col: 11, width: 1 });
    expect(layout[0][12].width).toBe(0);
    expect(layout[0][13]).toEqual({ row: 1, col: 0, width: 1 });
    expect(layout[0][22]).toEqual({ row: 1, col: 9, width: 1 });
  });

  it('wraps at the edge when the region starts partway along a row', () => {
    // The word wrap counts from the region, so only the terminal's own
    // wrap moves the rest down.
    expect(cells('abcdef', 7, 10)).toEqual([
      [
        [0, 7],
        [0, 8],
        [0, 9],
        [1, 0],
        [1, 1],
        [1, 2],
      ],
    ]);
  });

  it('gives a wide character two cells and moves it down whole at the edge', () => {
    expect(layoutPrompt('ab日', 0, 3)[0]).toEqual([
      { row: 0, col: 0, width: 1 },
      { row: 0, col: 1, width: 1 },
      { row: 1, col: 0, width: 2 },
    ]);
  });
});

describe('the piece under a cell', () => {
  const open: PromptOpenRow = {
    gen: 4,
    plain: 'HP 765\n<800 mn>',
    spans: [
      { ...span(0, 0, 0, 3), ...look },
      { ...span(1, 0, 3, 3), ...look },
      { ...span(3, 1, 0, 1), ...look },
      { ...span(4, 1, 1, 3), ...look },
    ],
  };
  const region: RegionOnScreen = { gen: 4, row: 10, col: 0, cols: 80, atBottom: true };

  it('finds the span over the character there', () => {
    expect(pieceAt(open.spans, 0, 4)).toBe(1);
    expect(pieceAt(open.spans, 1, 0)).toBe(3);
    expect(pieceAt(open.spans, 1, 5)).toBeNull();
    expect(pieceAtCell(open, region, { row: 10, col: 0 })).toBe(0);
    expect(pieceAtCell(open, region, { row: 10, col: 5 })).toBe(1);
    expect(pieceAtCell(open, region, { row: 11, col: 2 })).toBe(4);
  });

  it('maps nothing past the drawn characters, above the region or below it', () => {
    expect(pieceAtCell(open, region, { row: 10, col: 6 })).toBeNull();
    expect(pieceAtCell(open, region, { row: 11, col: 7 })).toBeNull();
    // The rows above are history, an earlier prompt among them.
    expect(pieceAtCell(open, region, { row: 9, col: 1 })).toBeNull();
    expect(pieceAtCell(open, region, { row: 12, col: 0 })).toBeNull();
  });

  it('maps nothing for another region, a screen scrolled back or no row', () => {
    expect(pieceAtCell(open, { ...region, gen: 5 }, { row: 10, col: 0 })).toBeNull();
    expect(pieceAtCell(open, { ...region, atBottom: false }, { row: 10, col: 0 })).toBeNull();
    expect(pieceAtCell(null, region, { row: 10, col: 0 })).toBeNull();
    expect(pieceAtCell(open, null, { row: 10, col: 0 })).toBeNull();
  });

  it('takes the cell under a point from the cell size', () => {
    expect(cellAtPoint(0, 0, { width: 7.8, height: 17.5 })).toEqual({ row: 0, col: 0 });
    expect(cellAtPoint(7.9, 17.4, { width: 7.8, height: 17.5 })).toEqual({ row: 0, col: 1 });
    expect(cellAtPoint(15.5, 35, { width: 7.8, height: 17.5 })).toEqual({ row: 2, col: 1 });
  });
});

interface NativeWidth {
  screen: string[];
  cursor: {
    line: number;
    col: number;
    at_bottom: boolean;
    cols: number;
    region: { gen: number; line: number; col: number } | null;
  };
  after_echo: { region: unknown };
}

interface PointerCase {
  template: string;
  show: 'text' | 'lifted' | 'pinned';
  payloads: OutputPayload[];
  open_row: PromptOpenRow | null;
  zone: number;
  native: Record<string, NativeWidth>;
}

const cases = fixture.cases as unknown as PointerCase[];

/** Every cell of `rows` mapped to a piece, as `row,col` to piece. */
function hits(
  open: PromptOpenRow,
  region: RegionOnScreen,
  rows: number,
  cols: number,
): Map<string, number> {
  const out = new Map<string, number>();
  for (let row = -2; row < rows + 2; row++) {
    for (let col = 0; col < cols; col++) {
      const piece = pieceAtCell(open, region, { row, col });
      if (piece !== null) out.set(`${row},${col}`, piece);
    }
  }
  return out;
}

/** The cells each span's characters take, as `row,col` to piece, and
 *  the character each shows, laid out from `region`. */
function expected(
  open: PromptOpenRow,
  region: RegionOnScreen,
): { cells: Map<string, number>; chars: Map<string, string> } {
  const layout = layoutPrompt(open.plain, region.col, region.cols);
  const rows = open.plain.split('\n').map((r) => Array.from(r));
  const cells = new Map<string, number>();
  const chars = new Map<string, string>();
  for (const s of open.spans) {
    for (let k = s.col; k < s.col + s.width; k++) {
      const p = layout[s.row][k];
      if (p.width === 0) continue;
      const key = `${region.row + p.row},${p.col}`;
      cells.set(key, s.piece);
      chars.set(key, rows[s.row][k]);
    }
  }
  return { cells, chars };
}

/** The session's payloads replayed into an xterm `cols` wide through the
 *  same decode, word wrap and writer Terminal.tsx uses. */
async function xtermAfter(payloads: OutputPayload[], cols: number) {
  const term = new Terminal({ cols, rows: 24, scrollback: 100, allowProposedApi: true });
  const writer = new RegionWriter(term);
  const shaper = new OutputShaper(cols);
  for (const payload of payloads) {
    const { output } = shaper.shape(decodeOutputPayload(payload));
    if (output) writer.output(output);
  }
  await new Promise<void>((resolve) => writer.whenParsed(resolve));
  return { term, writer };
}

/** The screen row `row` of xterm, counted from the top of the viewport. */
const xtermRow = (term: Terminal, row: number) =>
  term.buffer.active.getLine(term.buffer.active.viewportY + row)?.translateToString(false) ?? '';

// The session's own payloads for two pulses, a quiet prompt that the
// fight left in history and the fight's prompt with its tank line, in the
// text and lifted (src-tauri/src/session_pointer_tests.rs). Each renderer
// says where the open region starts in its own buffer, and every span
// laid out from there lands on the very characters that renderer drew.
// Nothing else maps: not the earlier prompt, not the tank line, not a
// cell past the prompt.
describe('a pointer on your prompt, in the text and lifted', () => {
  for (const c of cases.filter((c) => c.show !== 'pinned')) {
    for (const [width, native] of Object.entries(c.native)) {
      const cols = Number(width);
      const label = `${c.show}, ${JSON.stringify(c.template)}, ${cols} wide`;
      const open = c.open_row;

      it(`maps each piece the native grid drew, ${label}`, () => {
        expect(open).not.toBeNull();
        if (!open) return;
        const at = native.cursor.region;
        expect(at?.gen).toBe(open.gen);
        if (!at) return;
        const region: RegionOnScreen = {
          gen: at.gen,
          row: at.line,
          col: at.col,
          cols: native.cursor.cols,
          atBottom: native.cursor.at_bottom,
        };
        const want = expected(open, region);
        for (const [key, ch] of want.chars) {
          const [row, col] = key.split(',').map(Number);
          expect(native.screen[row]?.[col] ?? ' ', `${label} at ${key}`).toBe(ch);
        }
        expect(hits(open, region, 24, cols)).toEqual(want.cells);
        // Once your echo lands after it, nothing maps.
        expect(native.after_echo.region).toBeNull();
      });

      it(`maps each piece xterm drew, ${label}`, async () => {
        if (!open) return;
        const { term, writer } = await xtermAfter(c.payloads, cols);
        const start = writer.region();
        expect(start?.gen).toBe(open.gen);
        if (!start) return;
        const buffer = term.buffer.active;
        const region: RegionOnScreen = {
          gen: start.gen,
          row: start.row - buffer.viewportY,
          col: start.col,
          cols: term.cols,
          atBottom: buffer.viewportY === buffer.baseY,
        };
        // Both renderers put the region the same rows above the cursor.
        expect(buffer.cursorY - region.row).toBe(
          native.cursor.line - (native.cursor.region?.line ?? 0),
        );
        const want = expected(open, region);
        for (const [key, ch] of want.chars) {
          const [row, col] = key.split(',').map(Number);
          expect(xtermRow(term, row)[col] ?? ' ', `${label} at ${key}`).toBe(ch);
        }
        expect(hits(open, region, term.rows, cols)).toEqual(want.cells);
        // Your echo closes the region, so nothing maps any more.
        writer.local('look');
        await new Promise<void>((resolve) => writer.whenParsed(resolve));
        expect(writer.region()).toBeNull();
        writer.dispose();
        term.dispose();
      });
    }
  }
});

describe('a pointer on the pinned band', () => {
  const band = { text: 'tank [===|---]\r\nHP 765\r\n<800 mn>', spans: [] as PieceSpan[] };
  band.spans = [span(0, 1, 0, 3), span(1, 1, 3, 3), span(3, 2, 0, 1), span(4, 2, 1, 3)];

  for (const cell of [
    { width: 7.8, height: 17.5, cols: 120 },
    { width: 9, height: 21, cols: 120 },
  ]) {
    it(`maps the dock's own cells to pieces, ${cell.width} by ${cell.height}`, () => {
      // Three rows in a zone of three: the band's first row sits at the
      // top of the zone.
      const top = 6 + 2;
      const at = (row: number, col: number) =>
        dockPieceAt(
          band,
          3,
          cell,
          col * cell.width + cell.width / 2,
          top + row * cell.height + cell.height / 2,
        );
      expect(at(0, 0)).toBeNull();
      expect(at(1, 0)).toBe(0);
      expect(at(1, 4)).toBe(1);
      expect(at(2, 0)).toBe(3);
      expect(at(2, 3)).toBe(4);
      expect(at(2, 7)).toBeNull();
      // Left of the text, above and below the band.
      expect(dockPieceAt(band, 3, cell, -1, top + cell.height * 1.5)).toBeNull();
      expect(dockPieceAt(band, 3, cell, 1, top - 1)).toBeNull();
      expect(dockPieceAt(band, 3, cell, 1, top + 3 * cell.height + 1)).toBeNull();
    });
  }

  it('cuts the spans where the zone cuts the band', () => {
    const cell = { width: 8, height: 20, cols: 120 };
    // A zone of two keeps the last two rows, so the tank line goes.
    const top = 6 + 2;
    expect(dockPieceAt(band, 2, cell, 4, top + 10)).toBe(0);
    expect(dockPieceAt(band, 2, cell, 4, top + 30)).toBe(3);
    // A shorter band sits at the bottom of its zone.
    const short = { text: '<800 mn>', spans: [span(0, 0, 0, 1), span(1, 0, 1, 3)] };
    expect(dockPieceAt(short, 2, cell, 12, top + 10)).toBeNull();
    expect(dockPieceAt(short, 2, cell, 12, top + 30)).toBe(1);
  });

  it('maps nothing on the clipped last cell or without a band', () => {
    const cell = { width: 8, height: 20, cols: 6 };
    const long = { text: 'abcdefghij', spans: [span(0, 0, 0, 10)] };
    expect(dockPieceAt(long, 1, cell, 4 * 8 + 4, 8 + 10)).toBe(0);
    expect(dockPieceAt(long, 1, cell, 5 * 8 + 4, 8 + 10)).toBeNull();
    expect(dockPieceAt(null, 1, cell, 4, 18)).toBeNull();
  });

  it('counts a wide character as two cells and one character', () => {
    const cell = { width: 8, height: 20, cols: 120 };
    const wide = { text: '日本 7', spans: [span(0, 0, 0, 2), span(1, 0, 3, 1)] };
    expect(dockPieceAt(wide, 1, cell, 3 * 8 + 4, 18)).toBe(0);
    expect(dockPieceAt(wide, 1, cell, 5 * 8 + 4, 18)).toBe(1);
  });

  for (const c of cases.filter((c) => c.show === 'pinned')) {
    it(`maps the band the session pinned, ${JSON.stringify(c.template)}`, () => {
      const pin = [...c.payloads].reverse().find((p) => typeof p.pin === 'string');
      expect(pin?.pin_spans?.length).toBeGreaterThan(0);
      if (!pin?.pin || !pin.pin_spans) return;
      const text = new TextDecoder().decode(decodeOutputPayload(pin).pin);
      const cell = { width: 8, height: 20, cols: 120 };
      // eslint-disable-next-line no-control-regex
      const rows = text.replace(/\x1b\[[0-9;:]*m/g, '').split('\r\n');
      const shown = rows.slice(Math.max(0, rows.length - c.zone));
      const first = rows.length - shown.length;
      const top = 6 + 2 + (c.zone - shown.length) * cell.height;
      for (const s of pin.pin_spans) {
        const row = s.row - first;
        if (row < 0) continue;
        for (let k = s.col; k < s.col + s.width; k++) {
          const x = k * cell.width + 1;
          const y = top + row * cell.height + 1;
          expect(dockPieceAt({ text, spans: pin.pin_spans }, c.zone, cell, x, y)).toBe(s.piece);
        }
      }
      // The tank line the band shows as sent has no pieces.
      if (first === 0 && rows.length > 1) {
        expect(dockPieceAt({ text, spans: pin.pin_spans }, c.zone, cell, 1, top + 1)).toBeNull();
      }
    });
  }
});
