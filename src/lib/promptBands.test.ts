import { describe, expect, it } from 'vitest';
import { Terminal } from '@xterm/xterm';
import promptCss from '../styles/prompt.css?raw';
import {
  BAND_X,
  BAND_Y,
  BAND_Y_ADJACENT,
  LIFTED_ATTR,
  layoutBands,
  LiftTracker,
  markLifted,
  type LiftExtent,
} from './promptBands';
import { RegionWriter } from './terminalRegion';

// The lift marks against a real xterm with no page around it, through the
// same writer the terminal uses, so region marks and lift marks share the
// parser as they do in the app.

const mark = (gen: number) => `\x1b]7717;o;${gen}\x07`;
const start = (id: number) => `\x1b]7717;l;${id}\x07`;
const end = (id: number) => `\x1b]7717;e;${id}\x07`;

function setup(cols = 40, rows = 10, scrollback = 100, cap?: number) {
  const term = new Terminal({ cols, rows, scrollback, allowProposedApi: true });
  const writer = new RegionWriter(term);
  const lifts = new LiftTracker(term, cap);
  return { term, writer, lifts };
}

function parsed(writer: RegionWriter): Promise<void> {
  return new Promise((resolve) => writer.whenParsed(resolve));
}

/** Every lift on the buffer, relative to the viewport's first row. */
function extents(term: Terminal, lifts: LiftTracker): LiftExtent[] {
  const top = term.buffer.active.viewportY;
  return lifts.extents(0, term.buffer.active.length).map((e) => ({
    ...e,
    top: e.top - top,
    bottom: e.bottom - top,
  }));
}

describe('LiftTracker', () => {
  it('reads a lift from its marks, lines above the design included', async () => {
    const { term, writer, lifts } = setup();
    writer.output({
      text: `room\r\n\r\n${start(1)}Tester: [===]\r\n${mark(2)}<1020hp>${end(1)} `,
    });
    writer.local('look\r\n');
    await parsed(writer);
    expect(extents(term, lifts)).toEqual([{ id: 1, top: 2, bottom: 3, left: 0, right: 13 }]);
  });

  it('stops at the end mark, before the space and your echo after it', async () => {
    const { term, writer, lifts } = setup();
    writer.output({ text: `${start(1)}${mark(2)}<1020hp>${end(1)} ` });
    writer.local('fight\r\n');
    await parsed(writer);
    expect(extents(term, lifts)).toEqual([{ id: 1, top: 0, bottom: 0, left: 0, right: 8 }]);
    // The region marks still reach the writer, so a repaint lands.
    const { term: t2, writer: w2, lifts: l2 } = setup();
    w2.output({ text: `${start(1)}${mark(2)}<1020hp>${end(1)} ` });
    w2.output({
      text: '',
      replace: { gen: 2, text: `${mark(3)}<999hp 800m>${end(1)}`, fresh: false },
    });
    await parsed(w2);
    expect(t2.buffer.active.getLine(0)?.translateToString(true)).toBe('<999hp 800m>');
    // A repaint's end mark moves the lift's end.
    expect(extents(t2, l2)).toEqual([{ id: 1, top: 0, bottom: 0, left: 0, right: 12 }]);
  });

  it('finds a lift again after a reflow at a new width', async () => {
    const { term, writer, lifts } = setup(20);
    const long = '1020/1020hp 800/800mn 930/930mv';
    writer.output({ text: `before\r\n${start(1)}${mark(2)}${long}${end(1)}\r\nafter\r\n` });
    await parsed(writer);
    // Wrapped at 20 the lift takes two rows.
    expect(extents(term, lifts)).toEqual([{ id: 1, top: 1, bottom: 2, left: 0, right: 20 }]);
    term.resize(40, 10);
    await parsed(writer);
    expect(extents(term, lifts)).toEqual([
      { id: 1, top: 1, bottom: 1, left: 0, right: long.length },
    ]);
    term.resize(12, 10);
    await parsed(writer);
    const [lift] = extents(term, lifts);
    expect(lift.bottom - lift.top).toBe(2);
    expect(lift.right).toBe(12);
  });

  it('keeps the newest lifts up to its cap', async () => {
    const { term, writer, lifts } = setup(40, 10, 100, 3);
    for (let i = 1; i <= 5; i++) writer.output({ text: `${start(i)}p${i}${end(i)}\r\n` });
    await parsed(writer);
    expect(lifts.size).toBe(3);
    expect(extents(term, lifts).map((e) => e.id)).toEqual([3, 4, 5]);
  });

  it('forgets a lift once its first line leaves the scrollback', async () => {
    const { term, writer, lifts } = setup(40, 5, 5);
    writer.output({ text: `${start(1)}prompt${end(1)}\r\n` });
    writer.output({ text: 'line\r\n'.repeat(20) });
    await parsed(writer);
    expect(lifts.size).toBe(0);
    expect(extents(term, lifts)).toEqual([]);
  });

  it('draws nothing for a lift with no end or nothing shown', async () => {
    const { term, writer, lifts } = setup();
    writer.output({ text: `${start(1)}no end yet\r\n${start(2)}${end(2)}\r\n` });
    await parsed(writer);
    expect(extents(term, lifts)).toEqual([]);
  });
});

