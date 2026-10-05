// Selection drags across the xterm scrollback split.
//
// Scrolled back, xterm shows a second terminal, the history pane, over the
// top of the live one, with the divider on its bottom edge. xterm lets a
// drag in the history pane run on past its bottom edge and scrolls it all
// the way to the end of its buffer, so the history came to show and select
// the very rows the live half shows below the divider, and the split never
// closed.
//
// A drag that starts on the history pane's text now stays in the history.
// Once the pointer goes below the pane, this takes the drag over from
// xterm. The selection reaches the end of the last history row, never the
// divider or a live row, and the history scrolls down every 50 ms at
// xterm's own rate, faster the further past. It stops where the history
// shows what the live pane holds under it. There the split closes, the way
// a scroll back to the bottom closes it, and the same selection runs on in
// the live pane from the same line, so a copy holds one run of lines with
// nothing doubled and nothing skipped. A wheel or Page Down that takes the
// history to its bottom during the drag carries it on the same way. The
// native grid does the same in src-tauri/src/native/surface/split_drag.rs.
//
// A drag that starts in the live half, or that never leaves the history
// pane, stays xterm's own.

/** How far past an edge, in CSS px, the pointer reaches top speed. */
export const DRAG_SCROLL_EDGE = 50;
/** The most lines one autoscroll tick moves. */
export const DRAG_SCROLL_MAX = 15;
/** Time between autoscroll ticks, in ms. */
export const DRAG_SCROLL_INTERVAL = 50;

/** The lines one autoscroll tick moves for a pointer `past` CSS px beyond
 *  an edge: positive below the bottom edge, negative above the top, 0
 *  inside. One just past an edge, `DRAG_SCROLL_MAX` from
 *  `DRAG_SCROLL_EDGE` px on. The rule xterm's own drag scroll follows. */
export function autoscrollLines(past: number): number {
  if (!Number.isFinite(past) || past === 0) return 0;
  const reach = Math.min(Math.abs(past) / DRAG_SCROLL_EDGE, 1);
  return Math.sign(past) * (1 + Math.round(reach * (DRAG_SCROLL_MAX - 1)));
}

/** How far `y` lies past a span from `top` that is `height` tall:
 *  negative above it, positive below it, 0 on it. */
export function pastEdge(y: number, top: number, height: number): number {
  if (y < top) return y - top;
  if (y > top + height) return y - (top + height);
  return 0;
}

/** Where a pane's cell grid sits, in client px. */
export interface PaneGrid {
  left: number;
  top: number;
  cellW: number;
  cellH: number;
}

/** A pane's buffer as the drag reads it. `viewportY`, `baseY` and the
 *  cursor's row count in buffer rows, `cursorY` from the screen's top. */
export interface BufferView {
  cols: number;
  rows: number;
  viewportY: number;
  baseY: number;
  cursorY: number;
}

/** A buffer cell: column `x`, which may be `cols` for the end of a row,
 *  and buffer row `y`. */
export interface Cell {
  x: number;
  y: number;
}

/** The cell a selection reaches with the pointer at a client point, by
 *  xterm's own rule: the column boundary nearest the pointer, and the row
 *  under it, held inside the screen. */
export function xtermCell(
  clientX: number,
  clientY: number,
  grid: PaneGrid,
  view: BufferView,
): Cell {
  const clamp = (n: number, lo: number, hi: number) => Math.min(Math.max(n, lo), hi);
  const col = clamp(
    Math.ceil((clientX - grid.left + grid.cellW / 2) / grid.cellW),
    1,
    view.cols + 1,
  );
  const row = clamp(Math.ceil((clientY - grid.top) / grid.cellH), 1, view.rows);
  return { x: col - 1, y: row - 1 + view.viewportY };
}

/** The arguments to xterm's `select` for the cells from `a` to `b`, in
 *  either order: the first cell and how many cells follow it, rows
 *  counted whole at `cols`. */
export function selectSpan(
  a: Cell,
  b: Cell,
  cols: number,
): { column: number; row: number; length: number } {
  const [start, end] = a.y < b.y || (a.y === b.y && a.x <= b.x) ? [a, b] : [b, a];
  return {
    column: start.x,
    row: start.y,
    length: (end.y - start.y) * cols + end.x - start.x,
  };
}

/** What to add to a history row to find about the same line in the live
 *  pane. Both panes take the same output, so the line the cursor is on is
 *  the same line in each, whatever each holds above it. Rows only one
 *  pane holds between a row and the cursor move it off by as many, which
 *  `matchRow` corrects. */
export function lineShift(history: BufferView, live: BufferView): number {
  return live.baseY + live.cursorY - (history.baseY + history.cursorY);
}

