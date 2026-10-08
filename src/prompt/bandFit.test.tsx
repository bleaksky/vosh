import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { BandEnv } from '../terminal/bandCells';
import type { PromptShowState } from '../ipc/prompt';
import { fitBand, lineColumns, linesPlain, type BandSpan } from './bandFit';
import { dockGap } from './pinnedDock';
import { dockPieceAt } from './promptPointer';
import { PinnedBand } from './PromptDock';

// The pinned band fits a row with %{right} to the terminal's columns:
// the gap the push put in shrinks first, and only a row whose two parts
// cannot both fit ends the part before the push on an ellipsis.

// The prompt James saw cut on 2026-10-08, its left part, the push, and
// its right part.
const LEFT = 'H 329/329  M 9999/9999  V 9999/9999  standing';
const RIGHT = 'cloudy · common  │  7PM  65835 tnl  0.0Kg  1000 cp(Wizi 52)(Incog 52)';

/** The row as the session pushed it out to `cols`, in a few colors, with
 *  its three spans: the left part, the push, the right part. */
function pushed(cols: number): { text: string; spans: BandSpan[] } {
  const gap = Math.max(1, cols - LEFT.length - RIGHT.length);
  const text = `\x1b[33mH\x1b[0m \x1b[92m329\x1b[0m/329${LEFT.slice(9)}${' '.repeat(gap)}\x1b[35m${RIGHT}\x1b[0m`;
  return {
    text,
    spans: [
      { piece: 0, row: 0, col: 0, width: LEFT.length },
      { piece: 1, row: 0, col: LEFT.length, width: gap, push: true },
      { piece: 2, row: 0, col: LEFT.length + gap, width: RIGHT.length },
    ],
  };
}

const plainOf = (cols: number, band = pushed(155)) =>
  linesPlain(fitBand(band.text, band.spans, 1, cols).lines)[0];

describe('fitBand', () => {
  it('keeps the right part whole when the session pushed wider than the band', () => {
    // The session pushed to the 155 columns the game was told, and the
    // band has 153: the gap gives up two spaces and nothing is cut.
    const band = pushed(155);
    const fit = fitBand(band.text, band.spans, 1, 153);
    const [line] = fit.lines;
    expect(line.more).toBeNull();
    expect(lineColumns(line)).toBe(153);
    expect(linesPlain(fit.lines)[0]).toBe(LEFT + ' '.repeat(153 - 45 - 69) + RIGHT);
    // The pieces move with it: the push takes the shorter gap, and the
    // right part ends on the band's last column.
    expect(fit.spans.map((s) => [s.piece, s.col, s.width])).toEqual([
      [0, 0, 45],
      [1, 45, 39],
      [2, 84, 69],
    ]);
  });

  it('puts the right part on the last column of a wider band too', () => {
    expect(plainOf(170)).toBe(LEFT + ' '.repeat(170 - 45 - 69) + RIGHT);
    expect(plainOf(115)).toBe(`${LEFT} ${RIGHT}`);
  });

  it('cuts the left part before the right part when both cannot fit', () => {
    const band = pushed(155);
    const fit = fitBand(band.text, band.spans, 1, 100);
    const [line] = fit.lines;
    // The right part keeps its 69 cells on columns 31 to 99, one space
    // before it and the ellipsis before that, on column 29.
    expect(line.more).toBe(29);
    expect(linesPlain(fit.lines)[0]).toBe(`${LEFT.slice(0, 29)}  ${RIGHT}`);
    expect(lineColumns(line)).toBe(100);
    expect(fit.spans.map((s) => [s.piece, s.col, s.width])).toEqual([
      [0, 0, 29],
      [1, 30, 1],
      [2, 31, 69],
    ]);
    // The ellipsis drops the spaces it would follow.
    const spaced = fitBand(band.text, band.spans, 1, 69 + 2 + 26).lines[0];
    expect(spaced.more).toBe(LEFT.slice(0, 25).trimEnd().length);
  });

  it('cuts the end of a right part too wide on its own', () => {
    const fit = fitBand(pushed(155).text, pushed(155).spans, 1, 70);
    const [line] = fit.lines;
    expect(line.more).toBe(69);
    expect(linesPlain(fit.lines)[0]).toBe(`${LEFT} ${RIGHT}`.slice(0, 69));
    // The right part's span stops at the ellipsis, and the push keeps
    // its one space.
    expect(fit.spans.map((s) => [s.piece, s.col, s.width])).toEqual([
      [0, 0, 45],
      [1, 45, 1],
      [2, 46, 23],
    ]);
  });

  it('cuts a row with no push at its end, as before', () => {
    const fit = fitBand('abcdefghij', [{ piece: 0, row: 0, col: 0, width: 10 }], 1, 6);
    expect(fit.lines[0].more).toBe(5);
    expect(linesPlain(fit.lines)).toEqual(['abcde']);
    expect(fit.spans).toEqual([{ piece: 0, row: 0, col: 0, width: 5 }]);
    // A row that fits stays as it is.
    expect(fitBand('abc', [], 1, 6).lines[0]).toMatchObject({ more: null });
  });

  it('never cuts a wide character in half', () => {
    // 日本 takes four columns. Cut to three, it keeps the first.
    const spans = [
      { piece: 0, row: 0, col: 0, width: 4 },
      { piece: 1, row: 0, col: 4, width: 6, push: true },
      { piece: 2, row: 0, col: 10, width: 2 },
    ];
    const fit = fitBand(`日本${' '.repeat(6)}ok`, spans, 1, 6);
    expect(fit.lines[0].more).toBe(2);
    expect(linesPlain(fit.lines)).toEqual(['日  ok']);
  });

  it('fits only the rows the zone shows and leaves the others alone', () => {
    const band = pushed(155);
    const spans = [
      { piece: 9, row: 0, col: 0, width: 4 },
      ...band.spans.map((s) => ({ ...s, row: 1 })),
    ];
    const fit = fitBand(`tank\r\n${band.text}`, spans, 1, 153);
    expect(fit.first).toBe(1);
    expect(fit.lines).toHaveLength(1);
    expect(fit.spans[0]).toEqual({ piece: 9, row: 0, col: 0, width: 4 });
    expect(fit.spans[3]).toMatchObject({ piece: 2, row: 1, col: 84 });
  });
});

