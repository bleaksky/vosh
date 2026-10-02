import { describe, expect, it } from 'vitest';
import { Terminal } from '@xterm/xterm';
import {
  DRAG_SCROLL_INTERVAL,
  DRAG_SCROLL_MAX,
  HISTORY_TEXT,
  SplitDrag,
  autoscrollLines,
  closeAt,
  lineShift,
  listenSplitDrag,
  matchRow,
  pastEdge,
  selectSpan,
  xtermCell,
  type BufferView,
  type DragPane,
  type DragPointer,
  type DragTimers,
  type DragWindow,
  type PaneGrid,
  type WindowMouse,
} from './splitDrag';

// The drag across the xterm split, against real xterm buffers with no
// page around them: the history pane is 8 rows over the top of a live
// pane of 20, both 10 px rows from the top of the well, so the divider
// sits at 80 px. Both take the same 100 numbered lines, as both panes
// take the same output.

const CELL = { cellW: 8, cellH: 10 };

/** Write `text` and resolve once xterm parsed it. */
function write(term: Terminal, text: string): Promise<void> {
  return new Promise((resolve) => term.write(text, resolve));
}

/** `count` numbered lines from `from`, the last with no line break. */
function numbered(from: number, count: number): string {
  return Array.from({ length: count }, (_, n) => `L${String(from + n).padStart(3, '0')}`).join(
    '\r\n',
  );
}

function view(term: Terminal): BufferView {
  const b = term.buffer.active;
  return {
    cols: term.cols,
    rows: term.rows,
    viewportY: b.viewportY,
    baseY: b.baseY,
    cursorY: b.cursorY,
  };
}

/** A pane over a real xterm buffer that keeps what it was asked to select. */
class Pane implements DragPane {
  selected: { column: number; row: number; length: number } | null = null;
  selections = 0;
  constructor(
    readonly term: Terminal,
    readonly top = 0,
  ) {}
  grid(): PaneGrid {
    return { left: 0, top: this.top, ...CELL };
  }
  bufferView(): BufferView {
    return view(this.term);
  }
  scrollLines(n: number): void {
    this.term.scrollLines(n);
  }
  select(column: number, row: number, length: number): void {
    this.selected = { column, row, length };
    this.selections += 1;
  }
  markLine(row: number) {
    const b = this.term.buffer.active;
    return this.term.registerMarker(row - (b.baseY + b.cursorY));
  }
  lineText(row: number): string | null {
    return this.term.buffer.active.getLine(row)?.translateToString(true) ?? null;
  }
  /** The selected text as xterm copies it: whole rows between the ends,
   *  trailing blanks gone, a line break between rows. */
  text(): string {
    const s = this.selected;
    if (!s) return '';
    const cols = this.term.cols;
    const endAt = s.row * cols + s.column + s.length;
    let endRow = Math.floor(endAt / cols);
    let endX = endAt % cols;
    if (endX === 0 && endRow > s.row) {
      endRow -= 1;
      endX = cols;
    }
    const rows: string[] = [];
    for (let y = s.row; y <= endRow; y++) {
      const line = this.term.buffer.active.getLine(y);
      const from = y === s.row ? s.column : 0;
      const to = y === endRow ? endX : cols;
      rows.push(line?.translateToString(true, from, to) ?? '');
    }
    return rows.join('\n');
  }
  /** The last buffer row the selection reaches. */
  lastRow(): number {
    const s = this.selected;
    if (!s) return -1;
    const endAt = s.row * this.term.cols + s.column + s.length - 1;
    return Math.floor(endAt / this.term.cols);
  }
}

/** Timers the test runs by hand, which keep the interval asked for. */
function handTimers(): DragTimers & { running: number; intervals: number[] } {
  const t = {
    running: 0,
    intervals: [] as number[],
    start: (_tick: () => void, ms: number) => {
      t.running += 1;
      t.intervals.push(ms);
      return t.running;
    },
    stop: () => {
      t.running -= 1;
    },
  };
  return t;
}

function at(clientX: number, clientY: number, extra: Partial<DragPointer> = {}): DragPointer {
  return {
    clientX,
    clientY,
    button: 0,
    buttons: 1,
    detail: 1,
    shiftKey: false,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    ...extra,
  };
}

