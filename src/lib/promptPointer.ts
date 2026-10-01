// Which piece of your prompt a pointer is on (the prompt build spec,
// section 7 step 6, and the 2026-09-30 addendum item 4).
//
// In the text and lifted, your prompt is the open row, a region the
// session marked. Each renderer knows where that region starts in its own
// buffer: xterm from the marker its RegionWriter keeps, the native grid
// from terminal_cursor. The card reads the open row's spans and its rows
// as plain text (prompt_state_get). This lays the rows out from the
// region's start at the renderer's width, the way the shared word wrap
// and the terminal's own wrap at the edge placed them, and finds the span
// over the cell under the pointer. Only the open row maps. An earlier
// prompt in history, a tank line shown as sent, a cell past the prompt,
// a region something was written after, and a screen scrolled back all
// map to nothing.
//
// Pinned, the band never wraps. The dock puts each cell at its column on
// the terminal's cell grid, from the text's left edge, so a point maps
// straight to a row and column of the band and from there to a span. In
// terminal area coordinates that is col = floor((x - 16) / cellW), the
// area's padding being 16. A cell past the last whole one, where the dock
// draws an ellipsis, maps to nothing.

import { bandCut, bandRowsTop, type CellSize } from './promptBand';
import { isWide, shownColumns, type Cell } from './sgrCells';
import type { PromptSpan } from './session';
import { wrapBreaks } from './wordWrap';

/** The part of a span the mapping reads: the piece, the row from `%nl`,
 *  and the characters it covers in that row before any wrap. */
export type PieceSpan = Pick<PromptSpan, 'piece' | 'row' | 'col' | 'width'>;

/** A cell on screen: its row from the top of the visible screen, and its
 *  column. */
export interface ScreenCell {
  row: number;
  col: number;
}

/** Where the open region starts on screen, as a renderer holds it. */
export interface RegionOnScreen {
  gen: number;
  /** Its first row, from the top of the visible screen. Negative once it
   *  starts above it. */
  row: number;
  col: number;
  /** The renderer's columns. */
  cols: number;
  /** The screen shows the live tail, not history you scrolled back to. */
  atBottom: boolean;
}

/** Where one character of a drawn prompt landed: its row from the
 *  region's first, its column, and the cells it takes. A space the word
 *  wrap broke at takes none, and neither does a combining mark. */
export interface Placed {
  row: number;
  col: number;
  width: number;
}

/** Lay out `plain`, the rows of a drawn prompt joined by `\n`, from
 *  column `startCol` of a terminal `cols` wide: one entry per character,
 *  as the spans count them. Each row is word wrapped as the renderers wrap
 *  it, counting from the region's start, and the terminal moves a
 *  character that does not fit to the next row. A row after a line break
 *  starts at the first column. */
export function layoutPrompt(plain: string, startCol: number, cols: number): Placed[][] {
  const width = Math.max(1, cols);
  let row = 0;
  let col = Math.min(Math.max(0, startCol), width - 1);
  return plain.split('\n').map((text, index) => {
    if (index > 0) {
      row += 1;
      col = 0;
    }
    const breaks = wrapBreaks(text, width);
    let next = 0;
    const placed: Placed[] = [];
    for (let i = 0; i < text.length; ) {
      const code = text.codePointAt(i) ?? 0;
      const units = code > 0xffff ? 2 : 1;
      let gone = false;
      // The word wrap's CRLF goes before this character, or in its place.
      while (next < breaks.length && breaks[next].at < i + units) {
        gone = gone || breaks[next].replaced;
        next += 1;
        row += 1;
        col = 0;
      }
      if (gone) {
        placed.push({ row, col, width: 0 });
        i += units;
        continue;
      }
      const glyph = String.fromCodePoint(code);
      if (/\p{M}/u.test(glyph)) {
        // A combining mark joins the cell before it.
        placed.push({ row, col: Math.max(0, col - 1), width: 0 });
        i += units;
        continue;
      }
      const cells = isWide(code) ? 2 : 1;
      if (col + cells > width) {
        row += 1;
        col = 0;
      }
      placed.push({ row, col, width: cells });
      col += cells;
      i += units;
    }
    return placed;
  });
}

/** The piece whose span covers character `char` of row `row`, or null. */
export function pieceAt(spans: readonly PieceSpan[], row: number, char: number): number | null {
  for (const span of spans) {
    if (span.row === row && char >= span.col && char < span.col + span.width) return span.piece;
  }
  return null;
}

/** The open row as the card reads it: its region, its rows as plain
 *  text joined by `\n`, and where each piece landed in them. */
export interface OpenPrompt {
  gen: number;
  plain: string;
  spans: readonly PieceSpan[];
}

/** The piece of the open row drawn in screen cell `cell`, or null. The
 *  region has to be the open row's and the screen at the bottom. */
export function pieceAtCell(
  open: OpenPrompt | null,
  region: RegionOnScreen | null,
  cell: ScreenCell,
): number | null {
  if (!open || !region || open.gen !== region.gen || !region.atBottom) return null;
  const row = cell.row - region.row;
  if (row < 0) return null;
  const layout = layoutPrompt(open.plain, region.col, region.cols);
  for (let r = 0; r < layout.length; r++) {
    const placed = layout[r];
    for (let char = 0; char < placed.length; char++) {
      const p = placed[char];
      if (p.width > 0 && p.row === row && cell.col >= p.col && cell.col < p.col + p.width) {
        return pieceAt(open.spans, r, char);
      }
    }
  }
  return null;
}

/** The cell under a point `x` and `y` from the top left of a grid whose
 *  cells are `cell` in size. */
export function cellAtPoint(
  x: number,
  y: number,
  cell: { width: number; height: number },
): ScreenCell {
  return { row: Math.floor(y / cell.height), col: Math.floor(x / cell.width) };
}

/** The character the cell in column `col` of a band row shows, counted as
 *  the spans count it, or null past the row. */
function charAtColumn(row: Cell[], col: number): number | null {
  let chars = 0;
  for (let c = 0; c < row.length; c++) {
    const cell = row[c];
    // The right half of a wide character.
    if (cell.ch === '') continue;
    if (col >= c && col < c + cell.width) return chars;
    chars += Array.from(cell.ch).length;
  }
  return null;
}

/** The piece of your design on the pinned band under a point `x` and `y`
 *  from the dock's top left, or null. `band` is the text the dock shows
 *  with its spans, `zone` the dock's rows and `cell` the terminal's cell.
 *  The dock shows the band's last `zone` rows, so the spans are cut the
 *  same way. */
export function dockPieceAt(
  band: { text: string; spans: readonly PieceSpan[] } | null,
  zone: number,
  cell: CellSize,
  x: number,
  y: number,
): number | null {
  if (!band) return null;
  const rows = Math.max(1, zone);
  const { rows: shown, first } = bandCut(band.text, rows);
  if (shown.length === 0) return null;
  const top = bandRowsTop(rows, shown.length, cell.height);
  const row = Math.floor((y - top) / cell.height);
  if (row < 0 || row >= shown.length) return null;
  const col = Math.floor(x / cell.width);
  const limit = Math.max(1, cell.cols);
  // A row too wide for the terminal ends on an ellipsis in its last cell.
  const usable = shownColumns(shown[row]) > limit ? limit - 1 : limit;
  if (col < 0 || col >= usable) return null;
  const char = charAtColumn(shown[row], col);
  return char === null ? null : pieceAt(band.spans, first + row, char);
}
