import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { Terminal } from '@xterm/xterm';
import { resolveCell, type BandEnv } from '../../lib/bandCells';
import { planSubmit } from '../../lib/maskedInput';
import {
  bandRows,
  DOCK_GAP,
  dockHeight,
  dockRows,
  lentRows,
  type CellSize,
} from '../../lib/promptBand';
import type { PromptShowState } from '../../lib/promptShow';
import { dockPieceAt, type PieceSpan } from '../../lib/promptPointer';
import { parseSgrCells, PLAIN, shownColumns } from '../../lib/sgrCells';
import { RegionWriter } from '../../lib/terminalRegion';
import { keptRows, spareAbove } from '../../lib/terminalRows';
import { PinnedBand } from './PromptDock';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// Nord's terminal colors, as the palette the band resolves through.
const NORD: BandEnv = {
  palette: [
    '#3b4252',
    '#bf616a',
    '#a3be8c',
    '#ebcb8b',
    '#81a1c1',
    '#b48ead',
    '#88c0d0',
    '#e5e9f0',
    '#4c566a',
    '#bf616a',
    '#a3be8c',
    '#ebcb8b',
    '#81a1c1',
    '#b48ead',
    '#8fbcbb',
    '#eceff4',
  ],
  fg: '#d8dee9',
  bg: '#2e3440',
  renderer: 'xterm',
  brightBold: false,
};

const pinned: PromptShowState = {
  show: 'pinned',
  capture: true,
  gameSent: true,
  zone: 2,
  promptsOff: false,
};

// The board cell: JetBrains Mono 13 in a 17.5 row.
const CELL: CellSize = { width: 7.8, height: 17.5, cols: 120 };

function draw(pin: string | null, patch: Partial<PromptShowState> = {}, cell = CELL): string {
  return renderToStaticMarkup(
    <PinnedBand state={{ ...pinned, ...patch }} pin={pin} cell={cell} fontSize={13} env={NORD} />,
  );
}

const style = (html: string, marker: string): string => {
  const m = new RegExp(`${marker}[^>]*style="([^"]*)"|style="([^"]*)"[^>]*${marker}`).exec(html);
  return m?.[1] ?? m?.[2] ?? '';
};
const px = (css: string, prop: string) =>
  Number(new RegExp(`${prop}:\\s*([\\d.-]+)px`).exec(css)?.[1]);
/** A length the markup may leave out, or write as a bare 0. */
const pxOr0 = (css: string, prop: string) =>
  Number(new RegExp(`(?:^|;)${prop}:\\s*([\\d.-]+)(?:px)?(?:;|$)`).exec(css)?.[1] ?? 0);
const glyphs = (html: string) =>
  [...html.matchAll(/class="prompt-band-glyph"[^>]*>([^<]*)</g)].map((m) => m[1]).join('');

// The default design out of a fight and in one, where a row with the
// tank's gauge comes above your vitals.
const CALM = '\x1b[32m1020\x1b[39m/1020hp \x1b[36m800\x1b[39m/800mn ';
const FIGHT = `Tamwell: \x1b[33m█████████\x1b[90m░\x1b[39m\r\n${CALM}`;

/** The dock's box: its height, how far it reaches up over the terminal,
 *  and the place it takes under the terminal, which is the two together. */
function dockBox(html: string): { height: number; reach: number; place: number } {
  const css = style(html, 'class="prompt-dock"');
  const height = px(css, 'height');
  const reach = 0 - pxOr0(css, 'margin-top');
  return { height, reach, place: height - reach };
}