/** How many rows each way from its guess a handoff looks for the line. */
export const MATCH_REACH = 1000;
/** How many rows each side of the line a handoff compares as well. */
export const MATCH_CONTEXT = 4;

/** The live row that holds history row `row`, near `guess`, where
 *  `lineShift` puts it. The panes need not hold the same rows. The live
 *  pane alone holds the restored banner, the rows that pad a short
 *  restore to the bottom, and the notices the page writes, and the
 *  history loads the stored text at its mount. So this looks out from the
 *  guess, nearest first, for a row with the same text, and takes the one
 *  whose rows around it match most. A blank row counts for nothing there,
 *  since blank rows match anywhere. The guess when no row matches. */
export function matchRow(
  historyText: (row: number) => string | null,
  liveText: (row: number) => string | null,
  row: number,
  guess: number,
  reach = MATCH_REACH,
): number {
  const want = historyText(row);
  if (want === null) return guess;
  const around: { at: number; text: string }[] = [];
  for (let at = -MATCH_CONTEXT; at <= MATCH_CONTEXT; at++) {
    const text = at === 0 ? null : historyText(row + at);
    if (text) around.push({ at, text });
  }
  const seen = new Map<number, string | null>();
  const live = (r: number) => {
    if (r < 0) return null;
    if (!seen.has(r)) seen.set(r, liveText(r));
    return seen.get(r) ?? null;
  };
  let best = -1;
  let found = guess;
  for (let step = 0; step <= reach; step++) {
    for (const r of step === 0 ? [guess] : [guess - step, guess + step]) {
      if (live(r) !== want) continue;
      const score = around.filter(({ at, text }) => live(r + at) === text).length;
      if (score > best) {
        best = score;
        found = r;
      }
      if (best === around.length) return found;
    }
  }
  return found;
}

/** The history pane's `viewportY` at which it shows what the live pane
 *  holds under it, row for row, so the split can close without a jump.
 *  `rowShift` is how many rows the history screen's top sits below the
 *  live screen's. Never past the end of the history buffer. */
export function closeAt(history: BufferView, live: BufferView, rowShift = 0): number {
  const aligned = history.baseY + history.cursorY - live.cursorY + rowShift;
  return Math.max(0, Math.min(history.baseY, aligned));
}

/** A line xterm keeps track of through new output and trimmed history. */
export interface LineMark {
  readonly line: number;
  readonly isDisposed: boolean;
  dispose: () => void;
}

/** What the drag needs of a pane. `TerminalHandle` provides it. */
export interface DragPane {
  grid: () => PaneGrid | null;
  bufferView: () => BufferView;
  scrollLines: (n: number) => void;
  select: (column: number, row: number, length: number) => void;
  markLine: (row: number) => LineMark | null;
  /** Buffer row `row` as text with its trailing blanks gone, or null
   *  past the buffer. */
  lineText: (row: number) => string | null;
}

/** A mouse event as the drag reads it. */
export interface DragPointer {
  clientX: number;
  clientY: number;
  button: number;
  /** The buttons held, 1 for the primary one. */
  buttons: number;
  /** The click count: 1 for a plain press. */
  detail: number;
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}

/** Timers, so a test can run the ticks itself. */
export interface DragTimers {
  start: (tick: () => void, ms: number) => unknown;
  stop: (id: unknown) => void;
}

const windowTimers: DragTimers = {
  start: (tick, ms) => setInterval(tick, ms),
  stop: (id) => clearInterval(id as ReturnType<typeof setInterval>),
};

interface Options {
  /** The open split's history pane once it shows, else null. */
  history: () => DragPane | null;
  /** The live pane. */
  live: () => DragPane | null;
  /** Close the split as a scroll back to the bottom does. */
  closeSplit: () => void;
  timers?: DragTimers;
}

/** Where a drag began: column `col` of the line `mark` follows, or of
 *  buffer row `row` when xterm gave no mark. */
interface Anchor {
  mark: LineMark | null;
  row: number;
  col: number;
}

/** The anchor's cell now. A line trimmed off the top of the buffer
 *  leaves the drag anchored at the buffer's first cell. */
function anchorCell(anchor: Anchor): Cell {
  if (!anchor.mark) return { x: anchor.col, y: anchor.row };
  if (anchor.mark.isDisposed) return { x: 0, y: 0 };
  return { x: anchor.col, y: anchor.mark.line };
}

