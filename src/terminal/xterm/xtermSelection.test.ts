import type { Terminal } from '@xterm/xterm';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { watchSelection } from './xtermSelection';

/** The part of an xterm the selection watch reads. It shows 14 rows from
 *  row 100 and holds a selection over rows 105 to 110. */
function fakeTerm() {
  const scrolled: (() => void)[] = [];
  const term = {
    rows: 14,
    buffer: { active: { viewportY: 100, baseY: 400 } },
    selection: { start: 105, end: 110 } as { start: number; end: number } | null,
    cleared: 0,
    onScroll(cb: () => void) {
      scrolled.push(cb);
      return { dispose: () => scrolled.splice(scrolled.indexOf(cb), 1) };
    },
    onSelectionChange: () => ({ dispose() {} }),
    getSelectionPosition() {
      const sel = term.selection;
      return sel ? { start: { x: 0, y: sel.start }, end: { x: 0, y: sel.end } } : undefined;
    },
    clearSelection() {
      term.selection = null;
      term.cleared++;
    },
    /** Scroll by `n` rows, as xterm's drag scroll does, firing onScroll. */
    scroll(n: number) {
      term.buffer.active.viewportY += n;
      for (const cb of [...scrolled]) cb();
    },
  };
  return term;
}

function watch(term: ReturnType<typeof fakeTerm>) {
  return watchSelection(term as unknown as Terminal, true, { current: true }, 1);
}

describe('watchSelection', () => {
  beforeEach(() => {
    vi.stubGlobal('window', { addEventListener() {}, removeEventListener() {} });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('leaves a selection that a drag scroll carries along', async () => {
    const term = fakeTerm();
    const w = watch(term);
    // A drag 50 px above the history pane scrolls 15 rows up, past every
    // row the selection held. xterm moves the selection's top to the new
    // top only once the scroll returns, and a clear inside it threw.
    term.scroll(-15);
    expect(term.cleared).toBe(0);
    term.selection = { start: term.buffer.active.viewportY, end: 110 };
    await Promise.resolve();
    expect(term.cleared).toBe(0);
    w.scroll.dispose();
    w.removeCopyKey();
  });

  it('clears a selection the scroll leaves out of view', async () => {
    const term = fakeTerm();
    const w = watch(term);
    term.scroll(20);
    await Promise.resolve();
    expect(term.cleared).toBe(1);
    w.scroll.dispose();
    w.removeCopyKey();
  });

  it('clears nothing once the pane is gone', async () => {
    const term = fakeTerm();
    const w = watch(term);
    term.scroll(20);
    w.scroll.dispose();
    await Promise.resolve();
    expect(term.cleared).toBe(0);
    w.removeCopyKey();
  });
});