/** The numbered lines from `first` to `last`, one per row. */
function run(first: number, last: number): string {
  return numbered(first, last - first + 1).replace(/\r\n/g, '\n');
}

/** The split as the page opens it: the history pane at its bottom, then
 *  back by the live pane's rows, so its last row is the line above the
 *  live pane's first. `liveFirst` is text only the live pane holds, and
 *  `liveAfter` text it alone holds after the line `afterLine`, as the
 *  restored banner and its pad sit between the last session and this one. */
async function split(liveFirst = '', liveAfter = '', afterLine = 0) {
  const historyTerm = new Terminal({ cols: 20, rows: 8, scrollback: 1000, allowProposedApi: true });
  const liveTerm = new Terminal({ cols: 20, rows: 20, scrollback: 1000, allowProposedApi: true });
  await write(historyTerm, numbered(0, 100));
  const liveText = liveAfter
    ? `${numbered(0, afterLine + 1)}\r\n${liveAfter}${numbered(afterLine + 1, 99 - afterLine)}`
    : numbered(0, 100);
  await write(liveTerm, liveFirst + liveText);
  historyTerm.scrollToBottom();
  historyTerm.scrollLines(-liveTerm.rows);
  const history = new Pane(historyTerm);
  const live = new Pane(liveTerm);
  let open = true;
  let closes = 0;
  const timers = handTimers();
  const drag = new SplitDrag({
    history: () => (open ? history : null),
    live: () => live,
    closeSplit: () => {
      open = false;
      closes += 1;
    },
    timers,
  });
  const shut = () => {
    open = false;
  };
  return { history, live, drag, timers, closes: () => closes, shut };
}

describe('autoscrollLines', () => {
  it('scrolls nothing on the pane and one line just past an edge', () => {
    expect(autoscrollLines(0)).toBe(0);
    expect(autoscrollLines(Number.NaN)).toBe(0);
    expect(autoscrollLines(0.5)).toBe(1);
    expect(autoscrollLines(-0.5)).toBe(-1);
  });

  it('runs faster the further past, up to the cap at 50 px', () => {
    // xterm's own rule: 21 px is 7 lines a tick, as the native grid.
    expect(autoscrollLines(21)).toBe(7);
    expect(autoscrollLines(50)).toBe(DRAG_SCROLL_MAX);
    expect(autoscrollLines(400)).toBe(DRAG_SCROLL_MAX);
    expect(autoscrollLines(-400)).toBe(-DRAG_SCROLL_MAX);
    let last = 0;
    for (let px = 1; px <= 60; px++) {
      const lines = autoscrollLines(px);
      expect(lines).toBeGreaterThanOrEqual(last);
      last = lines;
    }
  });
});

describe('the drag geometry', () => {
  it('measures how far past a span the pointer is', () => {
    expect(pastEdge(50, 0, 80)).toBe(0);
    expect(pastEdge(80, 0, 80)).toBe(0);
    expect(pastEdge(101, 0, 80)).toBe(21);
    expect(pastEdge(-4, 0, 80)).toBe(-4);
  });

  it('maps a point to a cell the way xterm selects', () => {
    const v: BufferView = { cols: 20, rows: 8, viewportY: 72, baseY: 92, cursorY: 7 };
    const grid = { left: 0, top: 0, ...CELL };
    // The column boundary nearest the pointer, the row under it.
    expect(xtermCell(3, 15, grid, v)).toEqual({ x: 0, y: 73 });
    expect(xtermCell(5, 15, grid, v)).toEqual({ x: 1, y: 73 });
    // Held inside the screen, with the end of a row reachable.
    expect(xtermCell(1000, 1000, grid, v)).toEqual({ x: 20, y: 79 });
    expect(xtermCell(-10, -10, grid, v)).toEqual({ x: 0, y: 72 });
  });

  it('spans the cells between two ends in either order', () => {
    expect(selectSpan({ x: 2, y: 5 }, { x: 4, y: 7 }, 20)).toEqual({
      column: 2,
      row: 5,
      length: 42,
    });
    expect(selectSpan({ x: 4, y: 7 }, { x: 2, y: 5 }, 20)).toEqual({
      column: 2,
      row: 5,
      length: 42,
    });
    expect(selectSpan({ x: 3, y: 5 }, { x: 3, y: 5 }, 20).length).toBe(0);
  });

  it('closes where the history shows what the live pane holds under it', async () => {
    const { history, live } = await split();
    const h = history.bufferView();
    const l = live.bufferView();
    expect(lineShift(h, l)).toBe(0);
    const stop = closeAt(h, l);
    // The history's rows from there are the live pane's top rows.
    for (let row = 0; row < h.rows; row++) {
      expect(history.term.buffer.active.getLine(stop + row)?.translateToString(true)).toBe(
        live.term.buffer.active.getLine(l.viewportY + row)?.translateToString(true),
      );
    }
    // A history screen a row lower than the live one closes a row later.
    expect(closeAt(h, l, 1)).toBe(stop + 1);
    // Never past the end of the history.
    expect(closeAt({ ...h, cursorY: 7 }, { ...l, cursorY: 0 })).toBe(h.baseY);
  });
});