describe('layoutBands', () => {
  const cell = { w: 7.8, h: 17.5 };

  it('reaches 4 px past the text and 2 px above and below, as the boards draw it', () => {
    const [band] = layoutBands(
      [{ id: 1, top: 3, bottom: 3, left: 0, right: 35 }],
      0,
      cell.w,
      cell.h,
    );
    expect(band.left).toBe(-BAND_X);
    expect(band.top).toBe(3 * cell.h - BAND_Y);
    expect(band.width).toBeCloseTo(35 * cell.w + 2 * BAND_X, 6);
    expect(band.height).toBe(cell.h + 2 * BAND_Y);
    // P4 measures 281 wide by 21.5 tall for 35 cells.
    expect(band.width).toBeCloseTo(281, 6);
    expect(band.height).toBe(21.5);
  });

  it('spans every row of a lift as one band', () => {
    const [band] = layoutBands(
      [{ id: 1, top: 2, bottom: 3, left: 0, right: 59 }],
      0,
      cell.w,
      cell.h,
    );
    // P8b measures 39 tall for two rows.
    expect(band.height).toBe(39);
    expect(band.width).toBeCloseTo(59 * cell.w + 8, 6);
  });

  it('keeps 2 px of ground between lifts on adjacent rows', () => {
    const [a, b] = layoutBands(
      [
        { id: 2, top: 5, bottom: 5, left: 0, right: 10 },
        { id: 1, top: 4, bottom: 4, left: 0, right: 10 },
      ],
      0,
      cell.w,
      cell.h,
    );
    expect(a.id).toBe(1);
    expect(a.top + a.height).toBe(5 * cell.h + BAND_Y_ADJACENT);
    expect(b.top).toBe(5 * cell.h - BAND_Y_ADJACENT);
    expect(b.top - (a.top + a.height)).toBe(2);
    // The outer edges keep the full reach.
    expect(a.top).toBe(4 * cell.h - BAND_Y);
    expect(b.top + b.height).toBe(6 * cell.h + BAND_Y);
  });

  it('places lifts against the viewport, cut ones partly above it', () => {
    const [band] = layoutBands([{ id: 1, top: 8, bottom: 10, left: 2, right: 6 }], 9, 10, 20);
    expect(band.top).toBe(-20 - BAND_Y);
    expect(band.left).toBe(2 * 10 - BAND_X);
    expect(band.height).toBe(3 * 20 + 2 * BAND_Y);
  });
});

describe('the clear ground under lifted bands', () => {
  // App.tsx writes the terminal area's className whole whenever the
  // scrollback split opens or closes, so a class the pane added itself
  // would go and every band would sit under xterm's opaque ground. The
  // mark is an attribute React never writes on that element.
  it('marks the terminal area with an attribute, not a class', () => {
    const attrs = new Set<string>();
    const area = {
      className: 'terminal-area',
      toggleAttribute: (name: string, on: boolean) => {
        if (on) attrs.add(name);
        else attrs.delete(name);
        return on;
      },
    };
    markLifted(area, true);
    area.className = 'terminal-area terminal-area-split';
    area.className = 'terminal-area';
    expect(area.className).not.toContain('lifted');
    expect([...attrs]).toEqual([LIFTED_ATTR]);
    markLifted(area, false);
    expect([...attrs]).toEqual([]);
  });

  it('clears the ground from that attribute, never from a class', () => {
    for (const part of ['.terminal-host', '.xterm .xterm-viewport', '.xterm .xterm-screen']) {
      expect(promptCss).toContain(`.terminal-area[${LIFTED_ATTR}] .terminal-pane-live ${part}`);
    }
    expect(promptCss).not.toMatch(/\.prompt-lifted\b/);
  });
});
