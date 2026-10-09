// Bands under your prompts in xterm, while your prompt shows lifted.
//
// The session wraps each prompt in two private marks: ESC ] 7717 ; l ; L
// BEL before its first line and ESC ] 7717 ; e ; L BEL after its last
// visible byte (crates/prompt/src/stage/marks.rs). A LiftTracker reads them
// as xterm parses them. For each lift it keeps a marker on the first row of
// the logical line it starts in, where it starts within that logical line,
// and how many logical lines later and how far in it ends. A marker on the
// logical line's first row survives a reflow that joins or splits its
// wrapped rows, and walking the wrapped rows again from it finds the lift
// at any width.
//
// layoutBands turns lifts into rectangles drawn as the edit band: 4 px
// past the first and last glyph of the widest row, 2 px above the first
// row and below the last, radius 4. Two lifts on adjacent rows keep 2
// px of ground between them. When your echo follows a prompt whose last
// row is narrower than the widest, the band steps in 4 px past that
// row's last glyph, so the echo never sits on it. BandLayer draws them
// in a layer under xterm's text, repositioned in xterm's own render
// frame so band and text land in the same composite.

import { getPromptReach, subscribePromptReach } from '../../stores/session/promptReachStore';
import type { IBufferCell, IDisposable, IMarker, Terminal } from '@xterm/xterm';
import { REGION_OSC } from '../terminalRegion';

/** The most lifts one terminal keeps, so the newest this many prompts keep
 *  their band and older ones in the scrollback show plain. Every xterm
 *  marker listens to the buffer's trim, so each line that scrolls out of a
 *  full scrollback costs one call per marker. Measured in V8 with xterm
 *  6.1, a full 10000 line scrollback took 0.8 ms more per 100 new lines
 *  with 1000 markers, 3.2 ms with 3000 and 6.2 ms with 5000, and the
 *  budget is 1 ms. */
export const LIFT_CAP = 1000;

/** The most rows one lift can cover, however narrow the terminal. */
const MAX_LIFT_ROWS = 64;

/** One lift as the tracker keeps it. */
interface LiftRecord {
  id: number;
  /** On the first row of the logical line the lift starts in. */
  start: IMarker;
  /** Cells from that row's start to the lift's first cell. */
  startOffset: number;
  /** Logical lines after the start's to the one the lift ends in, and
   *  cells into it where the lift ends, or null until its end mark. */
  end: { lines: number; offset: number } | null;
}

/** Where a lift sits on the screen now, in cells. */
export interface LiftExtent {
  id: number;
  /** Buffer rows, the first and the last the lift covers. */
  top: number;
  bottom: number;
  /** The leftmost cell it starts on in any row. */
  left: number;
  /** One past the rightmost glyph it shows in any row. */
  right: number;
  /** One past the last glyph of its last row, when that row is narrower
   *  than the widest and something shows after the lift on it, such as
   *  your echo, so the band steps in there instead of running under it. */
  notch?: number;
}

/** A band in CSS px, relative to the first row of the viewport. A band
 *  with a notch leaves out the part right of `notch.x` and below
 *  `notch.y`, both from its own left and top, where your echo sits. */
export interface BandBox {
  id: number;
  left: number;
  top: number;
  width: number;
  height: number;
  notch?: { x: number; y: number };
}

/** The parts of an xterm the tracker reads. */
export type LiftTerminal = Pick<Terminal, 'cols' | 'buffer' | 'parser' | 'registerMarker'>;

export class LiftTracker implements IDisposable {
  private readonly term: LiftTerminal;
  private readonly cap: number;
  /** Oldest first. */
  private readonly lifts = new Map<number, LiftRecord>();
  private readonly osc: IDisposable;

  constructor(term: LiftTerminal, cap = LIFT_CAP) {
    this.term = term;
    this.cap = cap;
    // Registered after the region writer's handler, so xterm asks this
    // one first. Region marks pass on to the writer.
    this.osc = term.parser.registerOscHandler(REGION_OSC, (data) => this.onMark(data));
  }

  /** How many lifts the tracker keeps now. */
  get size(): number {
    return this.lifts.size;
  }

  dispose(): void {
    this.osc.dispose();
    for (const lift of this.lifts.values()) lift.start.dispose();
    this.lifts.clear();
  }

