import { act, createElement } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';

// Each session's terminal hears every session's output event and writes
// only its own session's. React DOM mounts two terminals on a stand in
// DOM (src/test/fakeDom.ts). xterm and the parts that draw stand in too,
// so what reaches each terminal's region writer is what it would write.

type Handler = (event: { payload: unknown }) => void;
const bus = vi.hoisted(() => ({
  handlers: new Map<string, Set<(event: { payload: unknown }) => void>>(),
  invoked: [] as [string, unknown][],
  /** What each terminal's region writer took, by the xterm it writes. */
  written: new Map<object, string[]>(),
  /** What each region writer wrote of the pane's own, by the xterm. */
  local: new Map<object, string[]>(),
  /** Each mark a region writer was given, in order. */
  marks: [] as string[],
  /** Scrollback loads held until a test answers them, while it holds. */
  hold: false,
  /** How wide each new xterm is. */
  cols: 80,
  held: [] as ((bytes: number[]) => void)[],
  /** Each call that reached a stand in xterm's renderer: a refresh or a
   *  scroll. */
  drawn: [] as string[],
  /** Every stand in xterm, its hidden input and its key handler. */
  terms: [] as {
    textarea: { tabIndex: number };
    keys: ((event: KeyboardEvent) => boolean) | null;
  }[],
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = bus.handlers.get(event);
    if (!set) bus.handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args?: unknown) => {
    bus.invoked.push([cmd, args]);
    if (cmd === 'scrollback_load' && bus.hold) {
      return new Promise((answer) =>
        bus.held.push((bytes) => answer({ bytes, seeded_native: false })),
      );
    }
    if (cmd === 'scrollback_load') return { bytes: [], seeded_native: false };
    return null;
  },
}));

vi.mock('@xterm/xterm', () => {
  const none = () => ({ dispose() {} });
  class Terminal {
    cols = bus.cols;
    rows = 24;
    options: Record<string, unknown>;
    unicode = { activeVersion: '' };
    buffer = { active: { cursorY: 23, baseY: 0, viewportY: 0, type: 'normal' } };
    textarea = { tabIndex: 0 };
    keys: ((event: KeyboardEvent) => boolean) | null = null;
    constructor(options: Record<string, unknown>) {
      this.options = { ...options };
      bus.terms.push(this);
    }
    loadAddon() {}
    open() {}
    attachCustomKeyEventHandler(keys: (event: KeyboardEvent) => boolean) {
      this.keys = keys;
    }
    scrollToLine() {}
    refresh() {
      bus.drawn.push('refresh');
    }
    scrollLines(n: number) {
      bus.drawn.push(`scrollLines(${n})`);
    }
    scrollToBottom() {
      bus.drawn.push('scrollToBottom');
    }
    resized: ((size: { cols: number; rows: number }) => void)[] = [];
    onResize = (cb: (size: { cols: number; rows: number }) => void) => {
      this.resized.push(cb);
      return { dispose() {} };
    };
    /** Take a new size, as a fit does. */
    resize(cols: number, rows: number) {
      this.cols = cols;
      this.rows = rows;
      for (const cb of this.resized) cb({ cols, rows });
    }
    onScroll = none;
    onSelectionChange = none;
    getSelectionPosition() {
      return undefined;
    }
    hasSelection() {
      return false;
    }
    getSelection() {
      return '';
    }
    dispose() {}
  }
  return { Terminal };
});

vi.mock('@xterm/addon-fit', () => ({ FitAddon: class {} }));
vi.mock('@xterm/addon-web-links', () => ({ WebLinksAddon: class {} }));
vi.mock('@xterm/addon-unicode11', () => ({ Unicode11Addon: class {} }));
vi.mock('@xterm/addon-search', () => ({
  SearchAddon: class {
    onDidChangeResults() {
      return { dispose() {} };
    }
    dispose() {}
  },
}));
vi.mock('@xterm/xterm/css/xterm.css', () => ({}));

