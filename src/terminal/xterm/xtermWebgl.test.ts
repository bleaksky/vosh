import type { Terminal } from '@xterm/xterm';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { XtermBlink } from './xtermBlink';
import { xtermWebgl } from './xtermWebgl';

// Only the pane that shows holds a WebGL context. The
// addon stands in here, and so do the canvas the probe asks
// for a context and the storage that can turn WebGL off.

const addons = vi.hoisted(() => [] as { disposed: number }[]);

vi.mock('@xterm/addon-webgl', () => ({
  WebglAddon: class {
    disposed = 0;
    constructor() {
      addons.push(this);
    }
    onContextLoss() {}
    dispose() {
      this.disposed += 1;
    }
  },
}));

const storage = new Map<string, string>();

beforeAll(() => {
  vi.stubGlobal('document', {
    createElement: () => ({ getContext: () => ({ getExtension: () => null }) }),
  });
  vi.stubGlobal('localStorage', { getItem: (key: string) => storage.get(key) ?? null });
  vi.spyOn(console, 'log').mockImplementation(() => {});
});

afterAll(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

beforeEach(() => {
  addons.length = 0;
  storage.clear();
});

function pane() {
  const loaded: unknown[] = [];
  const webglOn: boolean[] = [];
  const term = { loadAddon: (addon: unknown) => loaded.push(addon) };
  const blink = { setWebgl: (on: boolean) => webglOn.push(on) };
  return {
    loaded,
    webglOn,
    term: term as unknown as Terminal,
    blink: blink as unknown as XtermBlink,
  };
}

describe('the WebGL renderer of a pane', () => {
  it('loads as the pane shows and lets go as it hides', () => {
    const { loaded, webglOn, term, blink } = pane();
    const webgl = xtermWebgl(term, blink, false);
    expect(loaded).toEqual([]);
    webgl.load();
    expect(loaded).toEqual([addons[0]]);
    // Showing again while it holds one loads no second context.
    webgl.load();
    expect(addons).toHaveLength(1);
    webgl.release();
    expect(addons[0]?.disposed).toBe(1);
    // A pane that hides again lets go of nothing more.
    webgl.release();
    expect(addons[0]?.disposed).toBe(1);
    // It shows again with a context of its own.
    webgl.load();
    expect(loaded).toEqual([addons[0], addons[1]]);
    expect(webglOn).toEqual([true, false, true]);
  });

  it('never loads in the history pane or with WebGL turned off', () => {
    const history = pane();
    xtermWebgl(history.term, history.blink, true).load();
    storage.set('vosh.webgl', '0');
    const off = pane();
    xtermWebgl(off.term, off.blink, false).load();
    expect(addons).toEqual([]);
    expect([...history.loaded, ...off.loaded]).toEqual([]);
  });
});