  private onMark(data: string): boolean {
    const match = /^([le]);(\d+)$/.exec(data);
    if (!match) return false;
    const id = Number(match[2]);
    if (match[1] === 'l') this.begin(id);
    else this.finish(id);
    return true;
  }

  /** The cursor as a logical position: the buffer row its logical line
   *  starts on, and how many cells into that logical line it sits. */
  private cursorLogical(): { row: number; offset: number } {
    const buffer = this.term.buffer.active;
    const cursorRow = buffer.baseY + buffer.cursorY;
    let row = cursorRow;
    while (row > 0 && buffer.getLine(row)?.isWrapped) row--;
    return { row, offset: (cursorRow - row) * this.term.cols + buffer.cursorX };
  }

  /** Forget every lift that starts at or after buffer row `row` column
   *  `col`, which a replace that wrote nothing erased, so text written
   *  there later never takes its band. */
  dropFrom(row: number, col: number): void {
    const cols = this.term.cols;
    for (const lift of [...this.lifts.values()]) {
      if (lift.start.isDisposed) continue;
      const startRow = lift.start.line + Math.floor(lift.startOffset / cols);
      const startCol = lift.startOffset % cols;
      if (startRow > row || (startRow === row && startCol >= col)) {
        this.lifts.delete(lift.id);
        lift.start.dispose();
      }
    }
  }

  private begin(id: number): void {
    const buffer = this.term.buffer.active;
    const { row, offset } = this.cursorLogical();
    // A repaint starts a lift again inside its region. When it already
    // starts earlier, above the region at a tank line, it keeps that start.
    const known = this.lifts.get(id);
    if (
      known &&
      !known.start.isDisposed &&
      known.start.line >= 0 &&
      (known.start.line < row || (known.start.line === row && known.startOffset <= offset))
    ) {
      known.end = null;
      return;
    }
    const marker = this.term.registerMarker(row - (buffer.baseY + buffer.cursorY));
    if (!marker) return;
    this.lifts.get(id)?.start.dispose();
    this.lifts.delete(id);
    const record: LiftRecord = { id, start: marker, startOffset: offset, end: null };
    this.lifts.set(id, record);
    marker.onDispose(() => {
      if (this.lifts.get(id) === record) this.lifts.delete(id);
    });
    // The oldest lift goes past the cap.
    while (this.lifts.size > this.cap) {
      const oldest = this.lifts.values().next().value;
      if (!oldest) break;
      this.lifts.delete(oldest.id);
      oldest.start.dispose();
    }
  }

  private finish(id: number): void {
    const lift = this.lifts.get(id);
    if (!lift || lift.start.isDisposed || lift.start.line < 0) return;
    const buffer = this.term.buffer.active;
    const { row, offset } = this.cursorLogical();
    // Count logical lines from the start's to the end's.
    let lines = 0;
    for (let r = lift.start.line + 1; r <= row; r++) {
      if (!buffer.getLine(r)?.isWrapped) lines++;
    }
    lift.end = { lines, offset };
  }

  /** The lifts that meet buffer rows `from` to `to`, as extents at the
   *  width the buffer has now. */
  extents(from: number, to: number): LiftExtent[] {
    const buffer = this.term.buffer.active;
    const cols = this.term.cols;
    const out: LiftExtent[] = [];
    const cell: { value: IBufferCell | undefined } = { value: undefined };
    for (const lift of this.lifts.values()) {
      if (!lift.end || lift.start.isDisposed || lift.start.line < 0) continue;
      // A prompt is never this tall, so a lift that starts this far above
      // the rows asked for ends above them too.
      if (lift.start.line + MAX_LIFT_ROWS < from) continue;
      const startRow = lift.start.line + Math.floor(lift.startOffset / cols);
      if (startRow > to) continue;
      // Walk to the logical line the lift ends in.
      let logical = lift.start.line;
      for (let n = 0; n < lift.end.lines; n++) {
        logical++;
        while (buffer.getLine(logical)?.isWrapped) logical++;
      }
      const endOffset = lift.end.offset;
      // An end at a row's last cell belongs to that row, not the next.
      const endRow = logical + Math.max(0, Math.floor((endOffset - 1) / cols));
      if (endRow < from) continue;
      let left = cols;
      let right = 0;
      let lastRight = 0;
      let after = false;
      for (let row = startRow; row <= endRow; row++) {
        const line = buffer.getLine(row);
        if (!line) continue;
        const first = row === startRow ? lift.startOffset % cols : 0;
        const bound = row === endRow ? endOffset - (endRow - logical) * cols : cols;
        let last = -1;
        for (let x = Math.min(bound, cols) - 1; x >= first; x--) {
          cell.value = line.getCell(x, cell.value);
          const chars = cell.value?.getChars() ?? '';
          const width = cell.value?.getWidth() ?? 1;
          if (width > 0 && chars.trim().length > 0) {
            last = x + width;
            break;
          }
        }
        if (last > first) {
          left = Math.min(left, first);
          right = Math.max(right, last);
        }
        if (row === endRow) {
          lastRight = last;
          // Anything that shows after the lift on its last row.
          for (let x = Math.max(bound, 0); x < cols && !after; x++) {
            cell.value = line.getCell(x, cell.value);
            after =
              (cell.value?.getWidth() ?? 1) > 0 && (cell.value?.getChars() ?? '').trim() !== '';
          }
        }
      }
      if (right <= left) continue;
      const extent: LiftExtent = { id: lift.id, top: startRow, bottom: endRow, left, right };
      if (endRow > startRow && after && lastRight > left && lastRight < right) {
        extent.notch = lastRight;
      }
      out.push(extent);
    }
    return out;
  }
}

