// Rows the live pane lends to the pinned prompt band.
//
// While your prompt shows pinned and takes more than one row, as the
// default design does in a fight, the band reaches up over the bottom of
// the terminal pane by the rows past its first. The pane keeps its size
// and the grid gives those rows up: xterm keeps the rows its pane fits
// less the lent ones, and the native grid does the same in
// native_surface/mod.rs. Both renderers keep the newest line on their last
// row, so the line at the top leaves for the scrollback and comes back
// when the band shrinks again.
//
// The game never hears of a lent row. NAWS carries the rows the pane
// holds with a one row band, the same in a fight and out of one. A row
// the band borrows changes no line width, so nothing the game sends would
// wrap differently, and a fight that starts and ends several times a
// second would otherwise send a size each time. Aabahran reads only the
// width, and a game that pages by the height keeps one page length for
// the whole session.

/** The rows the live pane keeps when it fits `fit` and lends `lent` to
 *  the band, at least one. */
export function keptRows(fit: number, lent: number): number {
  return Math.max(1, fit - Math.max(0, lent));
}

/** A window size in cells. */
export interface WindowSize {
  cols: number;
  rows: number;
}

/** The size the game is told for a pane that shows `rows` while it lends
 *  `lent` to the band: the rows it holds with a one row band. */
export function gameSize(cols: number, rows: number, lent: number): WindowSize {
  return { cols, rows: rows + Math.max(0, lent) };
}

/** Which sizes reach the game: a new one each time it changes, and none
 *  for a row the band borrows or gives back. */
export class GameSizeReport {
  private last: string | null = null;

  /** The size to send for a pane `cols` by `rows` that lends `lent`, or
   *  null when the game already has it. */
  next(cols: number, rows: number, lent: number): WindowSize | null {
    const size = gameSize(cols, rows, lent);
    const key = `${size.cols}x${size.rows}`;
    if (key === this.last) return null;
    this.last = key;
    return size;
  }
}
