import type { FitAddon } from '@xterm/addon-fit';
import type { Terminal } from '@xterm/xterm';
import { nativeSurfaceSetBounds, nativeSurfaceSetCellMetrics } from '../ipc/nativeSurface';
import { nativeSurfaceEnabled } from './terminalRenderer';
import { keptRows, nativeBottomBounds, spareAbove } from './terminalRows';

// The size of a terminal pane and where its grid sits in it. xterm fits
// the pane less the rows the pinned band borrows, and the native grid
// hears of the pane's bounds and xterm's cell. The pane's layout, the
// window and a drag on the split all reach it.

/** What the sizer reads from the pane it sizes. The getters read the
 *  pane's props as they stand when the sizer asks. */
export interface SizedPane {
  term: Terminal;
  fit: FitAddon;
  /** The wrapper the layout sizes. */
  sizer: HTMLDivElement | null;
  /** The element xterm opened in, which takes the wrapper's size. */
  host: HTMLDivElement;
  /** Gives xterm a size through the region writer in use now. */
  resize(cols: number, rows: number): void;
  /** Rows lent to the pinned prompt band. */
  lent(): number;
  /** Whether the grid keeps to the bottom of the pane. */
  anchor(): boolean;
  /** Whether this is the split's history pane. */
  quiet(): boolean;
  onCellSize(): ((size: { width: number; height: number; cols: number }) => void) | undefined;
}

export class PaneSizer {
  private readonly pane: SizedPane;
  // Where xterm sits in its pane, as last placed.
  private placedTop = '0px';
  // The wrapper's size in whole pixels, as last given to the host.
  private lastW = 0;
  private lastH = 0;
  // Tier 3 (docs/native-renderer.md): report this pane's screen
  // rectangle to the native wgpu surface so it tracks the terminal.
  // Live pane only, and only while the surface draws it.
  //
  // The rows the pinned band borrows go along, so the grid gives them
  // up in the same frame as the new bounds, and the game keeps its size.
  // While the grid keeps to the bottom of the pane under the underlay,
  // the bounds start lower by the pixels its rows leave over
  // (`nativeSpare`), and pointer positions count from there.
  private readonly nativeSurfaceOn: boolean;
  private lastNativeBounds = '';
  private spare = 0;
  private lastCellMetrics = '';
  private lastCellSize = '';
  private observer: ResizeObserver | null = null;
  private rafPoll = 0;
  private intervalPoll: ReturnType<typeof setInterval> | undefined;

  constructor(pane: SizedPane) {
    this.pane = pane;
    this.nativeSurfaceOn = !pane.quiet() && nativeSurfaceEnabled();
  }

  /** The pixels the native grid starts below the pane's top. The bounds
   *  report sets them again on every sync. */
  get nativeSpare(): number {
    return this.spare;
  }

  // Where xterm sits in its pane. While it keeps to the bottom, the host
  // moves down by the pixels its rows leave over, and the sizer clips
  // what the host then reaches past its bottom, which the band covers.
  // xterm maps the pointer from its screen's own box, so selections and
  // links follow. Under the native surface the bounds carry it instead.
  placeGrid(): void {
    const { term, sizer, host } = this.pane;
    if (!sizer) return;
    let top = 0;
    const cell = term.dimensions?.css?.cell?.height;
    if (this.pane.anchor() && !this.pane.quiet() && !nativeSurfaceEnabled() && cell) {
      const dpr = window.devicePixelRatio || 1;
      const height = sizer.getBoundingClientRect().height;
      top = spareAbove(height, term.rows + this.pane.lent(), cell, dpr);
    }
    const next = `${top}px`;
    if (next === this.placedTop) return;
    this.placedTop = next;
    host.style.top = next;
  }

  // Fit xterm to its pane, less the rows the pinned band borrows. The
  // FitAddon only proposes the size, so the lent rows come off here.
  fitKept(): void {
    const dims = this.pane.fit.proposeDimensions();
    if (!dims || Number.isNaN(dims.cols) || Number.isNaN(dims.rows)) return;
    this.pane.resize(dims.cols, keptRows(dims.rows, this.pane.lent()));
    this.placeGrid();
  }