/** The attribute on the terminal area that clears xterm's ground so the
 *  bands show under the text. An attribute and not a class, since MainWindow.tsx
 *  writes the area's className whole whenever the scrollback split opens
 *  or closes, and React never writes this attribute. */
export const LIFTED_ATTR = 'data-prompt-lifted';

/** Clear xterm's ground in `area` for the bands, or give it back. */
export function markLifted(
  area: { toggleAttribute(name: string, force: boolean): boolean },
  on: boolean,
): void {
  area.toggleAttribute(LIFTED_ATTR, on);
}

// The native grid draws the same bands in src-tauri/src/native/gpu/bands.rs.
// Both sides run fixtures/prompt-bands/cases.json, so keep them in step.

/** The band outsets, and its corner radius. */
export const BAND_X = 4;
export const BAND_Y = 2;
export const BAND_RADIUS = 4;
/** A shared edge between lifts on adjacent rows stops 1 px inside its
 *  row instead of reaching 2 px past it, so 2 px of ground stays between
 *  the two bands. */
export const BAND_Y_ADJACENT = -1;

/** Rectangles for `extents`, with the viewport's first row at
 *  `viewportY`, cells `cellW` by `cellH`. */
export function layoutBands(
  extents: LiftExtent[],
  viewportY: number,
  cellW: number,
  cellH: number,
): BandBox[] {
  const sorted = [...extents].sort((a, b) => a.top - b.top || a.bottom - b.bottom);
  return sorted.map((lift, i) => {
    const above = sorted[i - 1];
    const below = sorted[i + 1];
    const topOut = above && above.bottom + 1 === lift.top ? BAND_Y_ADJACENT : BAND_Y;
    const bottomOut = below && lift.bottom + 1 === below.top ? BAND_Y_ADJACENT : BAND_Y;
    const top = (lift.top - viewportY) * cellH - topOut;
    const bottom = (lift.bottom - viewportY + 1) * cellH + bottomOut;
    const left = lift.left * cellW - BAND_X;
    const right = lift.right * cellW + BAND_X;
    const box: BandBox = { id: lift.id, left, top, width: right - left, height: bottom - top };
    if (lift.notch !== undefined) {
      box.notch = {
        x: lift.notch * cellW + BAND_X - left,
        y: (lift.bottom - viewportY) * cellH - top,
      };
    }
    return box;
  });
}

/** `boxes` with the newest lift's band `reach` px wider: the prompt
 *  card adds a ↵ after a row a line break ends and a caret past the
 *  last glyph, and the band grows to hold both. A band that steps in
 *  around your echo keeps its width. */
export function widenNewest(boxes: BandBox[], reach: number): BandBox[] {
  if (reach <= 0 || boxes.length === 0) return boxes;
  const newest = boxes.reduce((a, b) => (b.id > a.id ? b : a));
  return boxes.map((box) =>
    box === newest && box.notch === undefined ? { ...box, width: box.width + reach } : box,
  );
}

