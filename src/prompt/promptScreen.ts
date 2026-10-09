// The game's own lines on the terminal screen, for the prompt card's
// marks while the profile reads no prompt yet. With no capture Vosh
// draws nothing and opens no row, so the card finds the line the game
// sent in what the renderer shows: as the last text on screen while it
// reads your codes, or every row of the same shape while you point at
// the line on another game. Rows are the screen's rows from its top,
// with trailing blanks gone. Marks count cells, as the renderers place
// them.

import { textCells } from '../terminal/sgrCells';
import type { RawMark } from './promptPieces';
import { wrapBreaks } from '../terminal/wordWrap';

/** What the card asks the marks to find on screen: the game's lines,
 *  as the last text on screen or every row of their shape, and the marks
 *  each place they show takes, from the lines as that place shows them. */
export interface ScreenAsk {
  lines: string[];
  mode: 'tail' | 'shape';
  marks: (shown: string[]) => RawMark[];
}

/** A line of the game's found on screen: the screen row it starts on,
 *  and the text of each line as that row shows it. */
export interface OnScreen {
  row: number;
  lines: string[];
}

/** `line` as the rows the word wrap breaks it into at `cols` wide. */
export function wrappedRows(line: string, cols: number): string[] {
  const rows: string[] = [];
  let from = 0;
  for (const { at, replaced } of wrapBreaks(line, cols)) {
    rows.push(line.slice(from, at));
    from = replaced ? at + 1 : at;
  }
  rows.push(line.slice(from));
  return rows;
}

/** Where `lines` show as the last text on screen, or null when other
 *  text came after them or they are not there. */
export function tailOnScreen(
  rows: readonly string[],
  lines: readonly string[],
  cols: number,
): OnScreen | null {
  const want = lines.flatMap((line) => wrappedRows(line, cols)).map((row) => row.trimEnd());
  while (want.length > 0 && want[want.length - 1] === '') want.pop();
  let last = rows.length - 1;
  while (last >= 0 && rows[last].trimEnd() === '') last -= 1;
  const first = last - want.length + 1;
  if (want.length === 0 || first < 0) return null;
  for (let i = 0; i < want.length; i++) {
    if (rows[first + i].trimEnd() !== want[i]) return null;
  }
  return { row: first, lines: [...lines] };
}

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Every row that starts with a line of `line`'s shape, its numbers
 *  free to differ, with the part of the row the line takes. Your echo may
 *  follow it after the space the game ends it with. Nothing for a blank
 *  line or one wider than the screen. */
export function shapesOnScreen(rows: readonly string[], line: string, cols: number): OnScreen[] {
  const shown = line.trimEnd();
  if (shown === '' || textCells(shown) > cols) return [];
  const body = shown
    .split(/(-?\d+)/)
    .map((part, i) => (i % 2 === 1 ? '-?\\d+' : escape(part)))
    .join('');
  const spaced = line.length > shown.length;
  const shape = new RegExp(`^${body}${spaced ? '(?= |$)' : '$'}`);
  const found: OnScreen[] = [];
  rows.forEach((row, index) => {
    const match = shape.exec(row.trimEnd());
    if (match) found.push({ row: index, lines: [match[0]] });
  });
  return found;
}

/** Each run of digits in `line`, a minus before it included, as
 *  character indexes. */
export function numberRuns(line: string): { start: number; end: number }[] {
  const chars = Array.from(line);
  const runs: { start: number; end: number }[] = [];
  const text = chars.join('');
  for (const match of text.matchAll(/-?\d+/g)) {
    const start = Array.from(text.slice(0, match.index)).length;
    runs.push({ start, end: start + Array.from(match[0]).length });
  }
  return runs;
}

/** The accent tint over each line whole. */
export function wholeMarks(lines: readonly string[]): RawMark[] {
  return lines
    .map((line, row) => ({ row, col: 0, width: textCells(line.trimEnd()), warn: false }))
    .filter((mark) => mark.width > 0);
}

/** The accent tint over each number of `line` that `keep` names by
 *  its place among the numbers. */
export function numberMarks(line: string, keep: readonly boolean[]): RawMark[] {
  const chars = Array.from(line);
  return numberRuns(line)
    .filter((_, k) => keep[k] ?? false)
    .map((run) => ({
      row: 0,
      col: textCells(chars.slice(0, run.start).join('')),
      width: textCells(chars.slice(run.start, run.end).join('')),
      warn: false,
    }));
}

/** Where `ask` finds the game's lines on `rows`, `cols` wide. */
export function findOnScreen(rows: readonly string[], ask: ScreenAsk, cols: number): OnScreen[] {
  if (ask.mode === 'shape') return shapesOnScreen(rows, ask.lines[0] ?? '', cols);
  const tail = tailOnScreen(rows, ask.lines, cols);
  return tail ? [tail] : [];
}
