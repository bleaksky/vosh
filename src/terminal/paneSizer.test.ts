import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FitAddon } from '@xterm/addon-fit';
import type { Terminal } from '@xterm/xterm';
import { PaneSizer, type SizedPane } from './paneSizer';

const native = vi.hoisted(() => ({ on: false }));
vi.mock('./terminalRenderer', () => ({ nativeSurfaceEnabled: () => native.on }));
vi.mock('../ipc/nativeSurface', () => ({
  nativeSurfaceSetBounds: () => Promise.resolve(),
  nativeSurfaceSetCellMetrics: () => Promise.resolve(),
}));

beforeEach(() => {
  native.on = false;
});

/** A pane the FitAddon would fit at 153 by 40, as xterm's own cell and
 *  scrollbar count it. */
function pane(quiet: boolean) {
  const resize = vi.fn();
  const sized: SizedPane = {
    term: { rows: 40, cols: 156, dimensions: undefined } as unknown as Terminal,
    fit: { proposeDimensions: () => ({ cols: 153, rows: 40 }) } as unknown as FitAddon,
    sizer: null,
    host: {} as HTMLDivElement,
    resize,
    lent: () => 0,
    anchor: () => false,
    quiet: () => quiet,
    shown: () => true,
    onCellSize: () => undefined,
  };
  return { sizer: new PaneSizer(sized), resize };
}

describe('PaneSizer.fitKept', () => {
  it('fits xterm to its pane under xterm', () => {
    const { sizer, resize } = pane(false);
    sizer.fitKept();
    expect(resize).toHaveBeenCalledWith(153, 40);
  });

  it('leaves the live pane to the native grid, so xterm keeps the columns the game is told', () => {
    // A panel that opens or a font that changes fits through here. Under
    // the native surface that fit kept xterm at 153 while the grid, and
    // the game, had 156, and the pinned band cut a pushed row short.
    native.on = true;
    const live = pane(false);
    live.sizer.fitKept();
    live.sizer.safeFit();
    expect(live.resize).not.toHaveBeenCalled();
    // The history pane has no grid of its own and still fits.
    const history = pane(true);
    history.sizer.fitKept();
    expect(history.resize).toHaveBeenCalledWith(153, 40);
  });
});
