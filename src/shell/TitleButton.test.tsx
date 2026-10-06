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

const connected = (session: number) => {
  fire('session://state', { session, kind: 'connecting', host: HOST, port: 1848, tls: false });
  fire('session://state', { session, kind: 'connected', host: HOST, port: 1848, tls: false });
};
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
      connected: true,
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
  });

  /** Mount the button on useConnection, and read what it says. */
  async function mount() {
    const { useConnection } = await import('../stores/session/useConnection');
    const { startStores } = await import('../stores');
    const { TitleButton } = await import('./TitleButton');
    function Band() {
      const connection = useConnection(() => undefined);
      return createElement(TitleButton, { connection, open: false, onToggle: () => undefined });
    }
    startStores();
    await settle();
    const host = doc.createElement('div');
    const root = createRoot(host as unknown as HTMLElement);
    await act(async () => root.render(createElement(Band)));
    const label = () =>
      findAll(host, (el) => el.nodeName === 'BUTTON')[0]?.getAttribute('aria-label');
    return { root, label };
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

    await act(async () =>
      fire('session://state', {
        session: ORLA,
        kind: 'disconnected',
        reason: 'server closed connection',
      }),
    );
    expect(band.label()).toBe('Tolliver, connected to The Forsaken Lands');
    expect(titles.at(-1)).toBe(title);

    await act(async () => select(ORLA));
    expect(band.label()).toBe('Not connected. server closed connection');
    expect(titles.at(-1)).toBe('Vosh');
    await act(async () => band.root.unmount());
  });

  it('keeps the error of a drop through every try of its redial, and goes idle on your Disconnect', async () => {
    const band = await mount();
    const drop = (reason: string | null) =>
      fire('session://state', { session: TOLLIVER, kind: 'disconnected', reason });
    const redial = (payload: object) =>
      fire('session://reconnect', { session: TOLLIVER, ...payload });
    await act(async () => {
      connected(TOLLIVER);
      login(TOLLIVER, 'Tolliver');
      drop('server closed connection');
      redial({ kind: 'waiting', try: 1, tries: 8, seconds: 5 });
    });
    expect(band.label()).toBe('Not connected. server closed connection');
    expect(titles.at(-1)).toBe('Vosh');

    // A try dials, ends its link with no reason, then says why it failed.
    await act(async () => {
      redial({ kind: 'dialing', try: 1, tries: 8 });
      fire('session://state', {
        session: TOLLIVER,
        kind: 'connecting',
        host: HOST,
        port: 1848,
        tls: false,
      });
    });
    expect(band.label()).toBe('Connecting to The Forsaken Lands');
    await act(async () => {
      drop(null);
      redial({ kind: 'failed', try: 1, tries: 8, reason: 'the game refused the connection' });
      redial({ kind: 'waiting', try: 2, tries: 8, seconds: 10 });
    });
    expect(band.label()).toBe('Not connected. the game refused the connection');

    await act(async () => {
      redial({ kind: 'cancelled' });
      connected(TOLLIVER);
      drop(null);
    });
    expect(band.label()).toBe('Not connected');
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
});