/** The outline of a band `width` by `height` whose part right of `nx` and
 *  below `ny` is left out, as an SVG path: radius `r` on each outer
 *  corner, square where the rows above meet the last row. `inset` draws it
 *  that far inside, for the light ring. */
export function notchedPath(
  width: number,
  height: number,
  nx: number,
  ny: number,
  r: number,
  inset: number,
): string {
  const i = inset;
  const a = r - i;
  const n = (v: number) => String(Math.round(v * 1000) / 1000);
  const arc = (x: number, y: number) => `A${n(a)} ${n(a)} 0 0 1 ${n(x)} ${n(y)}`;
  return [
    `M${n(r)} ${n(i)}`,
    `H${n(width - r)}`,
    arc(width - i, r),
    `V${n(ny - r)}`,
    arc(width - r, ny - i),
    `H${n(nx - i)}`,
    `V${n(height - r)}`,
    arc(nx - r, height - i),
    `H${n(r)}`,
    arc(i, height - r),
    `V${n(r)}`,
    arc(r, i),
    'Z',
  ].join('');
}

/** The SVG a notched band draws: its fill, then the ring a light theme
 *  shows half a pixel inside its edge. */
function notchedSvg(box: BandBox): string {
  const notch = box.notch;
  if (!notch) return '';
  const w = box.width;
  const h = box.height;
  const fill = notchedPath(w, h, notch.x, notch.y, BAND_RADIUS, 0);
  const ring = notchedPath(w, h, notch.x, notch.y, BAND_RADIUS, 0.5);
  return (
    `<svg width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">` +
    `<path class="prompt-lift-fill" d="${fill}"/>` +
    `<path class="prompt-lift-ring" d="${ring}"/></svg>`
  );
}

/** How far down from its top the band layer, whose top sits at
 *  `layerTop` in the terminal area, is cut while the scrollback split
 *  lays the history pane over the live one down to `historyBottom`. The
 *  history text hides the live rows under it but not the band's reach
 *  past the text, so the live bands stop at the divider, as the native
 *  grid stops them. 0 with the split closed. */
export function dividerCut(layerTop: number, historyBottom: number | null): number {
  if (historyBottom === null) return 0;
  return Math.max(0, historyBottom - layerTop);
}

/** Draws the bands of a LiftTracker in a layer under an xterm's text.
 *
 *  The layer sits in the terminal area, under the terminal well, because
 *  the well and xterm clip at the text's edge and the band reaches 4 px
 *  past it. It is clipped to the text's rectangle plus the band's reach,
 *  so a lift the viewport cuts shows its visible part, square at the cut.
 *  xterm's own ground has to be clear for it to show, which Terminal.tsx
 *  sets while your prompt shows lifted. While the scrollback split is
 *  open the layer stops at the divider (see dividerCut). */
export class BandLayer implements IDisposable {
  private readonly term: Terminal;
  private readonly tracker: LiftTracker;
  private readonly container: HTMLElement;
  private layer: HTMLDivElement | null = null;
  private readonly subs: IDisposable[] = [];
  private readonly observer: ResizeObserver | null;
  /** Watches the terminal well for the history pane coming and going. */
  private readonly wellObserver: MutationObserver | null;
  /** The history pane being watched for its divider moving. */
  private history: Element | null = null;
  private on = false;
  /** Where xterm's first cell sits in the container, and the text's size,
   *  measured when the layout may have moved. */
  private frame: { x: number; y: number; width: number; height: number } | null = null;
  /** The history pane's bottom in the container, null with the split
   *  closed, undefined until measured. */
  private historyBottom: number | null | undefined = undefined;

  constructor(term: Terminal, tracker: LiftTracker, container: HTMLElement) {
    this.term = term;
    this.tracker = tracker;
    this.container = container;
    // xterm fires onRender in the frame it draws the rows in, so the
    // bands move with the text, scrolling and reflowing included.
    this.subs.push(term.onRender(() => this.place()));
    this.subs.push(term.onResize(() => (this.frame = null)));
    // The prompt card's marks reach past the open row's last glyph.
    const unsubscribe = subscribePromptReach(() => this.place());
    this.subs.push({ dispose: unsubscribe });
    this.observer =
      typeof ResizeObserver === 'undefined'
        ? null
        : new ResizeObserver(() => {
            this.frame = null;
            this.historyBottom = undefined;
            this.place();
          });
    this.observer?.observe(container);
    // The split mounts the history pane in the well and drags its
    // divider without xterm drawing, so the cut follows both itself.
    const well = container.querySelector('.terminal-well');
    this.wellObserver =
      well && typeof MutationObserver !== 'undefined'
        ? new MutationObserver(() => this.watchHistory())
        : null;
    if (well) this.wellObserver?.observe(well, { childList: true });
    this.watchHistory();
  }

