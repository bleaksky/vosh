import { describe, expect, it } from 'vitest';
import { Terminal } from '@xterm/xterm';
import bandCases from '../../../fixtures/prompt-bands/cases.json';
import promptCss from '../../styles/prompt.css?raw';
import liftBandsSource from './liftBands.ts?raw';
import {
  BAND_RADIUS,
  BAND_X,
  BAND_Y,
  BAND_Y_ADJACENT,
  dividerCut,
  LIFTED_ATTR,
  layoutBands,
  LiftTracker,
  markLifted,
  notchedPath,
  widenNewest,
  type LiftExtent,
} from './liftBands';
import { OutputShaper } from '../outputShaper';
import { RegionWriter } from '../terminalRegion';
import type { SessionOutput } from '../../ipc/terminal';

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
    // Your echo follows the shorter last row, so the band steps in there.
    expect(extents(term, lifts)).toEqual([
      { id: 1, top: 2, bottom: 3, left: 0, right: 13, notch: 8 },
    ]);
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

describe('LiftTracker on a change of where your prompt shows', () => {
  const tank = 'Tester: [===|---]';

  it('keeps a lift that starts above its region through a repaint that starts it again', async () => {
    const { term, writer, lifts } = setup();
    // Lifted when you chose it: the lift starts at the tank line.
    writer.output({ text: `room\r\n\r\n${start(1)}${tank}\r\n${mark(2)}<765>${end(1)} ` });
    // A later repaint starts it again inside the region.
    writer.output({
      text: '',
      replace: { gen: 2, text: `${mark(3)}${start(1)}<700>${end(1)} `, fresh: false },
    });
    await parsed(writer);
    expect(extents(term, lifts)).toEqual([{ id: 1, top: 2, bottom: 3, left: 0, right: 17 }]);
  });

  it('forgets the lifts a replace that writes nothing erased', async () => {
    const { term, writer, lifts } = setup();
    writer.onErase((row, col) => lifts.dropFrom(row, col));
    writer.output({ text: `room\r\n${start(1)}${tank}\r\n${mark(2)}<765>${end(1)} ` });
    writer.output({
      text: '',
      replace: { gen: 2, text: '', fresh: false, above: { plain: tank, text: '' } },
    });
    // Text that lands where the prompt was never takes its band.
    writer.output({ text: 'Joral tells you something\r\nand more\r\n' });
    await parsed(writer);
    expect(lifts.size).toBe(0);
    expect(extents(term, lifts)).toEqual([]);
  });
});

describe('LiftTracker with your echo after a prompt of several rows', () => {
  const tank = 'Tester: [===|---]';
  const guard = 'a Blackwatch guard 54% quite a few wounds';
  const fight = `${start(1)}${tank}\r\n${mark(2)}${guard}\r\n<765hp>${end(1)} `;

  it('notes where the last row ends when your echo follows it', async () => {
    const { term, writer, lifts } = setup(60);
    writer.output({ text: fight });
    writer.local('\x1b[93mflee\x1b[0m\r\n');
    await parsed(writer);
    expect(extents(term, lifts)).toEqual([
      { id: 1, top: 0, bottom: 2, left: 0, right: guard.length, notch: 7 },
    ]);
  });

  it('keeps one rectangle while nothing follows on the last row', async () => {
    const { term, writer, lifts } = setup(60);
    writer.output({ text: `${fight}\r\nJoral tells you 'hi'\r\n` });
    await parsed(writer);
    expect(extents(term, lifts)).toEqual([
      { id: 1, top: 0, bottom: 2, left: 0, right: guard.length },
    ]);
  });

  it('keeps one rectangle when the last row is the widest', async () => {
    const { term, writer, lifts } = setup(60);
    writer.output({ text: `${start(1)}${tank}\r\n${mark(2)}${guard}${end(1)} ` });
    writer.local('flee\r\n');
    await parsed(writer);
    expect(extents(term, lifts)).toEqual([
      { id: 1, top: 0, bottom: 1, left: 0, right: guard.length },
    ]);
  });

  it('notches a one row prompt a narrow terminal wrapped', async () => {
    const { term, writer, lifts } = setup(20);
    writer.output({ text: `${start(1)}${mark(2)}1020/1020hp 800/800mn 930/930mv${end(1)} ` });
    writer.local('flee\r\n');
    await parsed(writer);
    expect(extents(term, lifts)).toEqual([
      { id: 1, top: 0, bottom: 1, left: 0, right: 20, notch: 11 },
    ]);
  });
});