describe('matchRow', () => {
  /** Row text from a list, null past it. */
  const rows =
    (list: string[]) =>
    (row: number): string | null =>
      list[row] ?? null;

  it('keeps the guess when the rows there match', () => {
    const text = rows(['a', 'b', 'c', 'd']);
    expect(matchRow(text, text, 2, 2)).toBe(2);
  });

  it('finds the line the guess missed by rows only the live pane holds', () => {
    const history = rows(['a', 'b', 'c', 'd', 'e', 'f']);
    // A banner and a blank row after `c` put the guess two rows low.
    const live = rows(['a', 'b', 'c', '', '[restored]', 'd', 'e', 'f']);
    expect(matchRow(history, live, 1, 3)).toBe(1);
    // And a guess two rows high finds the line below.
    expect(matchRow(history, live, 4, 4)).toBe(6);
  });

  it('takes the repeated line whose rows around it match, not the nearest', () => {
    const history = rows(['<p>', 'one', '<p>', 'two', '<p>', 'three']);
    const live = rows(['x', 'x', '<p>', 'one', '<p>', 'two', '<p>', 'three']);
    // The guess sits on a prompt too, but the prompt between `one` and
    // `two` is two rows down.
    expect(matchRow(history, live, 2, 2)).toBe(4);
  });

  it('counts no blank row as a match around the line', () => {
    const history = rows(['', '', 'a', '', '', 'b']);
    const live = rows(['', '', 'a', '', '', 'x', 'q', 'a', 'z', 'z', 'b']);
    // Row 2 has the same four blank rows around it, which says nothing.
    // Row 7 has `b` three rows down, as the history does.
    expect(matchRow(history, live, 2, 2)).toBe(7);
  });

  it('keeps the guess when no row near it holds the line', () => {
    const history = rows(['a', 'b', 'c']);
    const live = rows(['x', 'y', 'z']);
    expect(matchRow(history, live, 1, 1)).toBe(1);
    expect(matchRow(rows([]), live, 1, 2)).toBe(2);
    // Never further out than its reach.
    const far = rows(['b', 'x', 'x', 'x', 'x']);
    expect(matchRow(history, far, 1, 4, 2)).toBe(4);
  });
});