// Nord's terminal colors.
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
  selection: '#4c566a',
  selectionText: '#eceff4',
  renderer: 'xterm',
  brightBold: false,
};

const state: PromptShowState = {
  show: 'pinned',
  capture: true,
  draw: true,
  gameSent: true,
  zone: 1,
  promptsOff: false,
};

describe('the pinned band with a push', () => {
  const cell = { width: 6, height: 15, cols: 153 };
  const draw = (cols: number) => {
    const band = pushed(155);
    return renderToStaticMarkup(
      <PinnedBand
        state={state}
        pin={band.text}
        spans={band.spans}
        cell={{ ...cell, cols }}
        fontSize={10}
        env={NORD}
      />,
    );
  };
  const glyphAt = (html: string, ch: string) =>
    [...html.matchAll(/class="prompt-band-glyph[^"]*" style="[^"]*left:([\d.]+)px[^"]*">([^<]*)</g)]
      .filter((m) => m[2] === ch)
      .map((m) => Number(m[1]));

  it('draws the whole right part with no ellipsis, its last cell on the last column', () => {
    const html = draw(153);
    expect(html).not.toContain('prompt-band-more');
    // The closing parenthesis of (Incog 52) sits in column 152, 4 px in.
    expect(glyphAt(html, ')').at(-1)).toBe(4 + 152 * 6);
  });

  it('ends the left part on the ellipsis when the band is too narrow for both', () => {
    const html = draw(100);
    expect(html).toContain('prompt-band-more');
    expect(glyphAt(html, ')').at(-1)).toBe(4 + 99 * 6);
    expect(glyphAt(html, '…')).toEqual([4 + 29 * 6]);
    // The band still reaches the right part's last cell.
    expect(html).toMatch(
      new RegExp(
        `data-prompt-band[^>]*width:${100 * 6 + 8}px|width:${100 * 6 + 8}px[^>]*data-prompt-band`,
      ),
    );
  });

  it('maps a point on the moved right part to its piece', () => {
    const band = pushed(155);
    const top = dockGap(cell.height) + 2;
    const at = (col: number) => dockPieceAt(band, 1, cell, col * 6 + 3, top + 5);
    expect(at(84)).toBe(2);
    expect(at(152)).toBe(2);
    expect(at(83)).toBe(1);
    expect(at(44)).toBe(0);
  });
});