describe('LiftTracker with a prompt pushed to the right edge', () => {
  // `<%hp>%{right}%mana!` as the session draws it at `cols` wide.
  const row = (cols: number) => `<1020>${' '.repeat(cols - 10)}800!`;
  const bytes = (text: string) => new TextEncoder().encode(text);

  it('keeps a band that fills its row on it, at each width the row draws again', async () => {
    const { term, writer, lifts } = setup(40);
    // Through the word wrap the terminal uses, which ends the row where
    // the space after the lift's end would run past it.
    const shaper = new OutputShaper(40);
    const write = (out: SessionOutput) => {
      const shaped = shaper.shape(out).output;
      if (shaped) writer.output(shaped);
    };
    const line = (n: number) => term.buffer.active.getLine(n)?.translateToString(true);
    write({ bytes: bytes(`${start(1)}${mark(2)}${row(40)}${end(1)} `) });
    await parsed(writer);
    expect(line(0)).toBe(row(40));
    expect(extents(term, lifts)).toEqual([{ id: 1, top: 0, bottom: 0, left: 0, right: 40 }]);
    // A new width draws the row again, as the session repaints it.
    term.resize(30, 10);
    shaper.setCols(30);
    write({
      bytes: bytes(''),
      replace: { gen: 2, bytes: bytes(`${mark(3)}${row(30)}${end(1)} `), fresh: false },
    });
    await parsed(writer);
    expect([line(0), line(1), line(2)]).toEqual([row(30), '', '']);
    expect(extents(term, lifts)).toEqual([{ id: 1, top: 0, bottom: 0, left: 0, right: 30 }]);
    // Your echo takes the next row, and the band stays on its own.
    writer.local('look\r\n');
    await parsed(writer);
    expect([line(0), line(1)]).toEqual([row(30), 'look']);
    expect(extents(term, lifts)).toEqual([{ id: 1, top: 0, bottom: 0, left: 0, right: 30 }]);
  });
});

describe('layoutBands', () => {
  const cell = { w: 7.8, h: 17.5 };

  it('widens the newest band to hold the card s caret, past the last glyph (P6)', () => {
    const boxes = layoutBands(
      [
        { id: 1, top: 1, bottom: 1, left: 0, right: 35 },
        { id: 4, top: 3, bottom: 3, left: 0, right: 35 },
      ],
      0,
      cell.w,
      cell.h,
    );
    // The caret waits a cell past the last glyph, 2 px wide, and the band
    // reaches 4 px past it: 16 + 36 x 7.8 + 2 + 4 - 12 = 290.8.
    const reach = cell.w + 2;
    const [old, open] = widenNewest(boxes, reach);
    expect(open.width).toBeCloseTo(290.8, 6);
    expect(old.width).toBe(boxes[0].width);
    expect(widenNewest(boxes, 0)).toBe(boxes);
    // A band that steps in around your echo keeps its width.
    const notched = [{ ...boxes[1], notch: { x: 10, y: 17.5 } }];
    expect(widenNewest(notched, reach)[0].width).toBe(boxes[1].width);
  });

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
    // 35 cells on one row measure 281 wide by 21.5 tall.
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
    // Two rows measure 39 tall.
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

  it('steps in around your echo on the last row', () => {
    const [band] = layoutBands(
      [{ id: 1, top: 2, bottom: 4, left: 0, right: 52, notch: 42 }],
      0,
      cell.w,
      cell.h,
    );
    expect(band.width).toBeCloseTo(52 * cell.w + 2 * BAND_X, 6);
    expect(band.height).toBe(3 * cell.h + 2 * BAND_Y);
    // The last row's band ends 4 px past its last glyph, and the rows
    // above keep the full width down to the last row's top.
    expect(band.notch?.x).toBeCloseTo(42 * cell.w + 2 * BAND_X, 6);
    expect(band.notch?.y).toBe(2 * cell.h + BAND_Y);
    const [plain] = layoutBands([{ id: 1, top: 2, bottom: 4, left: 0, right: 52 }], 0, 7.8, 17.5);
    expect(plain.notch).toBeUndefined();
  });

  it('draws a notched band as one outline with rounded outer corners', () => {
    const path = notchedPath(100, 60, 40, 20, 4, 0);
    expect(path).toBe(
      'M4 0H96A4 4 0 0 1 100 4V16A4 4 0 0 1 96 20H40V56A4 4 0 0 1 36 60H4A4 4 0 0 1 0 56V4A4 4 0 0 1 4 0Z',
    );
    // The light ring runs half a pixel inside it.
    expect(notchedPath(100, 60, 40, 20, 4, 0.5)).toBe(
      'M4 0.5H96A3.5 3.5 0 0 1 99.5 4V16A3.5 3.5 0 0 1 96 19.5H39.5V56A3.5 3.5 0 0 1 36 59.5H4A3.5 3.5 0 0 1 0.5 56V4A3.5 3.5 0 0 1 4 0.5Z',
    );
  });

  it('places lifts against the viewport, cut ones partly above it', () => {
    const [band] = layoutBands([{ id: 1, top: 8, bottom: 10, left: 2, right: 6 }], 9, 10, 20);
    expect(band.top).toBe(-20 - BAND_Y);
    expect(band.left).toBe(2 * 10 - BAND_X);
    expect(band.height).toBe(3 * 20 + 2 * BAND_Y);
  });
});

