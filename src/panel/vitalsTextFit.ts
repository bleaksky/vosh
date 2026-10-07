import type { PromptRendered, PromptSpan } from '../ipc/promptDesign';
import type { VitalsText } from '../ipc/vitals';
import { parseSgrCells, type Cell } from '../terminal/sgrCells';

// How the Text style fits your vitals text to the footer (Vitals Styles
// Q8). The session renders the text at the live values and again with
// each vital at its max and your opponent at 100, both for the
// footer's width in cells. Each row wraps where the full render wraps,
// at the same spaces, so a value that loses a digit in a fight never
// moves a line. A row with %{right} never wraps: what follows the push
// stays whole and the part before it ends in an ellipsis where they
// meet, which the footer's CSS draws.

/** One row of a render as cells, with where its %{right} pushes. */
export interface TextRow {
  cells: Cell[];
  /** The push's column and the spaces it took, or null. */
  push: { col: number; width: number } | null;
}

/** One line the footer draws: the cells before a push, and the cells
 *  after it, or null on a line with no push. */
export interface TextLine {
  left: Cell[];
  right: Cell[] | null;
}

/** A cell with the piece of your text that drew it, for the rings the
 *  vitals text card draws on the footer. */
export interface PieceCell extends Cell {
  piece?: number;
}

/** `rows` with each cell a span of `spans` covers marked with its
 *  piece. Cutting a row into lines keeps the marks. */
export function withPieces(rows: readonly TextRow[], spans: readonly PromptSpan[]): TextRow[] {
  return rows.map((row, r) => {
    const cells: PieceCell[] = row.cells.map((cell) => ({ ...cell }));
    for (const span of spans) {
      if (span.row !== r) continue;
      for (let col = span.col; col < span.col + span.width && col < cells.length; col += 1) {
        cells[col].piece = span.piece;
      }
    }
    return { ...row, cells };
  });
}

/** The rows of `rendered` as cells. `right` names the pieces that are a
 *  %{right}, and a row's first span of one is its push. */
export function textRows(rendered: PromptRendered, right: readonly number[]): TextRow[] {
  if (rendered.rows === 0) return [];
  const cells = parseSgrCells(rendered.ansi).slice(0, rendered.rows);
  return cells.map((row, r) => {
    const span = rendered.spans.find((s) => s.row === r && right.includes(s.piece));
    return { cells: row, push: span ? { col: span.col, width: span.width } : null };
  });
}

/** The words of a row as their first column and the column past their
 *  last, split at runs of spaces. */
function words(cells: readonly Cell[]): { start: number; end: number }[] {
  const found: { start: number; end: number }[] = [];
  let start = -1;
  cells.forEach((cell, col) => {
    const space = cell.ch === ' ';
    if (!space && start < 0) start = col;
    if (space && start >= 0) {
      found.push({ start, end: col });
      start = -1;
    }
  });
  if (start >= 0) found.push({ start, end: cells.length });
  return found;
}

/** Where a row of `cells` wraps at `cols` cells: for each line after the
 *  first, the word it starts on. A word wider than the line keeps a line
 *  of its own. */
export function wrapAt(cells: readonly Cell[], cols: number): number[] {
  const all = words(cells);
  const starts: number[] = [];
  let from = 0;
  all.forEach((word, i) => {
    if (i > 0 && word.end - from > cols) {
      starts.push(i);
      from = word.start;
    }
  });
  return starts;
}

/** `cells` cut into lines, each starting on one of the words `starts`
 *  names, the spaces between them dropped. */
function cutAt(cells: Cell[], starts: readonly number[]): Cell[][] {
  const all = words(cells);
  const lines: Cell[][] = [];
  let from = 0;
  for (const start of starts) {
    lines.push(cells.slice(from, all[start - 1].end));
    from = all[start].start;
  }
  lines.push(cells.slice(from));
  return lines;
}

/** The lines of each live row, at `cols` cells, laid out from `full`. */
export function fitText(
  live: readonly TextRow[],
  full: readonly TextRow[],
  cols: number,
): TextLine[][] {
  return live.map((row, r) => {
    const { cells, push } = row;
    if (push) {
      return [{ left: cells.slice(0, push.col), right: cells.slice(push.col + push.width) }];
    }
    const wide = full[r]?.cells;
    // The full row names the spaces to break at, while it holds the same
    // words, which it does unless a value with spaces in it changed.
    const lay = wide && words(wide).length === words(cells).length ? wide : cells;
    return cutAt(cells, wrapAt(lay, cols)).map((left) => ({ left, right: null }));
  });
}

/** The lines the footer draws of `text` at `cols` cells. `fightOnly`
 *  keeps only the rows that read your fight, for Hide vitals while your
 *  prompt is pinned (Q9), with everything else you wrote on them. With
 *  `pieces` each cell carries the piece that drew it. */
export function vitalsTextLines(
  text: VitalsText,
  cols: number,
  fightOnly: boolean,
  pieces = false,
): TextLine[] {
  const rows = textRows(text.live, text.right);
  const live = pieces ? withPieces(rows, text.live.spans) : rows;
  const full = textRows(text.full, text.right);
  return fitText(live, full, cols).flatMap((lines, r) =>
    !fightOnly || text.fight[r] ? lines : [],
  );
}

/** The pieces the status line writes of `text` on its one line: each
 *  row, split in two where its %{right} pushes, with the push and the
 *  spaces round each piece dropped and blank pieces left out. The line
 *  sets a 20 px gap between pieces. */
export function vitalsTextPieces(text: VitalsText): Cell[][] {
  return textRows(text.live, text.right)
    .flatMap(({ cells, push }) =>
      push ? [cells.slice(0, push.col), cells.slice(push.col + push.width)] : [cells],
    )
    .map(trimSpaces)
    .filter((piece) => piece.length > 0);
}

function trimSpaces(cells: Cell[]): Cell[] {
  let start = 0;
  let end = cells.length;
  while (start < end && cells[start].ch === ' ') start += 1;
  while (end > start && cells[end - 1].ch === ' ') end -= 1;
  return cells.slice(start, end);
}

/** The footer's sides, 18 px at the left and 12 at the right. */
const SIDES_PX = 30;

/** The cells of `face` at `size` px a footer `width` px wide holds
 *  inside its sides. */
export function textCols(width: number, face: string, size: number): number {
  return Math.max(1, Math.floor((width - SIDES_PX) / cellWidth(face, size)));
}

let measureCanvas: HTMLCanvasElement | null = null;

/** One cell of `face` at `size` px, or 0.6 of the size where nothing
 *  can measure. */
function cellWidth(face: string, size: number): number {
  const guess = size * 0.6;
  if (typeof document === 'undefined') return guess;
  measureCanvas ??= document.createElement('canvas');
  const ctx = measureCanvas.getContext?.('2d');
  if (!ctx) return guess;
  ctx.font = `${size}px ${face}`;
  const width = ctx.measureText('0000000000').width / 10;
  return width > 0 ? width : guess;
}