vi.mock('./terminalRegion', () => ({
  RegionWriter: class {
    private readonly took: string[] = [];
    private readonly own: string[] = [];
    constructor(term: object) {
      bus.written.set(term, this.took);
      bus.local.set(term, this.own);
    }
    output(out: { text: string }) {
      this.took.push(out.text);
    }
    local(text: string) {
      this.own.push(text);
    }
    setEchoMark(mark: string) {
      bus.marks.push(mark);
    }
    pad() {}
    onErase() {}
    pendingRows() {
      return 0;
    }
    region() {
      return null;
    }
    whenParsed(then: () => void) {
      then();
    }
    resize() {}
    dispose() {}
  },
}));
vi.mock('./paneSizer', () => ({
  PaneSizer: class {
    nativeSpare = 0;
    safeFit = () => {};
    start() {}
    stop() {}
    show() {}
    placeGrid() {}
    reportCellSize() {}
    relayout() {}
    fitKept() {}
  },
}));
vi.mock('./xterm/liftBands', () => ({
  LiftTracker: class {
    dropFrom() {}
    dispose() {}
  },
  BandLayer: class {},
  markLifted: () => {},
}));
vi.mock('./xterm/xtermBlink', () => ({
  XtermBlink: class {
    setOn() {}
    setWebgl() {}
    dispose() {}
  },
}));
vi.mock('./xterm/xtermWebgl', () => ({
  xtermWebgl: () => ({ load() {}, release() {}, dispose() {} }),
}));
vi.mock('./xterm/xtermMirror', () => ({
  XtermMirror: class {
    mirrors() {
      return true;
    }
    write(step: () => void) {
      step();
    }
    refill(fill: (done: () => void) => void) {
      fill(() => {});
    }
    check() {}
  },
  underlayShows: () => false,
}));
vi.mock('./native/underlayPointer', () => ({ forwardUnderlayPointer: () => () => {} }));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;

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
  const storage = { getItem: () => null, setItem() {}, removeItem() {} };
  vi.stubGlobal('localStorage', storage);
  vi.stubGlobal('sessionStorage', storage);
  vi.stubGlobal(
    'MutationObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal('requestAnimationFrame', () => 0);
  vi.stubGlobal('cancelAnimationFrame', () => {});
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  // The pane finds the terminal area it lifts your prompts in, which
  // this stand in leaves out.
  (FakeElement.prototype as unknown as { closest: () => null }).closest = () => null;
  // React DOM checks for a DOM once, when it loads.
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

/** The base64 of a line of game text. */
const b64 = (text: string) => btoa(text);

function output(session: number, text: string): void {
  for (const cb of bus.handlers.get('session://output') ?? []) {
    cb({ payload: { session, b64: b64(text) } });
  }
}

describe('a terminal for each session', () => {
  it('writes only the output of its own session', async () => {
    const { Terminal } = await import('./Terminal');
    const handles = new Map<number, object>();
    const pane = (session: number, shown: boolean) =>
      createElement(Terminal, {
        key: session,
        session,
        shown,
        fontFamily: 'monospace',
        fontSize: 13,
        lineHeight: 1.2,
        themeTerminalColors: false,
        onReady: (handle) => handles.set(session, handle),
      });
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () => root.render([pane(1, true), pane(2, false)]));
    expect(handles.size).toBe(2);
    expect(bus.written.size).toBe(2);
    // Each pane loaded its own session's scrollback.
    const loads = bus.invoked.filter(([cmd]) => cmd === 'scrollback_load').map(([, a]) => a);
    expect(loads).toEqual([
      { feedNative: false, session: 1 },
      { feedNative: false, session: 2 },
    ]);

    // Lines of fixtures/room-colors/looks.json.
    output(1, 'The day has begun.\r\n');
    output(2, '[Exits: south]\r\n');
    output(2, '<1020hp 800m 930mv> ');

    const [tolliver, orla] = [...bus.written.values()];
    expect(tolliver).toEqual(['The day has begun.\r\n']);
    expect(orla).toEqual(['[Exits: south]\r\n', '<1020hp 800m 930mv> ']);
    await act(async () => root.unmount());
  });
});

// The output is one Tab stop on its slot (Q22). xterm's hidden input
// leaves the Tab order and lets every Tab pass.
describe('the Tab order', () => {
  it('takes the hidden input out and never eats a Tab', async () => {
    const { Terminal } = await import('./Terminal');
    const before = bus.terms.length;
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () =>
      root.render(
        createElement(Terminal, {
          session: 1,
          shown: true,
          fontFamily: 'monospace',
          fontSize: 13,
          lineHeight: 1.2,
          themeTerminalColors: false,
        }),
      ),
    );
    const term = bus.terms[before];
    expect(term.textarea.tabIndex).toBe(-1);
    const key = (key: string, shiftKey = false) =>
      ({ type: 'keydown', key, shiftKey }) as unknown as KeyboardEvent;
    expect(term.keys?.(key('Tab'))).toBe(false);
    expect(term.keys?.(key('Tab', true))).toBe(false);
    expect(term.keys?.(key('a'))).toBe(true);
    await act(async () => root.unmount());
  });
});