describe('SplitDrag', () => {
  it('leaves a drag that stays on the history pane to xterm', async () => {
    const { history, drag, timers } = await split();
    expect(drag.press(at(4, 15), true)).toBe(true);
    expect(drag.state).toBe('armed');
    expect(drag.move(at(40, 75))).toBe(false);
    // Up past the top edge too: xterm scrolls the history up there.
    expect(drag.move(at(40, -30))).toBe(false);
    expect(history.selections).toBe(0);
    expect(timers.running).toBe(0);
    drag.release();
    expect(drag.state).toBe('idle');
  });

  it('never follows a press in the live half, off the text, or with a modifier', async () => {
    const { drag } = await split();
    expect(drag.press(at(4, 120), false)).toBe(false);
    expect(drag.move(at(40, 150))).toBe(false);
    expect(drag.press(at(4, 15), true)).toBe(true);
    expect(drag.press(at(4, 15, { detail: 2 }), true)).toBe(false);
    expect(drag.press(at(4, 15, { shiftKey: true }), true)).toBe(false);
    expect(drag.press(at(4, 15, { altKey: true }), true)).toBe(false);
    expect(drag.press(at(4, 15, { button: 2 }), true)).toBe(false);
    expect(drag.state).toBe('idle');
  });

  it('past the divider selects to the last history row and nothing below', async () => {
    const { history, live, drag, timers } = await split();
    drag.press(at(4, 15), true);
    // 21 px past the divider at 80 px.
    expect(drag.move(at(40, 101))).toBe(true);
    expect(drag.state).toBe('history');
    expect(timers.running).toBe(1);
    expect(timers.intervals).toEqual([DRAG_SCROLL_INTERVAL]);
    // The history shows L072 to L079, so the copy is L073 to L079.
    expect(history.text()).toBe(run(73, 79));
    expect(live.selections).toBe(0);
  });

  it('scrolls the history down and hands the selection to the live pane at the bottom', async () => {
    const { history, live, drag, timers, closes } = await split();
    drag.press(at(4, 15), true);
    drag.move(at(40, 101));
    // Seven lines a tick. The live pane's top row, under the history
    // pane, holds L080, so the history stops there.
    const stop = closeAt(history.bufferView(), live.bufferView());
    expect(stop).toBe(80);
    drag.tick();
    expect(history.bufferView().viewportY).toBe(79);
    expect(history.text()).toBe(run(73, 86));
    expect(closes()).toBe(0);
    // The rows the live half shows below the divider, from L088 on,
    // never came into the history selection.
    expect(history.lastRow()).toBeLessThan(88);
    drag.tick();
    expect(closes()).toBe(1);
    expect(drag.state).toBe('live');
    // One selection on the live pane from the same line to the pointer:
    // row 10 of the live screen is L090, to its fifth column boundary.
    expect(live.text()).toBe(`${run(73, 89)}\nL090`);
    // On down the live pane, and back up it, the run stays unbroken.
    drag.move(at(40, 45));
    expect(live.text()).toBe(`${run(73, 83)}\nL084`);
    drag.move(at(200, 195));
    expect(live.text()).toBe(run(73, 99));
    drag.release();
    expect(timers.running).toBe(0);
    expect(drag.state).toBe('idle');
  });

  it('carries the selection over by line when the live pane holds more above', async () => {
    const { live, drag } = await split('banner one\r\nbanner two\r\nbanner three\r\n');
    drag.press(at(4, 15), true);
    drag.move(at(40, 140));
    for (let n = 0; n < 10 && drag.state === 'history'; n++) drag.tick();
    expect(drag.state).toBe('live');
    expect(live.text().split('\n')[0]).toBe('L073');
    // The pointer's row 13 of the live screen holds L093.
    expect(live.text().split('\n').at(-1)).toBe('L093');
  });

  it('carries the selection over by line past rows only the live pane holds', async () => {
    // The restored banner, wrapped at 20 columns, and four pad rows sit
    // between L080 and L081 in the live pane only, below the anchor.
    const { live, drag } = await split('', '\r\n[scrollback restored]\r\n\r\n\r\n\r\n\r\n', 80);
    drag.press(at(4, 15), true);
    drag.move(at(40, 140));
    for (let n = 0; n < 10 && drag.state === 'history'; n++) drag.tick();
    expect(drag.state).toBe('live');
    const lines = live.text().split('\n');
    expect(lines[0]).toBe('L073');
    // Every numbered line once, in order, to the pointer's row 13 of the
    // live screen, L093, with the live pane's own rows where it holds them.
    expect(lines.filter((line) => /^L\d{3}$/.test(line))).toEqual(run(73, 93).split('\n'));
    expect(lines).toContain('[scrollback restored');
  });

  it('carries a history drag on when the wheel takes the history to its bottom', async () => {
    const { history, live, drag, timers, closes } = await split();
    drag.press(at(4, 15), true);
    drag.move(at(40, 101));
    expect(drag.state).toBe('history');
    // The wheel scrolls the history to the end of its buffer, and the page
    // says so before it closes the split.
    history.term.scrollToBottom();
    drag.historyBottomed();
    expect(closes()).toBe(1);
    expect(drag.state).toBe('live');
    expect(timers.running).toBe(1);
    // Row 10 of the live screen is L090, as at the end of a drag's own scroll.
    expect(live.text()).toBe(`${run(73, 89)}\nL090`);
    drag.move(at(200, 195));
    expect(live.text()).toBe(run(73, 99));
  });

  it('takes a drag over from xterm when the wheel bottoms the history under it', async () => {
    const { history, live, drag, timers, closes } = await split();
    drag.press(at(4, 15), true);
    // Still on the history pane, so xterm drags.
    expect(drag.move(at(40, 45))).toBe(false);
    history.term.scrollToBottom();
    drag.historyBottomed();
    // Selecting in the history ended xterm's own drag there first.
    expect(history.selections).toBe(1);
    expect(closes()).toBe(1);
    expect(drag.state).toBe('live');
    expect(timers.running).toBe(1);
    // Row 4 of the live screen holds L084.
    expect(live.text()).toBe(`${run(73, 83)}\nL084`);
    drag.release();
    expect(timers.running).toBe(0);
  });

  it('does nothing when the history bottoms with no drag on it', async () => {
    const { live, drag, closes } = await split();
    drag.historyBottomed();
    expect(drag.state).toBe('idle');
    expect(closes()).toBe(0);
    expect(live.selections).toBe(0);
  });

  it('scrolls the history back up past the top once it drags there', async () => {
    const { history, drag } = await split();
    drag.press(at(4, 15), true);
    drag.move(at(40, 101));
    drag.move(at(40, -21));
    // Above the top the selection reaches the start of the top row.
    expect(history.text()).toBe('L072');
    drag.tick();
    expect(history.bufferView().viewportY).toBe(65);
    expect(history.text().split('\n')[0]).toBe('L065');
    // Never above the top of the buffer.
    for (let n = 0; n < 20; n++) drag.tick();
    expect(history.bufferView().viewportY).toBe(0);
  });

  it('ends the drag when the button went up elsewhere or the split went', async () => {
    const { drag, timers, shut } = await split();
    drag.press(at(4, 15), true);
    drag.move(at(40, 101));
    expect(drag.move(at(40, 101, { buttons: 0 }))).toBe(false);
    expect(drag.state).toBe('idle');
    expect(timers.running).toBe(0);
    // A middle click or a find closes the split under the drag.
    drag.press(at(4, 15), true);
    drag.move(at(40, 101));
    shut();
    drag.tick();
    expect(drag.state).toBe('idle');
    expect(timers.running).toBe(0);
  });
});

