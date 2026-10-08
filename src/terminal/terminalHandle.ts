import type { ISearchOptions, SearchAddon } from '@xterm/addon-search';
import type { Terminal } from '@xterm/xterm';
import { terminalCursor, terminalScreenRows } from '../ipc/terminal';
import {
  cellInGrid,
  regionFromCursor,
  regionFromXterm,
  type RegionOnScreen,
  type ScreenCell,
} from '../prompt/promptPointer';
import { solidFindMarks } from '../theme/findMarks';
import { getCurrentThemeId } from '../theme/theme';
import { findTheme } from '../theme/themes';
import type { PaneSizer } from './paneSizer';
import type { BufferView, LineMark } from './splitDrag';
import type { RegionWriter } from './terminalRegion';
import { nativeSurfaceEnabled } from './terminalRenderer';
import { gameSize } from './terminalRows';

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
  /** The rows the pane shows and the lines its buffer holds. The history
   *  split reads them to know when its pane holds real content. */
  contentSize: () => { rows: number; bufferLength: number };
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

/** What a pane's handle reads. The setup effect replaces the region writer
 *  when the xterm copy fills anew, so what goes through the writer comes
 *  as functions. */
export interface HandleParts {
  term: Terminal;
  searchAddon: SearchAddon;
  paneSizer: PaneSizer;
  /** The wrapper the layout sizes, which the native grid draws in. */
  sizer: HTMLDivElement | null;
  /** The element xterm opened in. */
  host: HTMLDivElement;
  /** Writes local text through the mirror and the writer in use now. */
  write: TerminalHandle['write'];
  outputTaken(): number;
  /** The open region as the writer in use holds it. */
  region(): ReturnType<RegionWriter['region']>;
  /** Whether this is the split's history pane. */
  quiet(): boolean;
  /** Rows lent to the pinned prompt band. */
  lent(): number;
  /** The session whose terminal this is, whose native grid the prompt
   *  card reads. */
  session: number;
}

// The ground under the text as #rrggbb. While the pane lifts your prompt
// the terminal's ground is the theme's own color with a zero alpha byte, so
// that byte is dropped. A ground that does not parse is the theme's.
const solidGround = (background: string | undefined, theme: string): string => {
  const m = /^#([0-9a-f]{6})(?:[0-9a-f]{2})?$/i.exec(background ?? '');
  return m ? `#${m[1]}` : theme;
};

/** How a find marks matches in an xterm terminal. */
export const searchDecorations = (
  term: Pick<Terminal, 'options'>,
): NonNullable<ISearchOptions['decorations']> => {
  // Every match fills in the theme's ANSI yellow at 28% and the match you
  // are on at 60%, the way Help, the session logs page and the native grid
  // mark them. The search addon takes only #rrggbb, so the fills are laid
  // over the terminal's own ground. The addon registers the active
  // match on the top layer, but the renderer resolves a decoration
  // backgroundColor as the cell background, so the glyphs still paint over
  // the stronger fill and stay legible. The theme is read at each find, so
  // a theme switch shows on the next one.
  const { xterm } = findTheme(getCurrentThemeId());
  const ground = solidGround(term.options.theme?.background, xterm.background);
  // Theme yellows are always #rrggbb, so the marks always build. The
  // yellow itself stands in only to satisfy the addon's types.
  const marks = solidFindMarks(xterm, ground) ?? { match: xterm.yellow, current: xterm.yellow };
  return {
    matchBackground: marks.match,
    matchOverviewRuler: xterm.yellow,
    activeMatchBackground: marks.current,
    activeMatchColorOverviewRuler: xterm.yellow,
  };
};