describe('the pinned band', () => {
  it('is one row tall out of a fight, with no empty row above the band', () => {
    expect(dockHeight(1, 17.5)).toBe(27.5 + 3.5);
    expect(dockHeight(2, 17.5)).toBe(45 + 3.5);
    expect(dockHeight(3, 17.5)).toBe(62.5 + 3.5);
    // The zone of the default design is two, the tank's row and yours.
    const html = draw(CALM, { zone: 2 });
    expect(dockBox(html)).toEqual({ height: 31, reach: 0, place: 31 });
    // The band's top sits the 6 px gap under the dock's top, so nothing
    // in the dock is empty but that gap.
    const band = style(html, 'data-prompt-band');
    expect(dockBox(html).height - px(band, 'bottom') - px(band, 'height')).toBe(DOCK_GAP);
    expect(html).toContain('data-rows="1"');
  });

  it('grows a row in a fight, borrowed from the bottom of the terminal', () => {
    const html = draw(FIGHT, { zone: 2 });
    // Two rows tall, reaching one row up over the terminal, so its place
    // under the terminal stays the one row place.
    expect(dockBox(html)).toEqual({ height: 48.5, reach: 17.5, place: 31 });
    const band = style(html, 'data-prompt-band');
    expect(px(band, 'height')).toBe(2 * 17.5 + 4);
    expect(dockBox(html).height - px(band, 'bottom') - px(band, 'height')).toBe(DOCK_GAP);
    expect(glyphs(html).startsWith('Tamwell:')).toBe(true);
    expect(html).toContain('data-rows="2"');
  });

  it('keeps its one row place before any prompt and after you disconnect', () => {
    const empty = draw(null, { zone: 3 });
    expect(dockBox(empty)).toEqual({ height: 31, reach: 0, place: 31 });
    expect(empty).not.toContain('data-prompt-band');
  });

  it('holds the prompts off sentence on its one row', () => {
    const html = draw(FIGHT, { zone: 2, promptsOff: true });
    expect(dockBox(html)).toEqual({ height: 31, reach: 0, place: 31 });
    expect(html).not.toContain('data-prompt-band');
    // The sentence sits on the row the band's text would take.
    const note = style(html, 'data-prompt-dock-note');
    expect(px(note, 'top')).toBe(31 - 3.5 - 2 - 17.5 + (17.5 - 16) / 2);
  });

  it('never takes more rows than the zone, and never fewer than one', () => {
    const three = 'one\r\ntwo\r\nthree';
    expect(dockRows(three, 2, false)).toBe(2);
    expect(dockRows(three, 3, false)).toBe(3);
    expect(dockRows(three, 6, false)).toBe(3);
    expect(dockRows(FIGHT, 2, false)).toBe(2);
    expect(dockRows(CALM, 2, false)).toBe(1);
    // Rows that show nothing at the end take no room.
    expect(dockRows(`${CALM}\r\n  \r\n`, 3, false)).toBe(1);
    expect(dockRows(null, 2, false)).toBe(1);
    expect(dockRows('', 2, false)).toBe(1);
    expect(dockRows(FIGHT, 2, true)).toBe(1);
    expect(dockBox(draw(three, { zone: 2 }))).toEqual({ height: 48.5, reach: 17.5, place: 31 });
    // Whatever it shows, its place under the terminal stays the same.
    for (const pin of [null, CALM, FIGHT, three]) {
      for (const zone of [1, 2, 3, 6]) {
        expect(dockBox(draw(pin, { zone })).place).toBe(31);
      }
    }
  });

  it('draws one row at the bottom of its zone, 4 px past the text and 2 px above and below', () => {
    const html = draw('\x1b[32m1020\x1b[39m/1020hp ');
    const band = style(html, 'data-prompt-band');
    expect(px(band, 'left')).toBe(-4);
    // The board's band ends 9.5 px above the input band, 3.5 inside the
    // dock, whose bottom is the terminal area's 6 px above it.
    expect(px(band, 'bottom')).toBe(3.5);
    expect(px(band, 'height')).toBe(17.5 + 4);
    // Eleven cells to the last glyph, the trailing space not counted.
    expect(px(band, 'width')).toBeCloseTo(11 * 7.8 + 8, 5);
    expect(glyphs(html)).toBe('1020/1020hp');
    expect(html).toContain('color:#a3be8c');
  });

  it('spans every row as one band as wide as the longest', () => {
    const html = draw('Tester: [===|---]\r\n<1020hp 800m>');
    const band = style(html, 'data-prompt-band');
    expect(px(band, 'height')).toBe(2 * 17.5 + 4);
    expect(px(band, 'width')).toBeCloseTo(17 * 7.8 + 8, 5);
  });

  it('keeps the last rows when a prompt has more than the zone', () => {
    const rows = bandRows('one\r\ntwo\r\nthree', 2);
    expect(rows.map((r) => r.map((c) => c.ch).join(''))).toEqual(['two', 'three']);
  });

  it('ends a row too wide for the terminal on an ellipsis in the last cell', () => {
    const html = draw('abcdefghij', {}, { ...CELL, cols: 6 });
    expect(glyphs(html)).toBe('abcde');
    expect(html).toContain('prompt-band-more');
    expect(html).toContain('…');
    const band = style(html, 'data-prompt-band');
    expect(px(band, 'width')).toBeCloseTo(6 * 7.8 + 8, 5);
  });

  it('says prompts are off in place of the band', () => {
    const html = draw('<1020hp>', { promptsOff: true });
    expect(html).not.toContain('data-prompt-band');
    expect(html).toContain(
      'You turned prompts off in the game. Type prompt in the game to turn them back on.',
    );
  });

  it('puts every character on its own cell, so fallback glyphs never drift', () => {
    const html = draw('\x1b[33m█████\x1b[90m░░░░░\x1b[39m 54%');
    const lefts = [...html.matchAll(/class="prompt-band-glyph" style="[^"]*left:([\d.]+)px/g)].map(
      (m) => Number(m[1]),
    );
    expect(lefts.slice(0, 3)).toEqual([4, 4 + 7.8, 4 + 2 * 7.8]);
  });
});

