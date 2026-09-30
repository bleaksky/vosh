// Bands under your prompts in xterm, while your prompt shows lifted.
//
// The session wraps each prompt in two private marks: ESC ] 7717 ; l ; L
// BEL before its first line and ESC ] 7717 ; e ; L BEL after its last
// visible byte (crates/prompt/src/stage.rs). A LiftTracker reads them as
// xterm parses them. For each lift it keeps a marker on the first row of
// the logical line it starts in, where it starts within that logical line,
// and how many logical lines later and how far in it ends. A marker on the
// logical line's first row survives a reflow that joins or splits its
// wrapped rows, and walking the wrapped rows again from it finds the lift
// at any width.
//
// layoutBands turns lifts into rectangles the way the prompt boards draw
// the edit band: 4 px past the first and last glyph of the widest row, 2 px
// above the first row and below the last, radius 4. Two lifts on adjacent
// rows keep 2 px of ground between them. BandLayer draws them in a layer
// under xterm's text, repositioned in xterm's own render frame so band and
// text land in the same composite.

import type { IBufferCell, IDisposable, IMarker, Terminal } from '@xterm/xterm';
import { REGION_OSC } from './terminalRegion';

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
}

/** A band in CSS px, relative to the first row of the viewport. */
export interface BandBox {
  id: number;
  left: number;
  top: number;
  width: number;
  height: number;
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

  private begin(id: number): void {
    const buffer = this.term.buffer.active;
    const { row, offset } = this.cursorLogical();
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
      }
      if (right <= left) continue;
      out.push({ id: lift.id, top: startRow, bottom: endRow, left, right });
    }
    return out;
  }
}

/** The attribute on the terminal area that clears xterm's ground so the
 *  bands show under the text. An attribute and not a class, since App.tsx
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

/** The band outsets the prompt boards measure. */
export const BAND_X = 4;
export const BAND_Y = 2;
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
    return { id: lift.id, left, top, width: right - left, height: bottom - top };
  });
}

/** Draws the bands of a LiftTracker in a layer under an xterm's text.
 *
 *  The layer sits in the terminal area, under the terminal well, because
 *  the well and xterm clip at the text's edge and the band reaches 4 px
 *  past it. It is clipped to the text's rectangle plus the band's reach,
 *  so a lift the viewport cuts shows its visible part, square at the cut.
 *  xterm's own ground has to be clear for it to show, which Terminal.tsx
 *  sets while your prompt shows lifted. */
export class BandLayer implements IDisposable {
  private readonly term: Terminal;
  private readonly tracker: LiftTracker;
  private readonly container: HTMLElement;
  private layer: HTMLDivElement | null = null;
  private readonly subs: IDisposable[] = [];
  private readonly observer: ResizeObserver | null;
  private on = false;
  /** Where xterm's first cell sits in the container, and the text's size,
   *  measured when the layout may have moved. */
  private frame: { x: number; y: number; width: number; height: number } | null = null;

  constructor(term: Terminal, tracker: LiftTracker, container: HTMLElement) {
    this.term = term;
    this.tracker = tracker;
    this.container = container;
    // xterm fires onRender in the frame it draws the rows in, so the
    // bands move with the text, scrolling and reflowing included.
    this.subs.push(term.onRender(() => this.place()));
    this.subs.push(term.onResize(() => (this.frame = null)));
    this.observer =
      typeof ResizeObserver === 'undefined'
        ? null
        : new ResizeObserver(() => {
            this.frame = null;
            this.place();
          });
    this.observer?.observe(container);
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
    this.layer?.remove();
    this.layer = null;
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
    const viewportY = this.term.buffer.active.viewportY;
    const boxes = layoutBands(
      this.tracker.extents(viewportY - 1, viewportY + this.term.rows),
      viewportY,
      cell.width,
      cell.height,
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
    });
  }
}
