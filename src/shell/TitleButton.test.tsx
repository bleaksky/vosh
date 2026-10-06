import { act, createElement } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';

// The session button in the title band, driven by useConnection through
// a fake Tauri event bus with two sessions, Tolliver's (1) and Orla's
// (2). The band and the window title show the selected session, so a
// session behind that connects, logs in or drops leaves them as they
// were. Each test loads fresh modules.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const titles: string[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => [] }));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    setTitle: (title: string) => {
      titles.push(title);
      return Promise.resolve();
    },
  }),
}));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const TOLLIVER = 1;
const ORLA = 2;
const HOST = 'play.theforsakenlands.com';

/** The sessions whose link runs, as the app lists them. */
const live = new Set<number>();

const connected = (session: number) => {
  live.add(session);
  fire('session://state', { session, kind: 'connecting', host: HOST, port: 1848, tls: false });
  fire('session://state', { session, kind: 'connected', host: HOST, port: 1848, tls: false });
};
const mark = (session: number, source: string) => fire('session://mark', { session, source });
const login = (session: number, name: string) =>
  fire('session://gmcp/Char-Status', { session, data: { name } });

/** The list the app sends, with `selected` the one selected. */
const select = (selected: number) =>
  fire(
    'vosh://sessions-changed',
    [TOLLIVER, ORLA].map((id) => ({
      id,
      name: null,
      character: id === TOLLIVER ? 'Tolliver' : 'Orla',
      host: HOST,
      port: 1848,
      tls: false,
      profile: id === TOLLIVER ? 'Tolliver' : 'Orla',
      connected: live.has(id),
      since: live.has(id) ? 1_000 : null,
      selected: id === selected,
    })),
  );

describe('the session button in the title band', () => {
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
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    vi.stubGlobal('localStorage', { getItem: () => null, setItem() {}, removeItem() {} });
    // React DOM checks for a DOM once, when it loads.
    ({ createRoot } = await import('react-dom/client'));
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    vi.resetModules();
    handlers.clear();
    titles.length = 0;
    live.clear();
  });

  /** Mount the button on useConnection, and read what it says. */
  async function mount(folded = false) {
    const { useConnection } = await import('../stores/session/useConnection');
    const { startStores } = await import('../stores');
    const { TitleButton } = await import('./TitleButton');
    function Band() {
      const connection = useConnection(() => undefined);
      return createElement(TitleButton, {
        connection,
        open: false,
        folded,
        onToggle: () => undefined,
      });
    }
    startStores();
    await settle();
    const host = doc.createElement('div');
    const root = createRoot(host as unknown as HTMLElement);
    await act(async () => root.render(createElement(Band)));
    const label = () =>
      findAll(host, (el) => el.nodeName === 'BUTTON')[0]?.getAttribute('aria-label');
    const count = () =>
      findAll(host, (el) => el.getAttribute('class') === 'shell-sessions-count')[0]?.textContent;
    return { root, label, count };
  }

  it('leaves the band and the window title as they were when a session behind drops', async () => {
    const band = await mount();
    await act(async () => {
      connected(TOLLIVER);
      login(TOLLIVER, 'Tolliver');
      connected(ORLA);
      login(ORLA, 'Orla');
    });
    expect(band.label()).toBe('Tolliver, connected to The Forsaken Lands');
    const title = titles.at(-1);
    expect(title).toBe('Tolliver on The Forsaken Lands');

    await act(async () => {
      live.delete(ORLA);
      fire('session://state', { session: ORLA, kind: 'disconnected', reason: 'Connection reset' });
    });
    expect(band.label()).toBe('Tolliver, connected to The Forsaken Lands');
    expect(titles.at(-1)).toBe(title);

    await act(async () => select(ORLA));
    expect(band.label()).toBe('Not connected');
    expect(titles.at(-1)).toBe('Vosh');
    await act(async () => band.root.unmount());
  });

  it('shows the selected session as it connects and logs in', async () => {
    const band = await mount();
    await act(async () => {
      connected(TOLLIVER);
      login(ORLA, 'Orla');
    });
    expect(band.label()).toBe('Connected to The Forsaken Lands');
    await act(async () => select(ORLA));
    expect(band.label()).toBe('Not connected');
    await act(async () => connected(ORLA));
    expect(band.label()).toBe('Orla, connected to The Forsaken Lands');
    await act(async () => band.root.unmount());
  });

  it('adds the port after the world when it is not the world own, and reads a name you gave', async () => {
    const band = await mount();
    const rows = (name: string | null) =>
      fire('vosh://sessions-changed', [
        {
          id: ORLA,
          name,
          character: 'Orla',
          host: HOST,
          port: 1825,
          tls: false,
          profile: 'Orla',
          connected: true,
          selected: true,
        },
      ]);
    await act(async () => {
      rows(null);
      fire('session://state', {
        session: ORLA,
        kind: 'connected',
        host: HOST,
        port: 1825,
        tls: false,
      });
      login(ORLA, 'Orla');
    });
    expect(band.label()).toBe('Orla, connected to The Forsaken Lands 1825');
    expect(titles.at(-1)).toBe('Orla on The Forsaken Lands 1825');

    await act(async () => rows('Builder'));
    expect(band.label()).toBe('Builder, connected to The Forsaken Lands 1825');
    expect(titles.at(-1)).toBe('Builder on The Forsaken Lands 1825');
    await act(async () => band.root.unmount());
  });

  // Board 05 of the Sessions Sidebar review: with the sidebar folded the
  // button totals what waits on the sessions behind.
  it('totals what waits behind while the sidebar is folded, but not the session in front or a connection in trouble', async () => {
    const band = await mount(true);
    await act(async () => {
      select(TOLLIVER);
      connected(TOLLIVER);
      login(TOLLIVER, 'Tolliver');
    });
    expect(band.count()).toBeUndefined();
    await act(async () => {
      mark(ORLA, 'preset:alert_tells');
      mark(ORLA, 'preset:alert_attacked');
      mark(ORLA, 'preset:alert_connection');
      mark(TOLLIVER, 'preset:alert_name');
    });
    expect(band.count()).toBe('2');
    expect(band.label()).toBe(
      'Tolliver, connected to The Forsaken Lands, 2 waiting on other sessions',
    );

    // Looking at Orla clears hers, and Tolliver now behind adds his.
    await act(async () => select(ORLA));
    expect(band.count()).toBeUndefined();
    await act(async () => mark(TOLLIVER, 'preset:alert_name'));
    expect(band.count()).toBe('1');
    await act(async () => band.root.unmount());
  });

  it('reads a session that connected before the page started from the list', async () => {
    live.add(ORLA);
    const band = await mount();
    await act(async () => select(ORLA));
    expect(band.label()).toBe('Orla, connected to The Forsaken Lands');
    // Tolliver connects in front, and the band follows Orla again after.
    await act(async () => {
      select(TOLLIVER);
      connected(TOLLIVER);
    });
    expect(band.label()).toBe('Connected to The Forsaken Lands');
    await act(async () => select(ORLA));
    expect(band.label()).toBe('Orla, connected to The Forsaken Lands');
    await act(async () => band.root.unmount());
  });

  it('shows no total while the sidebar shows', async () => {
    const band = await mount(false);
    await act(async () => {
      select(TOLLIVER);
      mark(ORLA, 'preset:alert_tells');
    });
    expect(band.count()).toBeUndefined();
    expect(band.label()).toBe('Not connected');
    await act(async () => band.root.unmount());
  });
});
