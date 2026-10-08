import { act, createElement } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import source from './SnoopTerminal.tsx?raw';

// The terminal of one snooped player. React DOM mounts it on
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
    focused: boolean;
    keys: ((event: KeyboardEvent) => boolean) | null;
    selected: string;
  }[],
  sent: [] as string[],
  keydowns: new Set<(event: KeyboardEvent) => void>(),
  copied: [] as string[],
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
    focused = false;
    keys: ((event: KeyboardEvent) => boolean) | null = null;
    selected = '';
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
    attachCustomKeyEventHandler(keys: (event: KeyboardEvent) => boolean) {
      this.keys = keys;
    }
    focus() {
      this.focused = true;
    }
    scrollToBottom() {}
    getSelection() {
      return this.selected;
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
vi.mock('@xterm/addon-search', () => ({
  SearchAddon: class {
    onDidChangeResults() {
      return { dispose() {} };
    }
  },
}));
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
    addEventListener(type: string, cb: (event: KeyboardEvent) => void) {
      if (type === 'keydown') fake.keydowns.add(cb);
    },
    removeEventListener(type: string, cb: (event: KeyboardEvent) => void) {
      if (type === 'keydown') fake.keydowns.delete(cb);
    },
    dispatchEvent(event: Event) {
      fake.sent.push(event.type);
    },
  });
  vi.stubGlobal('navigator', {
    userAgent: 'node',
    platform: '',
    clipboard: {
      writeText: async (text: string) => {
        fake.copied.push(text);
      },
    },
  });
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
  // The copy key looks for the caret in the snoop.
  const el = FakeElement.prototype as unknown as Record<string, unknown>;
  el.contains = function (this: FakeNode, other: FakeNode | null): boolean {
    for (let n = other; n; n = n.parentNode) if (n === this) return true;
    return false;
  };
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  fake.terms.length = 0;
  fake.texts.clear();
  fake.listeners.clear();
  fake.sent.length = 0;
  fake.copied.length = 0;
  doc.activeElement = null;
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
  return {
    term,
    view: host.firstChild as FakeElement,
    unmount: () => act(() => root.unmount()),
    text: () => term.written.join(''),
  };
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

  it('hands Esc and a key that types back to the command line (SN7)', async () => {
    const { term, unmount } = await mount('Tolliver');
    let prevented = 0;
    const key = (key: string, over: Partial<KeyboardEvent> = {}) =>
      ({
        type: 'keydown',
        key,
        metaKey: false,
        ctrlKey: false,
        isComposing: false,
        preventDefault: () => (prevented += 1),
        ...over,
      }) as unknown as KeyboardEvent;
    const keys = term.keys;
    if (!keys) throw new Error('no key handler');
    // A key that types goes back and is left to type there, so xterm
    // must leave it alone and nothing takes it.
    expect(keys(key('l'))).toBe(false);
    expect(prevented).toBe(0);
    // Esc goes back and is taken.
    expect(keys(key('Escape'))).toBe(false);
    expect(prevented).toBe(1);
    expect(fake.sent).toEqual(['vosh:focus-input', 'vosh:focus-input']);
    // Cmd J, the arrows, and the key's other events stay with the snoop.
    expect(keys(key('j', { metaKey: true }))).toBe(true);
    expect(keys(key('ArrowUp'))).toBe(true);
    expect(keys(key('l', { type: 'keyup' }))).toBe(true);
    expect(fake.sent).toHaveLength(2);
    unmount();
  });

  it('copies what you selected here only while the caret is here', async () => {
    const before = fake.keydowns.size;
    const { term, view, unmount } = await mount('Maren');
    term.selected = 'The day has begun.';
    let prevented = 0;
    const press = () => {
      const event = {
        type: 'keydown',
        key: 'c',
        ctrlKey: true,
        metaKey: false,
        altKey: false,
        preventDefault: () => (prevented += 1),
      } as unknown as KeyboardEvent;
      for (const cb of fake.keydowns) cb(event);
    };
    // xterm keeps the selection once the caret leaves. Ctrl C in the
    // command line copies what you selected there.
    doc.createElement('input').focus();
    press();
    expect(prevented).toBe(0);
    expect(fake.copied).toEqual([]);
    expect(fake.sent).toEqual([]);
    // With the caret here it copies the snoop and hands the caret back.
    view.focus();
    press();
    await Promise.resolve();
    expect(prevented).toBe(1);
    expect(fake.copied).toEqual(['The day has begun.']);
    expect(fake.sent).toEqual(['vosh:focus-input']);
    unmount();
    expect(fake.keydowns.size).toBe(before);
  });

  it('leaves out every part of your automation that paints your terminal', () => {
    for (const part of ['RegionWriter', 'liftBands', 'setHighlightGround', 'PaneSizer']) {
      expect(source).not.toContain(part);
    }
  });
});