/** Tell every window the theme is now `id`. */
function themeChanged(id: string): void {
  for (const cb of bus.handlers.get('vosh://theme-changed') ?? []) cb({ payload: id });
}

/** The yellow wash of wash_wraps_whole_line in the trigger engine. */
const SANCTUARY =
  '\x1b[33;48;2;51;51;0mYour \x1b[33msanctuary\x1b[0m\x1b[33;48;2;51;51;0m flickers and fades.\x1b[0m\r\n';

/** The stand in xterm, which takes a new size as a fit gives it. */
interface Resizable {
  resize(cols: number, rows: number): void;
}

/** Wait past the pause a pane takes for its size to settle. */
const settled = () => new Promise((done) => setTimeout(done, 200));

/** How many times a pane loaded the scrollback. */
const loads = () => bus.invoked.filter(([cmd]) => cmd === 'scrollback_load').length;

describe('a theme change on a pane xterm draws', () => {
  async function mount() {
    const { Terminal } = await import('./Terminal');
    bus.written.clear();
    bus.local.clear();
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () =>
      root.render(
        createElement(Terminal, {
          session: 1,
          fontFamily: 'monospace',
          fontSize: 13,
          lineHeight: 1.2,
          themeTerminalColors: true,
        }),
      ),
    );
    return root;
  }

  it('fills anew from the scrollback, with no banner, once a wash painted', async () => {
    const root = await mount();
    themeChanged('obsidian-ember');
    output(1, SANCTUARY);
    const before = loads();
    await act(async () => themeChanged('vellum'));
    expect(loads()).toBe(before + 1);
    const local = [...bus.local.values()].flat();
    expect(local.some((text) => text.includes('[scrollback restored]'))).toBe(false);
    await act(async () => root.unmount());
  });

  it('writes the history once when a second change overtakes a fill', async () => {
    const root = await mount();
    themeChanged('obsidian-ember');
    const history = 'The day has begun.\r\n';
    output(1, SANCTUARY);
    bus.hold = true;
    await act(async () => themeChanged('vellum'));
    // The stand in mirror writes at once where the real one waits for
    // the fill, so the pane paints a wash again and the next change
    // fills anew while the first fill still loads.
    output(1, SANCTUARY);
    await act(async () => themeChanged('obsidian-ember'));
    const [first, second] = bus.held.splice(0);
    expect(second).toBeDefined();
    const bytes = [...new TextEncoder().encode(history)];
    await act(async () => second(bytes));
    await act(async () => first(bytes));
    bus.hold = false;
    const [local] = [...bus.local.values()];
    // The fill resets in the stream, then writes the history once.
    expect(local[0]).toBe('\x1bc');
    expect(local.join('').split('The day has begun.').length - 1).toBe(1);
    await act(async () => root.unmount());
  });

  it('word wraps the history it fills anew as live output wraps', async () => {
    bus.cols = 16;
    const root = await mount();
    bus.cols = 80;
    themeChanged('obsidian-ember');
    output(1, SANCTUARY);
    const [live] = [...bus.written.values()];
    bus.hold = true;
    await act(async () => themeChanged('vellum'));
    const [load] = bus.held.splice(0);
    await act(async () => load([...new TextEncoder().encode(SANCTUARY)]));
    bus.hold = false;
    const [local] = [...bus.local.values()];
    // eslint-disable-next-line no-control-regex
    const plain = (text: string) => text.replace(/\x1b\[[0-9;:]*[A-Za-z]/g, '');
    expect(plain(live.join(''))).toBe('Your sanctuary\r\nflickers and\r\nfades.\r\n');
    expect(plain(local[1])).toBe(plain(live.join('')));
    await act(async () => root.unmount());
  });

  it('fills anew once it settles wider than a wash it painted', async () => {
    const root = await mount();
    output(1, SANCTUARY);
    const [term] = [...bus.local.keys()] as Resizable[];
    const before = loads();
    term.resize(70, 24);
    await settled();
    expect(loads()).toBe(before);
    term.resize(90, 24);
    term.resize(100, 24);
    await settled();
    expect(loads()).toBe(before + 1);
    await act(async () => root.unmount());
  });

  it('keeps the screen as it widens when no wash painted', async () => {
    const root = await mount();
    output(1, 'The day has begun.\r\n');
    const [term] = [...bus.local.keys()] as Resizable[];
    const before = loads();
    term.resize(100, 24);
    await settled();
    expect(loads()).toBe(before);
    await act(async () => root.unmount());
  });

  it('writes nothing again when no wash painted', async () => {
    const root = await mount();
    themeChanged('obsidian-ember');
    output(1, 'The day has begun.\r\n');
    const before = loads();
    await act(async () => themeChanged('vellum'));
    expect(loads()).toBe(before);
    await act(async () => root.unmount());
  });
});

