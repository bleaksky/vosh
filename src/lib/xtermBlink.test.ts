import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Terminal } from '@xterm/xterm';
import { BLINK_MS } from './blink';
import { sgrBlink, XtermBlink } from './xtermBlink';

/** A stand in for the parts of xterm the aligner reads: a screen of
 *  cells that blink or not, its SGR hook and its render event, and the
 *  values the blink interval takes, in order. */
function fakeTerm(rows: number, cols: number) {
  const screen: boolean[][] = Array.from({ length: rows }, () => Array(cols).fill(false));
  let sgr: ((params: (number | number[])[]) => boolean) | null = null;
  let render: ((e: { start: number; end: number }) => void) | null = null;
  const intervals: number[] = [];
  const disposed: string[] = [];
  let reads = 0;
  const term = {
    rows,
    options: {
      set blinkIntervalDuration(ms: number) {
        intervals.push(ms);
      },
    },
    buffer: {
      active: {
        viewportY: 0,
        getLine: (y: number) => {
          reads++;
          const row = screen[y];
          if (!row) return undefined;
          return {
            length: cols,
            getCell: (x: number) => ({ isBlink: () => (row[x] ? 1 : 0) }),
          };
        },
      },
    },
    parser: {
      registerCsiHandler: (
        id: { final: string },
        cb: (params: (number | number[])[]) => boolean,
      ) => {
        expect(id).toEqual({ final: 'm' });
        sgr = cb;
        return {
          dispose: () => {
            if (sgr === cb) sgr = null;
            disposed.push('sgr');
          },
        };
      },
    },
    onRender: (cb: (e: { start: number; end: number }) => void) => {
      render = cb;
      return { dispose: () => disposed.push('render') };
    },
    onResize: () => ({ dispose: () => disposed.push('resize') }),
  };
  return {
    term: term as unknown as Terminal,
    screen,
    intervals,
    disposed,
    /** xterm parses an SGR, through the hook while it is there. The
     *  aligner never takes it over. */
    sgr: (params: (number | number[])[]) => {
      if (sgr) expect(sgr(params)).toBe(false);
    },
    /** The SGR hook is there. */
    hooked: () => sgr !== null,
    /** How many screen rows the aligner has read. */
    reads: () => reads,
    /** xterm draws rows `start` to `end`. */
    render: (start = 0, end = rows - 1) => render?.({ start, end }),
  };
}

describe('the SGR hook', () => {
  it('reads 5 as blink on and 0 or 25 as blink off, and neither 6 nor a number a color takes', () => {
    expect(sgrBlink([5])).toBe(true);
    expect(sgrBlink([1, 5])).toBe(true);
    // xterm has no SGR 6, so the rapid blink draws steady.
    expect(sgrBlink([1, 6])).toBe(null);
    expect(sgrBlink([0])).toBe(false);
    expect(sgrBlink([1, 25])).toBe(false);
    // The last of them counts.
    expect(sgrBlink([5, 0])).toBe(false);
    expect(sgrBlink([0, 5])).toBe(true);
    expect(sgrBlink([38, 5, 5])).toBe(null);
    expect(sgrBlink([38, 5, 0])).toBe(null);
    expect(sgrBlink([48, 2, 5, 5, 5])).toBe(null);
    expect(sgrBlink([58, 5, 6, 1])).toBe(null);
    expect(sgrBlink([38, [5, 5]])).toBe(null);
    expect(sgrBlink([38, 5, 196, 5])).toBe(true);
    expect(sgrBlink([0, 38, 5, 5])).toBe(false);
  });
});

