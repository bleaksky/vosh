import type { Terminal } from '@xterm/xterm';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { XtermBlink } from './xtermBlink';
import { xtermWebgl } from './xtermWebgl';

// Only the pane that shows holds a WebGL context. The
// addon stands in here, and so do the canvas the probe asks
// for a context and the storage that can turn WebGL off.

const addons = vi.hoisted(() => [] as { disposed: number; lose: () => void }[]);

vi.mock('@xterm/addon-webgl', () => ({
  WebglAddon: class {
    disposed = 0;
    lose = () => {};
    constructor() {
      addons.push(this);
    }
    onContextLoss(cb: () => void) {
      this.lose = cb;
    }
    dispose() {
      this.disposed += 1;
    }
  },
}));

const storage = new Map<string, string>();
let frames: FrameRequestCallback[] = [];

beforeAll(() => {
  vi.stubGlobal('document', {
    createElement: () => ({ getContext: () => ({ getExtension: () => null }) }),
  });
  vi.stubGlobal('localStorage', { getItem: (key: string) => storage.get(key) ?? null });
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => frames.push(cb));
  vi.spyOn(console, 'log').mockImplementation(() => {});
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});

afterAll(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

beforeEach(() => {
  addons.length = 0;
  storage.clear();
  frames = [];
});

function pane() {
  const loaded: unknown[] = [];
  const webglOn: boolean[] = [];
  const refreshed: number[] = [];
  const term = {
    rows: 24,
    loadAddon: (addon: unknown) => loaded.push(addon),
    refresh: (start: number) => refreshed.push(start),
  };
  const blink = { setWebgl: (on: boolean) => webglOn.push(on) };
  return {
    loaded,
    webglOn,
    refreshed,
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

  it('repaints after a lost context only while the pane stays', () => {
    const { refreshed, term, blink } = pane();
    const webgl = xtermWebgl(term, blink, false);
    webgl.load();
    addons[0]?.lose();
    for (const frame of frames.splice(0)) frame(0);
    expect(refreshed).toEqual([0]);

    // The pane goes between the lost context and the repaint frame. xterm
    // has no renderer by then, and a refresh would queue a frame that
    // reads it and throws.
    webgl.load();
    addons[1]?.lose();
    webgl.dispose();
    for (const frame of frames.splice(0)) frame(0);
    expect(refreshed).toEqual([0]);
    // A pane that went takes no context again.
    webgl.load();
    expect(addons).toHaveLength(2);
  });
});
