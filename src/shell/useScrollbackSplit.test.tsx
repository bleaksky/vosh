import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TerminalHandle } from '../terminal/terminalHandle';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import type { ScrollbackSplit } from './useScrollbackSplit';

// The scrollback split on xterm opening from the closed state. One
// PageUp opens the split and pages once the history pane has its
// scrollback, so a single press lands a page back. The wheel opens it
// with no motion.

vi.mock('../ipc/nativeSurface', () => ({ nativeSurfaceScroll: () => Promise.resolve() }));
vi.mock('../terminal/terminalRenderer', () => ({ nativeSurfaceEnabled: () => false }));
vi.mock('../terminal/readerBusy', () => ({ noteReader: () => {} }));
vi.mock('../terminal/splitDrag', () => ({
  SplitDrag: class {
    historyBottomed() {}
  },
  listenSplitDrag: () => () => {},
}));

const LIVE_ROWS = 24;
const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let unmount: (() => Promise<void>) | null = null;
let split: ScrollbackSplit;
let calls: string[];
let frames: FrameRequestCallback[];
let wheel: ((e: unknown) => void) | null;
let historyContent: { rows: number; bufferLength: number };

// The selection sync both panes take once the history is ready.
const selection = {
  onSelectionChange: () => () => {},
  hasSelection: () => false,
  clearSelection: () => {},
};

function history(): TerminalHandle {
  return {
    ...selection,
    scrollToBottom: () => calls.push('scrollToBottom'),
    scrollLines: (n: number) => calls.push(`scrollLines(${n})`),
    scrollPages: (n: number) => calls.push(`scrollPages(${n})`),
    contentSize: () => historyContent,
    refresh: () => {},
  } as unknown as TerminalHandle;
}

const live = {
  ...selection,
  getSize: () => ({ cols: 80, rows: LIVE_ROWS }),
} as unknown as TerminalHandle;
const area = {
  addEventListener: (_: string, cb: (e: unknown) => void) => {
    wheel = cb;
  },
  removeEventListener: () => {},
};

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => frames.push(cb));
  vi.stubGlobal('cancelAnimationFrame', () => {});
  ({ createRoot } = await import('react-dom/client'));
});

beforeEach(async () => {
  calls = [];
  frames = [];
  wheel = null;
  historyContent = { rows: 0, bufferLength: 0 };
  const { useScrollbackSplit } = await import('./useScrollbackSplit');
  const historyTermRef = { current: null as TerminalHandle | null };
  function Probe() {
    split = useScrollbackSplit({
      session: 1,
      termRef: { current: live },
      historyTermRef,
      terminalAreaRef: { current: area as unknown as HTMLDivElement },
      focusInput: () => {},
    });
    // The history pane mounts with the split, as MainWindow's does.
    historyTermRef.current = split.splitOpen ? history() : null;
    return null;
  }
  const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
  await act(async () => root.render(createElement(Probe)));
  unmount = () => act(async () => root.unmount());
});

afterEach(async () => {
  await unmount?.();
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const wheelUp = () =>
  act(async () => wheel?.({ deltaY: -40, preventDefault: () => {}, stopPropagation: () => {} }));

describe('useScrollbackSplit', () => {
  it('opens the split on one PageUp and pages once its history loads', async () => {
    await act(async () => split.pageSplit(-1));
    expect(split.splitOpen).toBe(true);
    expect(calls).toEqual([]);
    await act(async () => split.onHistoryLoaded());
    expect(calls).toEqual([`scrollLines(-${LIVE_ROWS})`, 'scrollPages(-1)']);
    // The frame poll then positions the pane from its bottom and takes
    // the page for good, so a later load pages no further.
    calls = [];
    historyContent = { rows: 10, bufferLength: 200 };
    await act(async () => frames.shift()?.(0));
    expect(calls).toEqual(['scrollToBottom', `scrollLines(-${LIVE_ROWS})`, 'scrollPages(-1)']);
    calls = [];
    await act(async () => split.onHistoryLoaded());
    expect(calls).toEqual([`scrollLines(-${LIVE_ROWS})`]);
  });

  it('pages from the frame polled reveal when the load callback never fires', async () => {
    await act(async () => split.pageSplit(-1));
    historyContent = { rows: 10, bufferLength: 200 };
    await act(async () => frames.shift()?.(0));
    expect(calls).toEqual(['scrollToBottom', `scrollLines(-${LIVE_ROWS})`, 'scrollPages(-1)']);
  });

  it('opens the split on the wheel without paging', async () => {
    await wheelUp();
    expect(split.splitOpen).toBe(true);
    await act(async () => split.onHistoryLoaded());
    expect(calls).toEqual([`scrollLines(-${LIVE_ROWS})`]);
  });

  it('opens the split on Mod+\\ without paging', async () => {
    await act(async () => split.toggleSplit());
    await act(async () => split.onHistoryLoaded());
    expect(calls).toEqual([`scrollLines(-${LIVE_ROWS})`]);
  });
});