describe('XtermBlink', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // A moment in the hidden half, 700 ms into a cycle.
    vi.setSystemTime(10 * 2 * BLINK_MS + 700);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('sets the blink interval to the shared rate while on, and to none while off', () => {
    const t = fakeTerm(3, 4);
    const blink = new XtermBlink(t.term);
    blink.setWebgl(true);
    expect(t.intervals).toEqual([]);
    blink.setOn(true);
    blink.setOn(false);
    expect(t.intervals).toEqual([BLINK_MS, 0]);
  });

  it('blinks only while WebGL draws the pane', () => {
    // The DOM renderer hides steady text after a blinking span in the
    // hidden half, so a DOM pane draws blinking text steady.
    const t = fakeTerm(3, 4);
    const blink = new XtermBlink(t.term);
    blink.setOn(true);
    t.sgr([5]);
    t.screen[0][0] = true;
    t.render();
    expect(t.intervals).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
    // WebGL takes over, then loses its context and hands back to DOM.
    blink.setWebgl(true);
    expect(t.intervals).toEqual([BLINK_MS]);
    expect(vi.getTimerCount()).toBe(1);
    blink.setWebgl(false);
    expect(t.intervals).toEqual([BLINK_MS, 0]);
    expect(vi.getTimerCount()).toBe(0);
    blink.dispose();
    // Disposed, a late context loss leaves the terminal alone.
    blink.setWebgl(true);
    blink.setWebgl(false);
    expect(t.intervals).toEqual([BLINK_MS, 0]);
  });

  it('lets its SGR hook go once a blink comes', () => {
    const t = fakeTerm(2, 2);
    const blink = new XtermBlink(t.term);
    blink.setOn(true);
    blink.setWebgl(true);
    t.sgr([1, 31]);
    t.sgr([38, 5, 5]);
    expect(t.hooked()).toBe(true);
    t.sgr([5]);
    expect(t.hooked()).toBe(false);
    expect(t.disposed).toEqual(['sgr']);
    // Every render reads the screen from here.
    t.screen[1][1] = true;
    t.render(1, 1);
    expect(vi.getTimerCount()).toBe(1);
    blink.dispose();
    expect(t.disposed.sort()).toEqual(['render', 'resize', 'sgr']);
  });

  it('reads no render once blink has left the screen and an SGR has ended it', () => {
    const t = fakeTerm(2, 3);
    const blink = new XtermBlink(t.term);
    blink.setOn(true);
    blink.setWebgl(true);
    // A 5 comes on its own, its text in a later read.
    t.sgr([5]);
    expect(t.hooked()).toBe(false);
    t.render();
    expect(t.hooked()).toBe(true);
    t.screen[0][0] = true;
    t.render(0, 0);
    expect(vi.getTimerCount()).toBe(1);
    // The blinking text scrolls away while blink is still on, and text
    // written in it later still shows up.
    t.screen[0][0] = false;
    t.render();
    expect(vi.getTimerCount()).toBe(0);
    t.screen[1][2] = true;
    t.render(1, 1);
    expect(vi.getTimerCount()).toBe(1);
    // A reset ends the blink. Once its text has gone too, renders go
    // unread.
    t.sgr([0]);
    t.screen[1][2] = false;
    t.render();
    const reads = t.reads();
    t.render();
    expect(t.reads()).toBe(reads);
    expect(vi.getTimerCount()).toBe(0);
    // The next 5 reads them again.
    t.sgr([0, 5]);
    t.screen[0][1] = true;
    t.render();
    expect(t.reads()).toBeGreaterThan(reads);
    expect(vi.getTimerCount()).toBe(1);
    blink.dispose();
    expect(t.hooked()).toBe(false);
  });

  it('runs no timer while nothing on screen blinks', () => {
    const t = fakeTerm(3, 4);
    const blink = new XtermBlink(t.term);
    blink.setWebgl(true);
    blink.setOn(true);
    // 256 colors never open the scan, and a screen with no blink keeps
    // it idle.
    t.sgr([38, 5, 208]);
    t.render();
    expect(vi.getTimerCount()).toBe(0);
    t.sgr([5]);
    t.render();
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(60_000);
    expect(t.intervals).toEqual([BLINK_MS]);
  });

  it('puts blinking text on the shared clock at its next shown half, and stops when it leaves', () => {
    const t = fakeTerm(3, 4);
    const blink = new XtermBlink(t.term);
    blink.setOn(true);
    blink.setWebgl(true);
    t.sgr([5]);
    t.screen[1][2] = true;
    t.render(1, 1);
    expect(vi.getTimerCount()).toBe(1);
    // 500 ms from 700 ms into the cycle, the next shown half starts, and
    // xterm's interval starts again from there.
    vi.advanceTimersByTime(499);
    expect(t.intervals).toEqual([BLINK_MS]);
    vi.advanceTimersByTime(1);
    expect(t.intervals).toEqual([BLINK_MS, 0, BLINK_MS]);
    // While it shows, it goes back on the clock now and then.
    vi.advanceTimersByTime(12 * BLINK_MS * 2);
    expect(t.intervals.length).toBeGreaterThan(3);
    // Scrolled away, no timer is left.
    t.screen[1][2] = false;
    t.render();
    expect(vi.getTimerCount()).toBe(0);
    const before = t.intervals.length;
    vi.advanceTimersByTime(60_000);
    expect(t.intervals.length).toBe(before);
  });

  it('reads blink through the real xterm parser and buffer', () => {
    // xterm 6.1 keeps SGR 5 on the cell and hands the SGR to the hook.
    // A terminal never opened draws nothing, so a resize stands in for
    // the render that reads the screen.
    const term = new Terminal({ cols: 8, rows: 2, allowProposedApi: true });
    const blink = new XtermBlink(term);
    blink.setOn(true);
    blink.setWebgl(true);
    expect(term.options.blinkIntervalDuration).toBe(BLINK_MS);
    term.write('\x1b[38;5;5mA\x1b[0m');
    vi.advanceTimersByTime(1);
    term.resize(8, 3);
    expect(vi.getTimerCount()).toBe(0);
    term.write('\x1b[5mB\x1b[25mC');
    // The hook let go inside its own call, and xterm reads every SGR
    // after it as before.
    term.write('\x1b[1;5mD\x1b[0m');
    vi.advanceTimersByTime(1);
    const line = term.buffer.active.getLine(0);
    expect(line?.getCell(0)?.isBlink()).toBe(0);
    expect(line?.getCell(1)?.isBlink()).not.toBe(0);
    expect(line?.getCell(2)?.isBlink()).toBe(0);
    expect(line?.getCell(3)?.isBlink()).not.toBe(0);
    expect(line?.getCell(3)?.isBold()).not.toBe(0);
    term.resize(8, 2);
    expect(vi.getTimerCount()).toBe(1);
    blink.setOn(false);
    expect(term.options.blinkIntervalDuration).toBe(0);
    blink.dispose();
    term.dispose();
  });

  it('drops its timer when blinking text goes off, and its hooks on dispose', () => {
    const t = fakeTerm(2, 2);
    const blink = new XtermBlink(t.term);
    blink.setOn(true);
    blink.setWebgl(true);
    t.sgr([5]);
    t.screen[0][0] = true;
    t.render();
    expect(vi.getTimerCount()).toBe(1);
    blink.setOn(false);
    expect(vi.getTimerCount()).toBe(0);
    // Off, a render scans nothing and arms nothing.
    t.render();
    expect(vi.getTimerCount()).toBe(0);
    blink.dispose();
    expect(t.disposed.sort()).toEqual(['render', 'resize', 'sgr']);
  });
});