describe('the clear ground under lifted bands', () => {
  // MainWindow.tsx writes the terminal area's className whole whenever the
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

describe('dividerCut', () => {
  // The scrollback split lays the history pane over the live one. Its
  // text hides the live rows under it, but a band reaches 4 px past the
  // text and 2 px past its rows, so the layer is cut at the divider, as
  // the native grid cuts the live region there.
  it('cuts the layer at the history pane bottom while the split is open', () => {
    // The layer starts 2 px above the first row at 38, and the history
    // pane ends at 278.
    expect(dividerCut(36, 278)).toBe(242);
  });

  it('cuts nothing with the split closed or a pane above the layer', () => {
    expect(dividerCut(36, null)).toBe(0);
    expect(dividerCut(36, 20)).toBe(0);
  });
});

// The native grid draws the same bands (band_rects and widen_newest in
// src-tauri/src/native/gpu/bands.rs), and its test runs these cases too.
interface BandCase {
  name: string;
  cell: { w: number; h: number };
  viewport_y: number;
  reach?: number;
  lifts: { id: number; top: number; bottom: number; left: number; right: number; notch?: number }[];
  bands: {
    id: number;
    left: number;
    top: number;
    width: number;
    height: number;
    notch?: { x: number; y: number };
  }[];
}

describe('the band cases the native grid draws too', () => {
  const { constants } = bandCases;
  const cases = bandCases.cases as BandCase[];

  it('reaches as far as the native grid does', () => {
    expect({ BAND_X, BAND_Y, BAND_Y_ADJACENT, BAND_RADIUS }).toEqual({
      BAND_X: constants.band_x,
      BAND_Y: constants.band_y,
      BAND_Y_ADJACENT: constants.band_y_adjacent,
      BAND_RADIUS: constants.band_radius,
    });
    // The tracker keeps this bound to itself, and the native grid uses
    // the same one to find the lifts that reach into a region.
    expect(liftBandsSource).toMatch(
      new RegExp(`\\nconst MAX_LIFT_ROWS = ${constants.max_lift_rows};\\n`),
    );
  });

  it.each(cases.map((c) => [c.name, c] as const))('%s', (_name, c) => {
    const extents: LiftExtent[] = c.lifts.map(({ notch, ...lift }) =>
      notch === undefined ? lift : { ...lift, notch },
    );
    const got = widenNewest(layoutBands(extents, c.viewport_y, c.cell.w, c.cell.h), c.reach ?? 0);
    expect(got.map((band) => band.id)).toEqual(c.bands.map((band) => band.id));
    got.forEach((band, i) => {
      const want = c.bands[i];
      expect(band.left).toBeCloseTo(want.left, 6);
      expect(band.top).toBeCloseTo(want.top, 6);
      expect(band.width).toBeCloseTo(want.width, 6);
      expect(band.height).toBeCloseTo(want.height, 6);
      expect(band.notch === undefined).toBe(want.notch === undefined);
      if (band.notch && want.notch) {
        expect(band.notch.x).toBeCloseTo(want.notch.x, 6);
        expect(band.notch.y).toBeCloseTo(want.notch.y, 6);
      }
    });
  });
});