type Phase =
  | { kind: 'idle' }
  // xterm drags. The press is marked in case the drag leaves the pane.
  | { kind: 'armed'; anchor: Anchor }
  // Past the history pane's bottom edge, this drags in the history.
  | { kind: 'history'; anchor: Anchor }
  // The split closed at the bottom and the drag runs on in the live pane.
  | { kind: 'live'; anchor: Anchor };

/** One drag across the split at a time, fed the window's mouse events. */
export class SplitDrag {
  private phase: Phase = { kind: 'idle' };
  private pointer: DragPointer | null = null;
  private timer: unknown = null;
  private readonly timers: DragTimers;

  constructor(private readonly opts: Options) {
    this.timers = opts.timers ?? windowTimers;
  }

  /** Who drags now: xterm (`idle`, `armed`) or this (`history`, `live`). */
  get state(): Phase['kind'] {
    return this.phase.kind;
  }

  /** A press. `onHistoryText` says it landed on the history pane's text,
   *  not its scrollbar. True when the drag is marked to follow. */
  press(p: DragPointer, onHistoryText: boolean): boolean {
    this.release();
    // Word, line, extend and column drags stay xterm's.
    if (!onHistoryText || p.button !== 0 || p.detail > 1) return false;
    if (p.shiftKey || p.altKey || p.ctrlKey || p.metaKey) return false;
    const history = this.opts.history();
    const grid = history?.grid();
    if (!history || !grid) return false;
    const cell = xtermCell(p.clientX, p.clientY, grid, history.bufferView());
    this.phase = {
      kind: 'armed',
      anchor: { mark: history.markLine(cell.y), row: cell.y, col: cell.x },
    };
    return true;
  }

  /** A move with the button held. True when this drags, so xterm must not. */
  move(p: DragPointer): boolean {
    const phase = this.phase;
    if (phase.kind === 'idle') return false;
    // The release went somewhere this never heard.
    if ((p.buttons & 1) === 0) {
      this.release();
      return false;
    }
    this.pointer = p;
    if (phase.kind === 'armed') {
      const history = this.opts.history();
      const grid = history?.grid();
      if (!history || !grid) {
        this.release();
        return false;
      }
      const view = history.bufferView();
      if (pastEdge(p.clientY, grid.top, view.rows * grid.cellH) <= 0) return false;
      // Selecting through xterm's API ends xterm's own drag, so from here
      // on this drags.
      this.phase = { kind: 'history', anchor: phase.anchor };
      this.timer = this.timers.start(() => this.tick(), DRAG_SCROLL_INTERVAL);
    }
    this.extend();
    return true;
  }

  /** One autoscroll tick, every `DRAG_SCROLL_INTERVAL` ms while this
   *  drags. False once this no longer drags. */
  tick(): boolean {
    const phase = this.phase;
    const p = this.pointer;
    if ((phase.kind !== 'history' && phase.kind !== 'live') || !p) return false;
    const pane = phase.kind === 'history' ? this.opts.history() : this.opts.live();
    const grid = pane?.grid();
    if (!pane || !grid) {
      this.release();
      return false;
    }
    const view = pane.bufferView();
    const lines = autoscrollLines(pastEdge(p.clientY, grid.top, view.rows * grid.cellH));
    if (lines === 0) return true;
    let floor = view.baseY;
    if (phase.kind === 'history' && lines > 0) {
      const live = this.opts.live();
      const liveGrid = live?.grid();
      if (live && liveGrid) {
        const shift = Math.round((grid.top - liveGrid.top) / grid.cellH);
        floor = closeAt(view, live.bufferView(), shift);
      }
      if (view.viewportY >= floor) {
        this.handOff();
        return this.dragging();
      }
    }
    const target = Math.max(0, Math.min(view.viewportY + lines, floor));
    if (target !== view.viewportY) pane.scrollLines(target - view.viewportY);
    if (phase.kind === 'history' && lines > 0 && target >= floor) {
      this.handOff();
      return this.dragging();
    }
    this.extend();
    return this.dragging();
  }

  /** The split is about to close because the history reached its bottom
   *  some other way than this drag's own scroll, by the wheel or Page
   *  Down. A drag in the history then carries its selection on in the
   *  live pane, as at the end of its own scroll, whether this or xterm
   *  drags it so far. Call it while the split is still open. A find, a
   *  middle click or Esc closes the split without it, and the drag ends. */
  historyBottomed(): void {
    const phase = this.phase;
    if (phase.kind === 'armed') {
      // From here on this drags, in the live pane. Selecting through
      // xterm's API ends xterm's own drag in the history first.
      this.phase = { kind: 'history', anchor: phase.anchor };
      this.timer = this.timers.start(() => this.tick(), DRAG_SCROLL_INTERVAL);
      this.extend();
    }
    if (this.phase.kind === 'history') this.handOff();
  }

