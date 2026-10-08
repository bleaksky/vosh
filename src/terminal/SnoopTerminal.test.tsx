import { act, createElement } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import source from './SnoopTerminal.tsx?raw';

// The terminal of one snooped player (Snoop SN3). React DOM mounts it on
// a stand in DOM (src/test/fakeDom.ts) with xterm standing in, so what
// reaches xterm is what it would draw. The snoop store stands in too,
// with the text each tab holds and the pieces it hands on.

const fake = vi.hoisted(() => ({
  terms: [] as {
    options: Record<string, unknown>;
    written: string[];
    resets: number;
    cols: number;
    disposed: boolean;
  }[],
  texts: new Map<string, string>(),
  listeners: new Set<(session: number, name: string, text: string, whole: boolean) => void>(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: async () => () => {},
  emit: async () => undefined,
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => null }));

vi.mock('@xterm/xterm', () => {
  const none = () => ({ dispose() {} });
  class Terminal {
    options: Record<string, unknown>;
    unicode = { activeVersion: '' };
    written: string[] = [];
    resets = 0;
    cols = 30;
    disposed = false;
    constructor(options: Record<string, unknown>) {
      this.options = { ...options };
      fake.terms.push(this);
    }
    loadAddon() {}
    open() {}
    write(text: string) {
      this.written.push(text);
    }
    reset() {
      this.resets += 1;
      this.written = [];
    }
    onResize = none;
    getSelection() {
      return '';
    }
    dispose() {
      this.disposed = true;
    }
  }
  return { Terminal };
});
vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class {
    fit() {}
  },
}));
vi.mock('@xterm/addon-unicode11', () => ({ Unicode11Addon: class {} }));
vi.mock('@xterm/xterm/css/xterm.css', () => ({}));

vi.mock('../stores/session/snoopStore', () => ({
  SNOOP_LINES: 5_000,
  snoopText: (session: number, name: string) => fake.texts.get(`${session}:${name}`) ?? '',
  subscribeSnoopOutput: (
    cb: (session: number, name: string, text: string, whole: boolean) => void,
  ) => {
    fake.listeners.add(cb);
    return () => fake.listeners.delete(cb);
  },
}));

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
    dispatchEvent() {},
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  const storage = { getItem: () => null, setItem() {}, removeItem() {} };
  vi.stubGlobal('localStorage', storage);
  vi.stubGlobal('sessionStorage', storage);
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  fake.terms.length = 0;
  fake.texts.clear();
  fake.listeners.clear();
});

const STAFF = 1;

function hand(name: string, text: string, whole = false, session = STAFF): void {
  for (const cb of fake.listeners) cb(session, name, text, whole);
}

async function mount(name: string) {
  const { SnoopTerminal } = await import('./SnoopTerminal');
  const host = doc.createElement('div');
  const root = createRoot(host as unknown as HTMLElement);
  act(() =>
    root.render(
      createElement(SnoopTerminal, {
        session: STAFF,
        name,
        shown: true,
        fontFamily: '"JetBrainsMono Bundled", Menlo, monospace',
        fontSize: 14,
        lineHeight: 1.2,
        themeTerminalColors: false,
      }),
    ),
  );
  const term = fake.terms[fake.terms.length - 1];
  return { term, unmount: () => act(() => root.unmount()), text: () => term.written.join('') };
}

describe('a snoop terminal', () => {
  it('draws in your face, size and line height, keeps 5,000 lines and takes no typing', async () => {
    const { term, unmount } = await mount('Tolliver');
    expect(term.options).toMatchObject({
      fontFamily: '"JetBrainsMono Bundled", Menlo, monospace',
      fontSize: 14,
      lineHeight: 1.2,
      scrollback: 5_000,
      disableStdin: true,
    });
    expect(term.options.theme).toMatchObject({ background: expect.any(String) });
    unmount();
    expect(term.disposed).toBe(true);
  });

  it('starts from the text its tab holds, then writes each new piece of its own', async () => {
    fake.texts.set(`${STAFF}:Tolliver`, '[Exits: east west]\n\r');
    const { text, unmount } = await mount('Tolliver');
    hand('Tolliver', '<612hp 480m 702mv> ');
    hand('Maren', 'The day has begun.\n\r');
    hand('Tolliver', 'Elsewhere', false, 2);
    expect(text()).toBe('\x1b[?25l[Exits: east west]\n\r<612hp 480m 702mv> ');
    unmount();
    expect(fake.listeners.size).toBe(0);
  });

  it('writes a snapshot afresh in place of what it showed', async () => {
    const { term, text } = await mount('Maren');
    hand('Maren', 'The day has begun.\n\r');
    hand('Maren', '[Exits: north south]\n\r', true);
    expect(term.resets).toBe(1);
    expect(text()).toBe('\x1b[?25l[Exits: north south]\n\r');
  });

  it('word wraps at its width as your terminal does, ANSI kept', async () => {
    const { text } = await mount('Orla');
    hand(
      'Orla',
      '\x1b[0;33mA rocky mountain path\x1b[0;0m and the cold air from the mountains\n\r',
    );
    expect(text()).toBe(
      '\x1b[?25l\x1b[0;33mA rocky mountain path\x1b[0;0m and the\r\ncold air from the mountains\n\r',
    );
  });

  it('leaves out every part of your automation that paints your terminal', () => {
    for (const part of ['RegionWriter', 'liftBands', 'setHighlightGround', 'PaneSizer']) {
      expect(source).not.toContain(part);
    }
  });
});