// The card maps a pointer on the dock from the dock's own grid (addendum
// item 4). Each glyph the band draws, at its center, maps to the piece
// whose span covers it, at two cell sizes.
describe('a pointer on the pinned band', () => {
  /** Each glyph the markup draws, with its center from the dock's top
   *  left. */
  function glyphCenters(html: string, cell: CellSize): { ch: string; x: number; y: number }[] {
    const dockH = px(style(html, 'class="prompt-dock"'), 'height');
    const band = style(html, 'data-prompt-band');
    const bandTop = dockH - px(band, 'bottom') - px(band, 'height');
    const bandLeft = px(band, 'left');
    const out: { ch: string; x: number; y: number }[] = [];
    for (const row of html.split('class="prompt-band-row"').slice(1)) {
      const rowTop = Number(/^ style="top:([\d.]+)px/.exec(row)?.[1]);
      for (const m of row.matchAll(
        /class="prompt-band-glyph" style="[^"]*left:([\d.]+)px;width:([\d.]+)px[^"]*">([^<]*)</g,
      )) {
        out.push({
          ch: m[3].replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&'),
          x: bandLeft + Number(m[1]) + Number(m[2]) / 2,
          y: bandTop + rowTop + cell.height / 2,
        });
      }
    }
    return out;
  }

  const look = {
    fg: { kind: 'default' as const },
    bg: { kind: 'default' as const },
    bold: false,
    italic: false,
    underline: false,
  };
  // The tank line the design leaves as sent, then two rows of pieces.
  const text = 'Tester: [===|---]\r\n\x1b[32mHP\x1b[39m 765\r\n<800 mn>';
  const spans: PieceSpan[] = [
    { piece: 0, row: 1, col: 0, width: 2 },
    { piece: 1, row: 1, col: 2, width: 1 },
    { piece: 2, row: 1, col: 3, width: 3 },
    { piece: 4, row: 2, col: 0, width: 1 },
    { piece: 5, row: 2, col: 1, width: 3 },
    { piece: 6, row: 2, col: 4, width: 4 },
  ];

  for (const cell of [CELL, { width: 9.6, height: 21, cols: 120 }]) {
    it(`maps every glyph the dock draws to its piece, ${cell.width} by ${cell.height}`, () => {
      const html = draw(text, { zone: 3 }, cell);
      const centers = glyphCenters(html, cell);
      expect(centers.map((g) => g.ch).join('')).toBe('Tester:[===|---]HP765<800mn>');
      const pieces = centers.map((g) =>
        dockPieceAt({ text, spans: spans.map((s) => ({ ...s, ...look })) }, 3, cell, g.x, g.y),
      );
      expect(pieces).toEqual([...Array<null>(16).fill(null), 0, 0, 2, 2, 2, 4, 5, 5, 5, 6, 6, 6]);
      // The zone of two cuts the tank line, and the spans with it.
      const cut = draw(text, { zone: 2 }, cell);
      const first = glyphCenters(cut, cell)[0];
      expect(first.ch).toBe('H');
      expect(dockPieceAt({ text, spans }, 2, cell, first.x, first.y)).toBe(0);
    });
  }
});

describe('cells and colors on the band', () => {
  it('reads every SGR attribute a prompt can carry', () => {
    const [row] = parseSgrCells('\x1b[1;2;3;4:3;9;7;8;38;5;208;48;2;1;2;3;58:2::9:8:7mx\x1b[0my');
    expect(row[0].attrs).toEqual({
      fg: { kind: 'indexed', n: 208 },
      bg: { kind: 'rgb', r: 1, g: 2, b: 3 },
      bold: true,
      dim: true,
      italic: true,
      underline: 3,
      underlineColor: { kind: 'rgb', r: 9, g: 8, b: 7 },
      strike: true,
      inverse: true,
      hidden: true,
    });
    expect(row[1].attrs).toEqual(PLAIN);
  });

  it('counts a wide character as two columns and a trailing space as none', () => {
    const [row] = parseSgrCells('日本 ');
    expect(row.map((c) => c.width)).toEqual([2, 1, 2, 1, 1]);
    expect(shownColumns(row)).toBe(4);
  });

  it('brightens bold 30 to 37 and dims as xterm does', () => {
    const bold = resolveCell({ ...PLAIN, bold: true, fg: { kind: 'named', n: 1 } }, NORD);
    expect(bold.color).toBe(NORD.palette[9]);
    expect(bold.bold).toBe(true);
    const dim = resolveCell({ ...PLAIN, dim: true, fg: { kind: 'named', n: 2 } }, NORD);
    expect(dim.color).toBe('rgba(163, 190, 140, 0.5)');
    // 38;5;1 is not brightened by bold.
    const indexed = resolveCell({ ...PLAIN, bold: true, fg: { kind: 'indexed', n: 1 } }, NORD);
    expect(indexed.color).toBe(NORD.palette[1]);
  });

  it('dims and bolds as the native grid does over it', () => {
    const native: BandEnv = { ...NORD, renderer: 'native' };
    const dim = resolveCell(
      { ...PLAIN, dim: true, fg: { kind: 'rgb', r: 200, g: 200, b: 200 } },
      native,
    );
    expect(dim.color).toBe('#9f9f9f');
    const bright = resolveCell({ ...PLAIN, fg: { kind: 'named', n: 9 } }, native);
    expect(bright.bold).toBe(false);
    expect(
      resolveCell({ ...PLAIN, fg: { kind: 'named', n: 9 } }, { ...native, brightBold: true }).bold,
    ).toBe(true);
    // The native grid draws concealed text and one solid underline.
    const hidden = resolveCell({ ...PLAIN, hidden: true, underline: 3 }, native);
    expect(hidden.color).toBe(NORD.fg);
    expect(hidden.decorationStyle).toBe('solid');
    expect(resolveCell({ ...PLAIN, hidden: true }, NORD).color).toBe('transparent');
  });

  it('follows the palette the terminal uses, theme or base', () => {
    const base: BandEnv = { ...NORD, palette: NORD.palette.map(() => '#808080') };
    expect(resolveCell({ ...PLAIN, fg: { kind: 'named', n: 2 } }, base).color).toBe('#808080');
    // The cube and the grays are fixed.
    expect(resolveCell({ ...PLAIN, fg: { kind: 'indexed', n: 16 + 36 * 5 } }, NORD).color).toBe(
      '#ff0000',
    );
    expect(resolveCell({ ...PLAIN, fg: { kind: 'indexed', n: 244 } }, NORD).color).toBe('#808080');
  });

  it('swaps the colors of an inverse cell and draws its ground over the band', () => {
    const look = resolveCell({ ...PLAIN, inverse: true }, NORD);
    expect(look.color).toBe(NORD.bg);
    expect(look.background).toBe(NORD.fg);
  });
});

describe('Enter while your prompt shows pinned', () => {
  it('echoes nothing for an empty line at your pinned prompt, and typed lines as before', () => {
    const context = { masked: false, quickKey: false, echoColor: null, pinRowOpen: true };
    expect(planSubmit('', context).echo).toBeNull();
    expect(planSubmit('look', context).echo).toBe('look\r\n');
    expect(planSubmit('', { ...context, masked: true }).echo).toBe('\r\n');
  });

  it('ends the row of a prompt left in the text, such as the pager', () => {
    const context = { masked: false, quickKey: false, echoColor: null, pinRowOpen: false };
    expect(planSubmit('', context).echo).toBe('\r\n');
  });
});

// The terminal gives the dock the rows it borrows: xterm keeps the rows
// its pane fits less those (keptRows), and the native grid does the same
// in term_grid.rs. Each step here lays the dock out from its real markup
// and sizes a real xterm as the live pane does, so the gap between the
// newest line and the band's top is what the window shows.
describe('the text above the pinned band', () => {
  /** The screen's rows, trailing blanks trimmed, up to the last row that
   *  shows anything. */
  function screen(term: Terminal): string[] {
    const buffer = term.buffer.active;
    const rows: string[] = [];
    for (let y = 0; y < term.rows; y++) {
      rows.push(buffer.getLine(buffer.baseY + y)?.translateToString(true) ?? '');
    }
    while (rows.length > 0 && rows[rows.length - 1] === '') rows.pop();
    return rows;
  }

  function parsed(writer: RegionWriter): Promise<void> {
    return new Promise((resolve) => writer.whenParsed(resolve));
  }

  // A pane with no pixels to spare, with some, and with most of a row.
  for (const spare of [0, 9, 17.25]) {
    it(`keeps the newest line 6 px over the band through four fights, ${spare} px over`, async () => {
      const paneHeight = 20 * CELL.height + spare;
      const fit = Math.floor(paneHeight / CELL.height);
      const term = new Terminal({ cols: 60, rows: fit, scrollback: 100, allowProposedApi: true });
      const writer = new RegionWriter(term);
      // Forty lines, the line end before the pinned prompt held back as
      // the session holds it, so the newest line is the last on screen.
      const lines = Array.from({ length: 40 }, (_, i) => `line ${i + 1}`);
      writer.output({ text: lines.join('\r\n'), hold: '\r\n' });
      await parsed(writer);
      // A fight starts and ends four times within a second.
      const pins = [CALM, FIGHT, CALM, FIGHT, FIGHT, CALM, FIGHT, CALM, FIGHT, CALM];
      const seen: { gap: number; first: string; last: string }[] = [];
      for (const pin of pins) {
        const html = draw(pin, { zone: 2 });
        const lent = lentRows(dockRows(pin, 2, false));
        term.resize(60, keptRows(fit, lent));
        await parsed(writer);
        // The dock's place starts where the pane ends, and the dock
        // reaches up over the pane by the rows it borrows.
        const box = dockBox(html);
        const band = style(html, 'data-prompt-band');
        const bandTop =
          paneHeight - box.reach + box.height - px(band, 'bottom') - px(band, 'height');
        // The grid keeps to the bottom of the pane, the pixels its rows
        // leave over above its first row.
        const gridTop = spareAbove(paneHeight, term.rows + lent, CELL.height, 2);
        const textBottom = gridTop + term.rows * CELL.height;
        const rows = screen(term);
        seen.push({ gap: bandTop - textBottom, first: rows[0], last: rows[rows.length - 1] });
      }
      // The newest line never moves off the row right above the band,
      // 6 px over it each time, whatever the pane leaves over. The grid
      // moves on whole device pixels, so at 2x the gap can run half a
      // pixel more, the same at every step.
      for (const step of seen) {
        expect(step.gap).toBeGreaterThanOrEqual(DOCK_GAP - 1e-9);
        expect(step.gap).toBeLessThan(DOCK_GAP + 0.5);
        expect(step.gap).toBeCloseTo(seen[0].gap, 9);
        expect(step.last).toBe('line 40');
      }
      // The line the band's new row takes leaves at the top, and comes
      // back when the fight ends. No line is lost or doubled.
      expect(seen.map((s) => s.first)).toEqual(
        pins.map((pin) => (pin === FIGHT ? 'line 22' : 'line 21')),
      );
      expect(term.rows).toBe(fit);
      expect(screen(term)).toEqual(lines.slice(20));
      // The held line end still goes first when the next text lands.
      writer.output({ text: 'tell' });
      await parsed(writer);
      expect(screen(term).slice(-2)).toEqual(['line 40', 'tell']);
      term.dispose();
    });
  }

  it('borrows every row of the band past its first', () => {
    expect(lentRows(1)).toBe(0);
    expect(lentRows(2)).toBe(1);
    expect(lentRows(0)).toBe(0);
  });
});