  /** The button went up, or the drag ended some other way. */
  release(): void {
    if (this.timer !== null) this.timers.stop(this.timer);
    this.timer = null;
    const phase = this.phase;
    if (phase.kind !== 'idle') phase.anchor.mark?.dispose();
    this.phase = { kind: 'idle' };
    this.pointer = null;
  }

  /** True while this drags rather than xterm. */
  private dragging(): boolean {
    return this.phase.kind === 'history' || this.phase.kind === 'live';
  }

  /** The history reached the bottom: close the split and carry the
   *  selection over to the live pane, from the same line. */
  private handOff(): void {
    const phase = this.phase;
    const history = this.opts.history();
    const live = this.opts.live();
    if (phase.kind !== 'history' || !history || !live) {
      this.release();
      return;
    }
    const from = anchorCell(phase.anchor);
    const guess = Math.max(0, from.y + lineShift(history.bufferView(), live.bufferView()));
    const row = matchRow(
      (r) => history.lineText(r),
      (r) => live.lineText(r),
      from.y,
      guess,
    );
    phase.anchor.mark?.dispose();
    this.phase = { kind: 'live', anchor: { mark: live.markLine(row), row, col: from.x } };
    this.opts.closeSplit();
    this.extend();
  }

  /** Select from the anchor to the pointer in the pane this drags in: the
   *  cell under it, or past an edge the far end of the row at that edge. */
  private extend(): void {
    const phase = this.phase;
    const p = this.pointer;
    if ((phase.kind !== 'history' && phase.kind !== 'live') || !p) return;
    const pane = phase.kind === 'history' ? this.opts.history() : this.opts.live();
    const grid = pane?.grid();
    if (!pane || !grid) {
      this.release();
      return;
    }
    const view = pane.bufferView();
    const past = pastEdge(p.clientY, grid.top, view.rows * grid.cellH);
    const end =
      past > 0
        ? { x: view.cols, y: view.viewportY + view.rows - 1 }
        : past < 0
          ? { x: 0, y: view.viewportY }
          : xtermCell(p.clientX, p.clientY, grid, view);
    const span = selectSpan(anchorCell(phase.anchor), end, view.cols);
    pane.select(span.column, span.row, span.length);
  }
}

/** The mouse event fields the window listeners read. */
export type WindowMouse = DragPointer & { target: unknown };

/** Where the listeners go: the window, or a stand in for it. Method
 *  signatures, so the window's own, which take any event, fit. */
export interface DragWindow {
  addEventListener(type: string, listener: (e: WindowMouse) => void, capture: boolean): void;
  removeEventListener(type: string, listener: (e: WindowMouse) => void, capture: boolean): void;
}

/** The text of the open split's history pane, which a drag starts on. */
export const HISTORY_TEXT = '.terminal-pane-history .xterm-screen';

/** Feed `drag` the window's mouse events, in the capture phase so the
 *  move that takes a drag over selects, and with it ends xterm's own
 *  drag, before xterm's document listener hears it. Nothing starts while
 *  `native` says the native grid draws, since it splits itself. The drag
 *  ends when the window loses focus or the page hides, since the release
 *  may never come then. Leaving the window ends nothing, as the release
 *  still comes. Returns the cleanup. */
export function listenSplitDrag(win: DragWindow, drag: SplitDrag, native: () => boolean) {
  const onDown = (e: WindowMouse) => {
    if (native()) return;
    const target = e.target as { closest?: (selector: string) => unknown } | null;
    const onText = typeof target?.closest === 'function' && target.closest(HISTORY_TEXT) != null;
    drag.press(e, onText);
  };
  const onMove = (e: WindowMouse) => {
    drag.move(e);
  };
  const onUp = () => drag.release();
  // The window's own blur. A field in the page that loses focus, as the
  // command line does on a press in the terminal, blurs too, and must not
  // end the drag that press starts.
  const onBlur = (e: WindowMouse) => {
    if (e.target === win) drag.release();
  };
  const onHide = () => drag.release();
  win.addEventListener('mousedown', onDown, true);
  win.addEventListener('mousemove', onMove, true);
  win.addEventListener('mouseup', onUp, true);
  win.addEventListener('blur', onBlur, false);
  win.addEventListener('visibilitychange', onHide, true);
  return () => {
    drag.release();
    win.removeEventListener('mousedown', onDown, true);
    win.removeEventListener('mousemove', onMove, true);
    win.removeEventListener('mouseup', onUp, true);
    win.removeEventListener('blur', onBlur, false);
    win.removeEventListener('visibilitychange', onHide, true);
  };
}