  readonly safeFit = (): void => {
    // When the native surface owns the pane it is the size authority and
    // resizes xterm via the native-grid-size event. The FitAddon sizes
    // xterm from its own cells, so letting it fit here would fight the
    // native grid.
    if (!this.pane.quiet() && nativeSurfaceEnabled()) return;
    try {
      this.fitKept();
    } catch {
      // ignore resize before layout settles
    }
  };

  private reportNativeBounds(): void {
    const { term, sizer } = this.pane;
    if (!this.nativeSurfaceOn || !sizer) return;
    const r = sizer.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const lent = this.pane.lent();
    let { top, height } = r;
    this.spare = 0;
    if (this.pane.anchor() && nativeSurfaceEnabled()) {
      const cellPx = Math.round(term.dimensions?.device?.cell?.height ?? 0);
      const placed = nativeBottomBounds(r.top, r.height, dpr, cellPx);
      ({ top, height } = placed);
      this.spare = placed.spare;
    }
    const key = `${Math.round(r.left)},${top},${Math.round(r.width)},${height},${dpr},${lent}`;
    if (key === this.lastNativeBounds) return;
    this.lastNativeBounds = key;
    void nativeSurfaceSetBounds({
      x: r.left,
      y: top,
      width: r.width,
      height,
      dpr,
      lent,
    }).catch(() => {});
  }

  // Report xterm's exact device cell size so the surface grid matches the
  // webview's spacing instead of deriving it from font metrics. The cell
  // size is stable across pane resizes; it changes on font, dpr, and line
  // height changes. The line height is in the cell: xterm multiplies its
  // glyph box by it and centers the box in the cell, so the box height
  // rides along and the surface puts its baseline where xterm's is.
  reportCellMetrics(): void {
    if (!this.nativeSurfaceOn) return;
    const device = this.pane.term.dimensions?.device;
    const cell = device?.cell;
    if (!device || !cell?.width || !cell?.height) return;
    const width = Math.round(cell.width);
    const height = Math.round(cell.height);
    const charHeight = Math.round(device.char?.height ?? 0);
    const key = `${width},${height},${charHeight}`;
    if (key === this.lastCellMetrics) return;
    this.lastCellMetrics = key;
    void nativeSurfaceSetCellMetrics({
      width,
      height,
      charHeight: charHeight > 0 ? charHeight : null,
    }).catch(() => {});
  }

  // The cell size a band outside the grid lays its characters out on.
  // The native grid draws xterm's device cell rounded to whole pixels
  // (reportCellMetrics above), and xterm draws its own.
  reportCellSize(): void {
    const onCellSize = this.pane.onCellSize();
    if (!onCellSize) return;
    const { term } = this.pane;
    const cell = term.dimensions?.device?.cell;
    if (!cell?.width || !cell?.height) return;
    const dpr = window.devicePixelRatio || 1;
    const native = !this.pane.quiet() && nativeSurfaceEnabled();
    const width = (native ? Math.round(cell.width) : cell.width) / dpr;
    const height = (native ? Math.round(cell.height) : cell.height) / dpr;
    const key = `${width},${height},${term.cols}`;
    if (key === this.lastCellSize) return;
    this.lastCellSize = key;
    onCellSize({ width, height, cols: term.cols });
  }

  // The actual fix. Read the sizing wrapper's bounding rect every
  // frame and explicitly write width/height in pixels onto the
  // terminal-host element. xterm-addon-fit reads the host's
  // computed `height` style (not its clientHeight); without an
  // explicit pixel height, computed height comes back wrong in some
  // Tauri/WebKit layout passes — opening DevTools forces a layout
  // and the value goes right, but otherwise it stays stale.
  private readonly sync = (): void => {
    const { sizer, host } = this.pane;
    if (!sizer) return;
    this.reportNativeBounds();
    this.reportCellMetrics();
    this.reportCellSize();
    // A pane a fraction of a pixel taller fits the same rows and leaves
    // a different spare.
    this.placeGrid();
    const rect = sizer.getBoundingClientRect();
    const w = Math.floor(rect.width);
    const h = Math.floor(rect.height);
    if (w === this.lastW && h === this.lastH) return;
    this.lastW = w;
    this.lastH = h;
    host.style.width = `${w}px`;
    host.style.height = `${h}px`;
    this.safeFit();
    // Fit may have just established or changed the cell dimensions.
    this.reportCellMetrics();
    this.reportCellSize();
  };

