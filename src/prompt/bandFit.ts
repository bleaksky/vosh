import { PLAIN, shownColumns, type Cell } from '../terminal/sgrCells';
import { bandCut } from './pinnedDock';

// How the pinned band fits each row of your prompt to the columns it
// has. The session draws a row with %{right} out to the width the game
// is told, and the band lays it out again on the terminal's own
// columns, so the part after the push always ends on the band's last
// column. When the row is too wide, the gap the push put in shrinks
// first, down to one space. Only when the part before the push and the
// part after it cannot both fit does an ellipsis show, and then the
// part before the push gives way: it ends on the ellipsis and the part
// after it stays whole, as your vitals text in the footer does
// (src/panel/vitalsTextFit.ts). A part after the push too wide for the
// band on its own, and any row without a push, ends on an ellipsis in
// the band's last column.
//
// James saw the right part cut short with room to spare on 2026-10-08.
// The band cut each row at the terminal's columns while the session had
// pushed it out to the columns the game was told, which the native grid
// counted wider, so the end of the right part went under an ellipsis
// with a wide gap still in the middle.

/** Where a piece of your design lands on the band, as the session
 *  sends it. `push` marks the spaces a %{right} put in. */
export interface BandSpan {
  piece: number;
  row: number;
  col: number;
  width: number;
  push?: boolean;
}

/** One row as the band draws it. */
export interface BandLine {
  cells: Cell[];
  /** The column the ellipsis takes, or null when the row shows whole. */
  more: number | null;
}

/** Where a fitted row puts the columns of the row as the session drew
 *  it: those before `cut` stay, those at `from` and past it move by
 *  `shift`, and the rest the band does not draw. */
interface Moves {
  cut: number;
  from: number;
  shift: number;
  /** The column past the last one the band draws a piece on. */
  end: number;
  /** Where the push's spaces sit now, or null when it has none. */
  gap: { col: number; width: number } | null;
}

/** The first `cols` columns of `cells`, less a wide character that
 *  would reach past them. */
function head(cells: Cell[], cols: number): Cell[] {
  const kept = cells.slice(0, Math.max(0, cols));
  const last = kept[kept.length - 1];
  if (last && last.width === 2 && kept.length === cols) kept.pop();
  return kept;
}

/** `width` spaces in the look the push's first cell had, so a ground
 *  that runs through the gap still does. */
function spaces(width: number, like: Cell | undefined): Cell[] {
  const attrs = like?.attrs ?? PLAIN;
  return Array.from({ length: Math.max(0, width) }, () => ({ ch: ' ', width: 1, attrs }));
}

/** A row cut to end on an ellipsis in the last of `limit` columns. */
function cutEnd(cells: Cell[], limit: number): BandLine {
  return { cells: head(cells, limit - 1), more: limit - 1 };
}

/** One row of the band at `limit` columns, with where its columns
 *  went. `push` is the column the push starts on and the spaces it
 *  took. */
function fitRow(
  row: Cell[],
  push: { col: number; width: number } | null,
  limit: number,
): { line: BandLine; moves: Moves } {
  const total = shownColumns(row);
  const keep: Moves = { cut: Infinity, from: Infinity, shift: 0, end: Infinity, gap: null };
  if (!push) {
    if (total <= limit) return { line: { cells: row, more: null }, moves: keep };
    return {
      line: cutEnd(row, limit),
      moves: { ...keep, cut: head(row, limit - 1).length, end: limit - 1 },
    };
  }
  const left = row.slice(0, push.col);
  const from = push.col + push.width;
  const right = row.slice(from, from + shownColumns(row.slice(from)));
  const like = row[push.col];
  // The part before the push as written, or without the spaces it ends
  // in when that is what it takes to fit.
  const leftCols = left.length + 1 + right.length <= limit ? left.length : shownColumns(left);
  if (leftCols + 1 + right.length <= limit) {
    const gap = limit - leftCols - right.length;
    return {
      line: { cells: [...left.slice(0, leftCols), ...spaces(gap, like), ...right], more: null },
      moves: {
        cut: leftCols,
        from,
        shift: leftCols + gap - from,
        end: limit,
        gap: { col: leftCols, width: gap },
      },
    };
  }
  // The part before the push gives way: it ends on the ellipsis, one
  // space before the part after it, which stays whole.
  const room = limit - right.length - 2;
  if (room >= 1) {
    const kept = head(left, room);
    const more = shownColumns(kept);
    if (more > 0) {
      const gap = limit - right.length - more - 1;
      return {
        line: {
          cells: [...kept.slice(0, more), ...spaces(1 + gap, like), ...right],
          more,
        },
        moves: {
          cut: more,
          from,
          shift: limit - right.length - from,
          end: limit,
          gap: { col: more + 1, width: gap },
        },
      };
    }
  }
  // The part after the push is too wide on its own: the row as the
  // push left it at its narrowest ends on an ellipsis.
  const narrow = [...left.slice(0, leftCols), ...spaces(1, like), ...right];
  const line = cutEnd(narrow, limit);
  const cut = line.cells.length;
  return {
    line,
    moves: {
      cut: Math.min(cut, leftCols),
      from,
      shift: leftCols + 1 - from,
      end: limit - 1,
      gap: { col: leftCols, width: 1 },
    },
  };
}

/** A span moved to where the band draws its cells, or null when the
 *  band draws none of them. */
function moveSpan<S extends BandSpan>(span: S, moves: Moves): S | null {
  const clip = (col: number, end: number): S | null => {
    const to = Math.min(end, moves.end);
    return to > col ? { ...span, col, width: to - col } : null;
  };
  if (span.push && moves.gap) return clip(moves.gap.col, moves.gap.col + moves.gap.width);
  if (span.col >= moves.from) {
    return clip(span.col + moves.shift, span.col + span.width + moves.shift);
  }
  return clip(span.col, Math.min(span.col + span.width, moves.cut));
}

/** The band for `text` at `cols` columns: the last `zone` rows it shows,
 *  the pinned prompt's row it starts on, and `spans` moved to where the
 *  band draws their pieces, their rows still the pinned prompt's. */
export function fitBand<S extends BandSpan>(
  text: string,
  spans: readonly S[],
  zone: number,
  cols: number,
): { lines: BandLine[]; first: number; spans: S[] } {
  const limit = Math.max(1, cols);
  const { rows, first } = bandCut(text, Math.max(1, zone));
  const fits = rows.map((row, r) => {
    const push = spans.find((s) => s.push && s.row === first + r) ?? null;
    return fitRow(row, push && { col: push.col, width: push.width }, limit);
  });
  const moved: S[] = [];
  for (const span of spans) {
    const fit = fits[span.row - first];
    if (!fit) {
      moved.push(span);
      continue;
    }
    const next = moveSpan(span, fit.moves);
    if (next) moved.push(next);
  }
  return { lines: fits.map((f) => f.line), first, spans: moved };
}

/** The columns a fitted row shows, its ellipsis and a right part after
 *  it included. */
export function lineColumns(line: BandLine): number {
  return Math.max(shownColumns(line.cells), line.more === null ? 0 : line.more + 1);
}

/** The fitted rows as plain text, one per row, a wide character once
 *  and the ellipsis left out, as bandPlain writes the rows. */
export function linesPlain(lines: readonly BandLine[]): string[] {
  return lines.map((line) =>
    line.cells
      .filter((c) => c.ch !== '')
      .map((c) => c.ch)
      .join(''),
  );
}
