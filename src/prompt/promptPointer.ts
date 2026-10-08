// Which piece of your prompt a pointer is on.
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
//
// Spans count cells, as the band does: a wide character takes two and a
// combining mark none (cellWidth in sgrCells.ts).

import { bandCut, bandRowsTop, type CellSize } from './pinnedDock';
import { cellWidth, parseSgrCells, shownColumns } from '../terminal/sgrCells';
import type { PromptSpan } from '../ipc/promptDesign';
import type { TerminalCursor } from '../ipc/terminal';
import { wrapBreaks } from '../terminal/wordWrap';

/** The part of a span the mapping reads: the piece, the row from `%nl`,
 *  and the cells it covers in that row before any wrap. */
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
 *  wrap broke at takes none, and neither does a combining mark. `cell` is
 *  its cell in its row before any wrap, as the spans count cells, a
 *  combining mark on the cell it joins. */
export interface Placed {
  row: number;
  col: number;
  width: number;
  cell: number;
}

/** Lay out `plain`, the rows of a drawn prompt joined by `\n`, from
 *  column `startCol` of a terminal `cols` wide: one entry per character.
 *  Each row is word wrapped as the renderers wrap it, counting from the
 *  region's start, and the terminal moves a character that does not fit
 *  to the next row. A row after a line break starts at the first column. */
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
    // The row's cells before any wrap, as the spans count them.
    let logical = 0;
    let joined = 0;
    for (let i = 0; i < text.length; ) {
      const code = text.codePointAt(i) ?? 0;
      const units = code > 0xffff ? 2 : 1;
      const cells = cellWidth(code);
      const cell = cells === 0 ? joined : logical;
      joined = cell;
      logical += cells;
      let gone = false;
      // The word wrap's CRLF goes before this character, or in its place.
      while (next < breaks.length && breaks[next].at < i + units) {
        gone = gone || breaks[next].replaced;
        next += 1;
        row += 1;
        col = 0;
      }
      if (gone) {
        placed.push({ row, col, width: 0, cell });
        i += units;
        continue;
      }
      if (cells === 0) {
        // A combining mark joins the cell before it.
        placed.push({ row, col: Math.max(0, col - 1), width: 0, cell });
        i += units;
        continue;
      }
      if (col + cells > width) {
        row += 1;
        col = 0;
      }
      placed.push({ row, col, width: cells, cell });
      col += cells;
      i += units;
    }
    return placed;
  });
}

/** The piece whose span covers cell `cell` of row `row`, counted before
 *  any wrap, or null. */
export function pieceAt(spans: readonly PieceSpan[], row: number, cell: number): number | null {
  for (const span of spans) {
    if (span.row === row && cell >= span.col && cell < span.col + span.width) return span.piece;
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
    for (const p of layout[r]) {
      if (p.width > 0 && p.row === row && cell.col >= p.col && cell.col < p.col + p.width) {
        return pieceAt(open.spans, r, p.cell);
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

/** The cell under a point in client px, from the top left of a grid
 *  `rect` whose cells are `cell` in size, or null outside the grid. */
export function cellInGrid(
  clientX: number,
  clientY: number,
  rect: { left: number; top: number; width: number; height: number },
  cell: { width: number; height: number },
): ScreenCell | null {
  const x = clientX - rect.left;
  const y = clientY - rect.top;
  if (x < 0 || y < 0 || x >= rect.width || y >= rect.height) return null;
  if (cell.width <= 0 || cell.height <= 0) return null;
  return cellAtPoint(x, y, cell);
}

/** The open region on screen as the native grid reports it. Its lines
 *  count from the top of the live screen, which is the screen's own top
 *  while you are at the bottom. */
export function regionFromCursor(cursor: TerminalCursor | null): RegionOnScreen | null {
  if (!cursor?.region) return null;
  return {
    gen: cursor.region.gen,
    row: cursor.region.line,
    col: cursor.region.col,
    cols: cursor.cols,
    atBottom: cursor.at_bottom,
  };
}

/** The open region on screen as xterm holds it: its buffer row less the
 *  top of the viewport. */
export function regionFromXterm(
  start: { gen: number; row: number; col: number } | null,
  buffer: { viewportY: number; baseY: number },
  cols: number,
): RegionOnScreen | null {
  if (!start) return null;
  return {
    gen: start.gen,
    row: start.row - buffer.viewportY,
    col: start.col,
    cols,
    atBottom: buffer.viewportY === buffer.baseY,
  };
}

/** The characters piece `piece` covers in `plain`, the rows of a drawn
 *  prompt joined by `\n`, a line break between rows it spans. Empty for
 *  a piece that drew nothing. */
export function pieceText(plain: string, spans: readonly PieceSpan[], piece: number): string {
  const rows = plain.split('\n').map((row) => {
    // Each character with its cell before any wrap, a combining mark on
    // the cell it joins.
    let logical = 0;
    let joined = 0;
    return Array.from(row).map((ch) => {
      const cells = cellWidth(ch.codePointAt(0) ?? 0);
      const cell = cells === 0 ? joined : logical;
      joined = cell;
      logical += cells;
      return { ch, cell };
    });
  });
  const own = spans.filter((s) => s.piece === piece).sort((a, b) => a.row - b.row || a.col - b.col);
  let out = '';
  let last: number | null = null;
  for (const span of own) {
    if (last !== null && span.row !== last) out += '\n';
    for (const { ch, cell } of rows[span.row] ?? []) {
      if (cell >= span.col && cell < span.col + span.width) out += ch;
    }
    last = span.row;
  }
  return out;
}

/** The row and column of the pinned band under a point `x` and `y` from
 *  the dock's top left, the row counted from the band's first shown row,
 *  or null off its rows. `zone` is the dock's rows and `cell` the
 *  terminal's cell. */
export function dockCellAt(
  text: string,
  zone: number,
  cell: CellSize,
  x: number,
  y: number,
): ScreenCell | null {
  const rows = Math.max(1, zone);
  const shown = bandCut(text, rows).rows.length;
  if (shown === 0 || x < 0) return null;
  const at = cellAtPoint(x, y - bandRowsTop(rows, shown, cell.height), cell);
  return at.row >= 0 && at.row < shown ? at : null;
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
  const at = dockCellAt(band.text, zone, cell, x, y);
  if (!at) return null;
  const { rows: shown, first } = bandCut(band.text, Math.max(1, zone));
  const limit = Math.max(1, cell.cols);
  // A row too wide for the terminal ends on an ellipsis in its last cell.
  const usable = shownColumns(shown[at.row]) > limit ? limit - 1 : limit;
  if (at.col >= usable) return null;
  // The band puts each cell at its own column, so a column is a cell of
  // the row before any wrap, as the spans count them.
  return pieceAt(band.spans, first + at.row, at.col);
}

/** The band's rows as plain text joined by `\n`, combining marks with the
 *  character they join, for naming what a piece covers. */
export function bandPlain(text: string): string {
  return parseSgrCells(text)
    .map((row) =>
      row
        .filter((c) => c.ch !== '')
        .map((c) => c.ch)
        .join(''),
    )
    .join('\n');
}