  // The pinned band borrows more or fewer rows, or the grid starts or
  // stops keeping to the bottom. The pane keeps its size, so nothing
  // above notices: xterm fits and places itself again, or the native
  // grid hears of it with its bounds.
  relayout(): void {
    if (this.nativeSurfaceOn) this.reportNativeBounds();
    else this.safeFit();
    this.placeGrid();
  }

  // Resizable broadcasts `vosh:resize-progress` { size } from
  // its pointermove handler — fires synchronously inside the
  // same JS task that just set the wrapper's CSS height. We
  // run sync + fit + anchor restore in that same task so
  // wrapper, xterm, and scroll all update before the browser
  // paints. Anything async (React state, ResizeObserver) would
  // land in a separate paint and the user would see a brief
  // mismatched intermediate frame — the "jitter" that every
  // previous attempt produced. For quiet panes (split-scrollback
  // history) the viewport top is saved before fit and restored
  // after so the larger viewport exposes new rows BELOW the old
  // bottom instead of pushing old content down. That's what
  // makes the drag look like a continuous curtain: the new
  // rows xterm exposes match the rows that were just at the
  // top of the live pane (both buffers are in sync because
  // they both consume the same session://output stream).
  private readonly onResizeProgress = (): void => {
    const { sizer, host } = this.pane;
    if (!sizer) return;
    const rect = sizer.getBoundingClientRect();
    const w = Math.floor(rect.width);
    const h = Math.floor(rect.height);
    if (w === this.lastW && h === this.lastH) return;
    this.lastW = w;
    this.lastH = h;
    host.style.width = `${w}px`;
    host.style.height = `${h}px`;
    this.safeFit();
    // No explicit refresh or viewport restore. xterm's resize
    // adjusts the buffer dimensions; the WebGL renderer's own
    // debounced redraw paints once after the drag settles, which
    // matches what the DOM renderer does — both panes look
    // stationary during the drag and the divider slides cleanly
    // between them. Forcing a per-frame refresh produced the
    // curtain effect (content shifted per drag frame); doing
    // scrollLines or scrollToLine produced oscillation. Leaving
    // it alone gives the right visual.
  };

  /** Follow the pane from now on: a drag on the split, the window, the
   *  layout and a poll. */
  start(): void {
    window.addEventListener('vosh:resize-progress', this.onResizeProgress);
    window.addEventListener('resize', this.sync);
    const observer = new ResizeObserver(this.sync);
    if (this.pane.sizer) observer.observe(this.pane.sizer);
    observer.observe(document.body);
    this.observer = observer;

    // Keep a resize sync alive after mount as a backup for the rare
    // Tauri/WebKit case where ResizeObserver misses a one-shot chrome
    // change. The split-scrollback history pane (quiet) needs a
    // per-frame sync: it mounts transiently when the split opens and
    // must be fully fit by the time onScrollbackLoaded positions its
    // viewport, otherwise that first scroll lands on blank rows and only
    // a second scroll re-renders it. The live pane uses a low-frequency
    // interval instead, because a per-frame getBoundingClientRect there
    // stacked a layout reflow onto every combat-round write and stole
    // frames from the renderer.
    if (this.pane.quiet()) {
      const pollLoop = () => {
        this.sync();
        this.rafPoll = requestAnimationFrame(pollLoop);
      };
      this.rafPoll = requestAnimationFrame(pollLoop);
    } else {
      this.intervalPoll = setInterval(this.sync, 250);
    }
  }

  // Fits the pane to a cell xterm measured again once a face loaded, and
  // reports the cell. Under the native surface the fit waits for the
  // grid, which hears the new cell.
  refitCell(): void {
    this.safeFit();
    this.reportCellMetrics();
    this.reportCellSize();
  }

  /** Stop following the pane. */
  stop(): void {
    if (this.rafPoll) cancelAnimationFrame(this.rafPoll);
    if (this.intervalPoll) clearInterval(this.intervalPoll);
    this.observer?.disconnect();
    window.removeEventListener('resize', this.sync);
    window.removeEventListener('vosh:resize-progress', this.onResizeProgress);
  }
}
