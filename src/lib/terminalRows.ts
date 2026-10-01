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
//
// Both renderers run fixtures/terminal-rows/cases.json, so keep them in
// step.

/** The rows the live pane keeps when it fits `fit` and lends `lent` to
 *  the band, at least one. */
export function keptRows(fit: number, lent: number): number {
  return Math.max(1, fit - Math.max(0, lent));
}

// While your prompt shows pinned the grid also keeps to the bottom of its
// pane. A pane is rarely a whole number of rows tall, and the pixels left
// over used to sit under the last row, between your newest line and the
// band, up to a row of them. That read as the very blank line the band
// had lost. They go above the first row instead, so the newest line sits
// the dock's 6 px gap over the band in any window.

/** How far xterm sits down its pane while it keeps to the bottom: the CSS
 *  px a pane `paneH` tall leaves over under `fitRows` rows of `cellH`, cut
 *  to whole device pixels at `dpr` so the text stays sharp. Never up. */
export function spareAbove(paneH: number, fitRows: number, cellH: number, dpr: number): number {
  const spare = Math.max(0, paneH - fitRows * cellH);
  return Math.floor(spare * dpr + 1e-6) / dpr;
}

/** The native grid's bounds while it keeps to the bottom of its pane: the
 *  pane's `top` and `height` in CSS px, moved down by the device pixels
 *  its rows of `cellPx` leave over, counted as the grid counts them, so
 *  it fits the same rows and its bottom stays the pane's. */
export function nativeBottomBounds(
  top: number,
  height: number,
  dpr: number,
  cellPx: number,
): { top: number; height: number; spare: number } {
  if (cellPx <= 0) return { top, height, spare: 0 };
  const px = Math.round(height * dpr);
  const spare = (px - Math.floor(px / cellPx) * cellPx) / dpr;
  return { top: top + spare, height: height - spare, spare };
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

// The live pane follows its newest rows, after each output and each
// resize. xterm keeps a pane that shows them there through a row resize on
// its own, the newest line on the last row. Its scrollbar takes the new
// rows in only on the next frame, though, and a scroll asked of it before
// then is measured on the old rows: at 2x it lands a row short and leaves
// the newest line just under the screen. A fight that lends a row resizes
// xterm in the same task its text lands in, so the pane asks for a scroll
// only when it has really left its newest rows.

/** The parts of xterm the live pane's tail needs. */
export interface TailView {
  buffer: { active: { viewportY: number; baseY: number } };
  scrollToBottom(): void;
}

/** Brings the live pane back to its newest rows when it has left them,
 *  and asks nothing of xterm while it shows them. */
export function keepTail(view: TailView): void {
  const b = view.buffer.active;
  if (b.viewportY !== b.baseY) view.scrollToBottom();
}