  /** Draw the bands, or clear them. */
  setEnabled(on: boolean): void {
    this.on = on;
    this.frame = null;
    if (on) {
      this.place();
    } else {
      this.layer?.remove();
      this.layer = null;
    }
  }

  dispose(): void {
    for (const sub of this.subs) sub.dispose();
    this.observer?.disconnect();
    this.wellObserver?.disconnect();
    this.layer?.remove();
    this.layer = null;
  }

  /** Follow the history pane of the scrollback split, when there is one. */
  private watchHistory(): void {
    const history = this.container.querySelector('.terminal-pane-history');
    if (history !== this.history) {
      if (this.history) this.observer?.unobserve(this.history);
      this.history = history;
      if (history) this.observer?.observe(history);
    }
    this.historyBottom = undefined;
    this.place();
  }

  /** The layer, first in the container, made on first use. */
  private ensureLayer(): HTMLDivElement {
    if (this.layer?.isConnected) return this.layer;
    const layer = document.createElement('div');
    layer.className = 'prompt-lift-layer';
    layer.setAttribute('aria-hidden', 'true');
    this.container.insertBefore(layer, this.container.firstChild);
    this.layer = layer;
    return layer;
  }

  private measure(): { x: number; y: number; width: number; height: number } | null {
    const screen = this.term.element?.querySelector('.xterm-screen');
    if (!screen) return null;
    const s = screen.getBoundingClientRect();
    const c = this.container.getBoundingClientRect();
    return { x: s.left - c.left, y: s.top - c.top, width: s.width, height: s.height };
  }

  /** The history pane's bottom in the container, or null with the split
   *  closed. */
  private measureHistory(): number | null {
    const history = this.history;
    if (!history?.isConnected) return null;
    return history.getBoundingClientRect().bottom - this.container.getBoundingClientRect().top;
  }

  private place(): void {
    if (!this.on) return;
    const cell = (
      this.term as unknown as {
        dimensions?: { css?: { cell?: { width: number; height: number } } };
      }
    ).dimensions?.css?.cell;
    if (!cell?.width || !cell.height) return;
    this.frame ??= this.measure();
    const frame = this.frame;
    if (!frame) return;
    const layer = this.ensureLayer();
    layer.style.left = `${frame.x - BAND_X}px`;
    layer.style.top = `${frame.y - BAND_Y}px`;
    layer.style.width = `${frame.width + 2 * BAND_X}px`;
    layer.style.height = `${frame.height + 2 * BAND_Y}px`;
    if (this.historyBottom === undefined) this.historyBottom = this.measureHistory();
    const cut = dividerCut(frame.y - BAND_Y, this.historyBottom);
    layer.style.clipPath = cut > 0 ? `inset(${cut}px 0 0 0)` : '';
    const viewportY = this.term.buffer.active.viewportY;
    const boxes = widenNewest(
      layoutBands(
        this.tracker.extents(viewportY - 1, viewportY + this.term.rows),
        viewportY,
        cell.width,
        cell.height,
      ),
      getPromptReach(),
    );
    while (layer.children.length > boxes.length) layer.lastElementChild?.remove();
    while (layer.children.length < boxes.length) {
      const band = document.createElement('div');
      band.className = 'prompt-lift-band';
      layer.appendChild(band);
    }
    boxes.forEach((box, i) => {
      const band = layer.children[i] as HTMLDivElement;
      band.style.transform = `translate(${box.left + BAND_X}px, ${box.top + BAND_Y}px)`;
      band.style.width = `${box.width}px`;
      band.style.height = `${box.height}px`;
      // A band that steps in around your echo is an outline, not a box.
      const shape = notchedSvg(box);
      if (band.dataset.shape !== shape) {
        band.dataset.shape = shape;
        band.innerHTML = shape;
        band.classList.toggle('prompt-lift-band-notched', shape !== '');
      }
    });
  }
}
