import type { RegionOnScreen, ScreenCell } from '../prompt/promptPointer';
import type { BufferView, LineMark } from './splitDrag';

export interface FindOptions {
  /** Treat the term as a regex. Default false (plain substring). */
  regex?: boolean;
  /** Whole-word match. Default false. */
  wholeWord?: boolean;
  /** Case-sensitive search. Default false. */
  caseSensitive?: boolean;
}

export interface TerminalHandle {
  write: (data: Uint8Array | string) => void;
  /** The newest output of the prompt stage this terminal took, 0 before
   *  the first. Text written after it follows it, which the session reads
   *  to tell which prompt the text closes. */
  outputTaken: () => number;
  fit: () => void;
  focus: () => void;
  clear: () => void;
  /** Scroll the xterm scrollback by N pages. Negative N scrolls up. */
  scrollPages: (n: number) => void;
  /** Scroll the xterm scrollback by N lines. Negative N scrolls up. */
  scrollLines: (n: number) => void;
  /** Jump the viewport to the live tail. */
  scrollToBottom: () => void;
  /** True when the viewport is anchored at the live tail (no scrollback offset). */
  isAtBottom: () => boolean;
  /** Current cols × rows the pane shows. */
  getSize: () => { cols: number; rows: number };
  /** The size the game is told through NAWS: the rows the pane shows
   *  plus the rows it lends to the pinned prompt band. The host pushes it
   *  after a (re)connect. */
  windowSize: () => { cols: number; rows: number };
  /** Force the renderer to redraw the visible rows. The split-scrollback
   *  history pane can mount sized and positioned on real content yet the
   *  DOM renderer leaves it blank until the next scroll triggers a draw;
   *  calling this after it settles paints it immediately. */
  refresh: () => void;
  /** A snapshot of the xterm internals. The history split reads
   *  `bufferLength` and `rows` from it to know when its pane holds real
   *  content. */
  debug: () => {
    rows: number;
    cols: number;
    viewportY: number;
    baseY: number;
    bufferLength: number;
    hostW: number;
    hostH: number;
    webgl: boolean;
  };
  /** Height of one terminal cell in CSS pixels, derived from the
   *  host's pixel height divided by the current row count. Used
   *  by the split-scrollback Resizable to snap the divider to
   *  row boundaries so it never lands mid-row and clips a half
   *  line of content. */
  cellHeight: () => number;
  /** Search forward from the current selection (or top of buffer). Returns
   *  true when a match was found and scrolled into view. Highlights every
   *  match across the entire scrollback as a side effect. */
  findNext: (term: string, options?: FindOptions) => boolean;
  /** Search backward. Same return + decoration semantics as findNext. */
  findPrevious: (term: string, options?: FindOptions) => boolean;
  /** Clear search decorations (called when the find toolbar closes). */
  clearSearch: () => void;
  /** Drop any current selection. Used to enforce "one selection across
   *  panes" when the split is open — when one pane gets a selection
   *  the other pane's selection is cleared. */
  clearSelection: () => void;
  /** True when this pane currently has a non-empty selection. */
  hasSelection: () => boolean;
  /** Subscribe to selection-change events on this terminal. Returns
   *  an unsubscribe function. */
  onSelectionChange: (cb: () => void) => () => void;
  /** Current selected text, or empty string when no selection. */
  getSelection: () => string;
  /** Select `length` cells from `column` of buffer row `row`, rows
   *  counted whole. A drag across the scrollback split selects through
   *  it (src/terminal/splitDrag.ts). */
  select: (column: number, row: number, length: number) => void;
  /** The buffer as a drag reads it: the size, where the viewport and the
   *  bottom page start, and the cursor's row on the screen. */
  bufferView: () => BufferView;
  /** Keep track of buffer row `row` through new output and trimmed
   *  history, or null on the alternate screen. */
  markLine: (row: number) => LineMark | null;
  /** Buffer row `row` as text with its trailing blanks gone, or null past
   *  the buffer. A drag across the split finds its line by it. */
  lineText: (row: number) => string | null;
  /** Select the whole buffer, scrollback included. */
  selectAll: () => void;
  /** Where the open region starts on this pane's screen, as the renderer
   *  that draws it holds it: the native grid through terminal_cursor, or
   *  xterm from the line its mark came on. Null while no region is open.
   *  The prompt card lays your prompt out from it to map a pointer to a
   *  piece. */
  promptRegion: () => Promise<RegionOnScreen | null>;
  /** The screen's rows as text from its top, each with trailing blanks
   *  gone, as the renderer in use holds them, with its width and whether
   *  it shows the live tail. The prompt card finds the game's own line in
   *  them while the profile reads no prompt. Null before it has a size. */
  screenRows: () => Promise<{ rows: string[]; cols: number; atBottom: boolean } | null>;
  /** The screen cell under a point in client px, on the grid the
   *  renderer in use draws, or null outside it. */
  cellAt: (clientX: number, clientY: number) => ScreenCell | null;
  /** The client y of the top of screen row `row`, from the top of the
   *  visible screen, on the grid the renderer in use draws, or null
   *  before it has a size. The prompt card sits over your prompt by it. */
  rowTop: (row: number) => number | null;
  /** The cell grid the renderer in use draws: the top left of its first
   *  cell in client px and the size of a cell, or null before it has a
   *  size. The prompt card puts its marks on your prompt by it. */
  grid: () => { left: number; top: number; cellW: number; cellH: number } | null;
}