describe('Scrollback size', () => {
  it('reaches the xterm it has without building another', async () => {
    const { Terminal } = await import('./Terminal');
    bus.written.clear();
    const ready: object[] = [];
    const pane = (scrollback: number) =>
      createElement(Terminal, {
        session: 1,
        fontFamily: 'monospace',
        fontSize: 13,
        lineHeight: 1.2,
        themeTerminalColors: false,
        scrollback,
        onReady: (handle) => ready.push(handle),
      });
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () => root.render(pane(10_000)));
    const [term] = [...bus.written.keys()] as { options: Record<string, unknown> }[];
    expect(term.options.scrollback).toBe(10_000);
    const before = loads();

    await act(async () => root.render(pane(1_000)));
    expect(term.options.scrollback).toBe(1_000);
    await act(async () => root.render(pane(25_000)));
    expect(term.options.scrollback).toBe(25_000);
    // The same xterm, ready once, and its history stays as it is.
    expect(bus.written.size).toBe(1);
    expect(ready).toHaveLength(1);
    expect(loads()).toBe(before);
    await act(async () => root.unmount());
  });
});

describe('the mark your echo starts with', () => {
  it('reaches the writer, a change to it, and a writer built to fill anew', async () => {
    const { Terminal } = await import('./Terminal');
    const chevron = '\x1b[90m\u203a \x1b[0m';
    const gt = '\x1b[90m> \x1b[0m';
    bus.marks.length = 0;
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () =>
      root.render(
        createElement(Terminal, {
          session: 1,
          fontFamily: 'monospace',
          fontSize: 13,
          lineHeight: 1.2,
          themeTerminalColors: true,
        }),
      ),
    );
    expect(bus.marks).toEqual([chevron]);
    for (const cb of bus.handlers.get('vosh://input-echo-mark-changed') ?? []) {
      cb({ payload: { mark: 'gt', text: '', color: null, dim: false } });
    }
    expect(bus.marks.at(-1)).toBe(gt);
    // A theme change after a wash fills anew with a new writer.
    themeChanged('obsidian-ember');
    output(1, SANCTUARY);
    const given = bus.marks.length;
    await act(async () => themeChanged('vellum'));
    expect(bus.marks.slice(given)).toEqual([gt]);
    await act(async () => root.unmount());
  });
});

// The split's history pane unmounts as a wheel closes the split, and xterm
// lets its renderer go. A handle the host still holds, or a scrollback
// load that lands after, must not reach it: a refresh or a scroll of a
// disposed xterm queues a frame that reads the missing renderer and
// throws (`_renderer.value.dimensions`).
describe('a pane that went', () => {
  it('lets its host go of the handle and reaches xterm no more', async () => {
    const { Terminal } = await import('./Terminal');
    bus.hold = true;
    bus.drawn = [];
    let ready: import('./terminalHandle').TerminalHandle | null = null;
    const gone: unknown[] = [];
    const loaded = vi.fn();
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () =>
      root.render(
        createElement(Terminal, {
          session: 1,
          quiet: true,
          fontFamily: 'monospace',
          fontSize: 13,
          lineHeight: 1.2,
          themeTerminalColors: false,
          onReady: (handle) => {
            ready = handle;
          },
          onGone: (handle) => gone.push(handle),
          onScrollbackLoaded: loaded,
        }),
      ),
    );
    const handle = ready as unknown as import('./terminalHandle').TerminalHandle;
    handle.refresh();
    expect(bus.drawn).toEqual(['refresh']);
    bus.drawn = [];

    await act(async () => root.unmount());
    expect(gone).toEqual([handle]);
    handle.refresh();
    handle.scrollLines(3);
    handle.scrollPages(1);
    handle.scrollToBottom();
    expect(handle.isAtBottom()).toBe(true);
    expect(bus.drawn).toEqual([]);

    // Its scrollback lands after it went, and the host hears nothing.
    const [load] = bus.held.splice(0);
    bus.hold = false;
    await act(async () => load?.([...new TextEncoder().encode('The day has begun.\r\n')]));
    expect(loaded).not.toHaveBeenCalled();
    expect(bus.drawn).toEqual([]);
  });
});
