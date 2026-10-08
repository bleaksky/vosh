// The parts of your design no value fills, which the card rings on your
// prompt and Settings rings in its preview, in --warn: a name Vosh does
// not know, and a value only a code of your prompt in the game sends
// while your prompt leaves that code out and no package sends it.

import { needsCode } from './pickerRows';
import type { PromptFieldState } from '../ipc/prompt';
import type { PromptPiece, PromptSpan, PromptToken } from '../ipc/promptDesign';

/** The pieces of a design to ring. */
export function warnedPieces(
  pieces: readonly PromptPiece[],
  tokens: readonly PromptToken[],
  catalog: readonly PromptFieldState[],
): Set<number> {
  const out = new Set<number>();
  const unknown = new Set(tokens.filter((t) => !t.known).map((t) => t.piece));
  for (const piece of pieces) {
    if (!piece.shows) continue;
    if (unknown.has(piece.piece)) out.add(piece.piece);
    // `aff:sanctuary` is a field of `aff`.
    const name = piece.field ? piece.field.split(':')[0] : null;
    const entry = name ? catalog.find((f) => f.name === name) : undefined;
    if (entry && entry.state !== 'value' && needsCode(entry)) out.add(piece.piece);
  }
  return out;
}

/** Where a drawing puts its cells: the left of its first cell, the top of
 *  its first row, a cell's width and a row's height, in px. */
export interface CellGrid {
  x: number;
  y: number;
  cellW: number;
  rowH: number;
}

/** The ring round each run of cells a ringed piece draws. */
export function warnBoxes(
  spans: readonly Pick<PromptSpan, 'piece' | 'row' | 'col' | 'width'>[],
  warn: ReadonlySet<number>,
  grid: CellGrid,
): { left: number; top: number; width: number; height: number }[] {
  const round = (n: number) => Math.round(n * 100) / 100;
  return spans
    .filter((s) => warn.has(s.piece) && s.width > 0)
    .map((s) => ({
      left: round(grid.x + s.col * grid.cellW),
      top: round(grid.y + s.row * grid.rowH),
      width: round(s.width * grid.cellW),
      height: grid.rowH,
    }));
}