/** The handle a terminal pane gives its host once it is set up. */
export function terminalHandle(parts: HandleParts): TerminalHandle {
  const {
    term,
    searchAddon,
    paneSizer,
    sizer,
    host,
    write,
    outputTaken,
    region,
    quiet,
    lent,
    session,
  } = parts;
  // The cell grid the renderer in use draws, in client px: the box it
  // fills and the size of a cell, or null before it has a size. The
  // native grid draws from the pane's top left, below the pixels its
  // rows leave over, each cell xterm's device cell rounded to whole
  // pixels.
  const gridBox = () => {
    if (!quiet() && nativeSurfaceEnabled()) {
      const device = term.dimensions?.device?.cell;
      if (!sizer || !device?.width || !device?.height) return null;
      const dpr = window.devicePixelRatio || 1;
      const r = sizer.getBoundingClientRect();
      return {
        left: r.left,
        top: r.top + paneSizer.nativeSpare,
        width: r.width,
        height: r.height - paneSizer.nativeSpare,
        cellW: Math.round(device.width) / dpr,
        cellH: Math.round(device.height) / dpr,
      };
    }
    const screen = host?.querySelector('.xterm-screen');
    const cell = term.dimensions?.css?.cell;
    if (!screen || !cell?.width || !cell?.height) return null;
    const r = screen.getBoundingClientRect();
    return {
      left: r.left,
      top: r.top,
      width: r.width,
      height: r.height,
      cellW: cell.width,
      cellH: cell.height,
    };
  };
  return {
    write,
    outputTaken,
    fit: () => paneSizer.fitKept(),
    focus: () => term.focus(),
    clear: () => term.clear(),
    scrollPages: (n) => term.scrollPages(n),
    scrollLines: (n) => term.scrollLines(n),
    scrollToBottom: () => term.scrollToBottom(),
    refresh: () => {
      if (term.rows > 0) term.refresh(0, term.rows - 1);
    },
    contentSize: () => ({
      rows: term.rows,
      bufferLength: term.buffer.active.length,
    }),
    // viewportY tracks the top of the viewport in scrollback coords;
    // baseY tracks the top of the bottom page. Equal means the
    // viewport is anchored to the live tail.
    isAtBottom: () => term.buffer.active.viewportY === term.buffer.active.baseY,
    getSize: () => ({ cols: term.cols, rows: term.rows }),
    windowSize: () => gameSize(term.cols, term.rows, lent()),
    cellHeight: () => {
      // Read the host's pixel height (set by sync), divide by
      // xterm's current row count, and round UP to whole pixels.
      // xterm does not expose actual cell height in its public
      // API; this derivation matches FitAddon's own row math.
      // Ceiling matters: the snap pitch is the wrapper-height
      // delta per row. With fractional cellHeight the wrapper
      // height between two snap points is `N * cellHeight`, but
      // the DOM rounds it to whole pixels — drag near a snap
      // boundary then oscillates between two pixel rows of
      // remainder space at the bottom of xterm and the line near
      // the divider looks like its height is changing. Ceiling
      // guarantees `N * snap > N * actualCellHeight`, so xterm
      // always fits N rows comfortably with a constant tiny
      // remainder, and that remainder doesn't move as snaps
      // change. A few unused pixels at the bottom is cheaper
      // than a visible jitter.
      // The rows lent to the pinned band are still the host's.
      const h = host && host.style.height ? parseFloat(host.style.height) : 0;
      const rows = term.rows + lent();
      return rows > 0 ? Math.ceil(h / rows) : 0;
    },
    findNext: (query, opts) =>
      searchAddon.findNext(query, {
        regex: opts?.regex ?? false,
        wholeWord: opts?.wholeWord ?? false,
        caseSensitive: opts?.caseSensitive ?? false,
        decorations: searchDecorations(term),
      }),
    findPrevious: (query, opts) =>
      searchAddon.findPrevious(query, {
        regex: opts?.regex ?? false,
        wholeWord: opts?.wholeWord ?? false,
        caseSensitive: opts?.caseSensitive ?? false,
        decorations: searchDecorations(term),
      }),
    clearSearch: () => searchAddon.clearDecorations(),
    clearSelection: () => term.clearSelection(),
    hasSelection: () => term.hasSelection(),
    getSelection: () => term.getSelection(),
    select: (column, row, length) => term.select(column, row, length),
    bufferView: () => {
      const buffer = term.buffer.active;
      return {
        cols: term.cols,
        rows: term.rows,
        viewportY: buffer.viewportY,
        baseY: buffer.baseY,
        cursorY: buffer.cursorY,
      };
    },
    markLine: (row) => {
      const buffer = term.buffer.active;
      if (buffer.type !== 'normal') return null;
      return term.registerMarker(Math.max(0, row) - (buffer.baseY + buffer.cursorY)) ?? null;
    },
    lineText: (row) => term.buffer.active.getLine(row)?.translateToString(true) ?? null,
    selectAll: () => term.selectAll(),
    onSelectionChange: (cb) => {
      const disposable = term.onSelectionChange(cb);
      return () => disposable.dispose();
    },
    promptRegion: async () => {
      if (!quiet() && nativeSurfaceEnabled()) {
        return regionFromCursor(await terminalCursor(session).catch(() => null));
      }
      return regionFromXterm(region(), term.buffer.active, term.cols);
    },
    screenRows: async () => {
      if (!quiet() && nativeSurfaceEnabled()) {
        const screen = await terminalScreenRows(session).catch(() => null);
        return screen ? { rows: screen.rows, cols: screen.cols, atBottom: screen.at_bottom } : null;
      }
      const buffer = term.buffer.active;
      const rows = Array.from(
        { length: term.rows },
        (_, row) => buffer.getLine(buffer.viewportY + row)?.translateToString(true) ?? '',
      );
      return { rows, cols: term.cols, atBottom: buffer.viewportY === buffer.baseY };
    },
    cellAt: (clientX, clientY) => {
      const g = gridBox();
      return g ? cellInGrid(clientX, clientY, g, { width: g.cellW, height: g.cellH }) : null;
    },
    rowTop: (row) => {
      const g = gridBox();
      return g ? g.top + row * g.cellH : null;
    },
    grid: () => {
      const g = gridBox();
      return g ? { left: g.left, top: g.top, cellW: g.cellW, cellH: g.cellH } : null;
    },
  };
}