describe('listenSplitDrag', () => {
  /** A window that keeps its listeners, and targets that are or are not
   *  on the history pane's text. */
  function fakeWindow() {
    const listeners = new Map<string, { fn: (e: WindowMouse) => void; capture: boolean }>();
    const win: DragWindow = {
      addEventListener: (type, fn, capture) => listeners.set(type, { fn, capture }),
      removeEventListener: (type, fn) => {
        if (listeners.get(type)?.fn === fn) listeners.delete(type);
      },
    };
    const send = (type: string, p: DragPointer, onText = false) =>
      listeners.get(type)?.fn({
        ...p,
        target: {
          closest: (selector: string) => (onText && selector === HISTORY_TEXT ? {} : null),
        },
      });
    return { win, listeners, send };
  }

  it('drives a drag from the window in the capture phase', async () => {
    const { history, drag } = await split();
    const { win, listeners, send } = fakeWindow();
    const stop = listenSplitDrag(win, drag, () => false);
    expect([...listeners.values()].every((l) => l.capture)).toBe(true);
    send('mousedown', at(4, 15), true);
    expect(drag.state).toBe('armed');
    send('mousemove', at(40, 101));
    expect(drag.state).toBe('history');
    expect(history.text()).toBe(run(73, 79));
    send('mouseup', at(40, 101, { buttons: 0 }));
    expect(drag.state).toBe('idle');
    stop();
    expect(listeners.size).toBe(0);
  });

  it('starts nothing off the history text or under the native grid', async () => {
    const { drag } = await split();
    const { win, send } = fakeWindow();
    let native = false;
    const stop = listenSplitDrag(win, drag, () => native);
    send('mousedown', at(4, 120), false);
    expect(drag.state).toBe('idle');
    native = true;
    send('mousedown', at(4, 15), true);
    expect(drag.state).toBe('idle');
    stop();
  });
});
