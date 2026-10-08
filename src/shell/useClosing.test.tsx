import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import type { Closing } from './useClosing';

// Closing a session, the main window and the app. The red light and the
// close button reach the window's close request, which asks as Close
// window does. Close session on the selected row brings the next row
// down to the front first, or the one before it when it is last, and
// with one session it closes the window. closeQuestions.test.ts holds
// the words.

/** Every step the hook took, in order, the window's own among them. */
const steps = vi.hoisted(() => [] as unknown[]);

const sessions = vi.hoisted(() => ({ rows: [] as SessionRow[], selected: 1 }));

/** The main window, with the close request the hook holds. */
const win = vi.hoisted(() => ({
  request: null as ((event: { preventDefault: () => void }) => void) | null,
  letGo: (): void => {
    steps.push('let go');
  },
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onCloseRequested: (handler: (event: { preventDefault: () => void }) => void) => {
      win.request = handler;
      return Promise.resolve(win.letGo);
    },
    close: () => {
      steps.push('close window');
      return Promise.resolve();
    },
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: unknown) => {
    steps.push([cmd, args]);
    return Promise.resolve();
  },
}));

vi.mock('../stores/session/sessionsStore', () => ({
  getSessions: () => sessions.rows,
  getSelected: () => sessions.selected,
  select: (id: number) => {
    steps.push(['select', id]);
    sessions.selected = id;
    return Promise.resolve();
  },
}));

vi.mock('../stores/session/connectionStore', () => ({ sessionLive: () => false }));

const PLAY = 'play.theforsakenlands.com';

function row(id: number, character: string, connected: boolean): SessionRow {
  return {
    id,
    name: null,
    character,
    host: PLAY,
    port: id === 2 ? 1825 : 1848,
    tls: false,
    profile: 'default',
    connected,
    since: null,
    selected: id === sessions.selected,
  };
}

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
const cleanups: (() => Promise<void>)[] = [];

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
  ({ createRoot } = await import('react-dom/client'));
});

beforeEach(() => {
  steps.length = 0;
  sessions.selected = 1;
  win.request = null;
});

afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) await cleanup();
});

afterAll(() => {
  vi.unstubAllGlobals();
});

/** Mount the hook, and hand back what it gives the window now. */
async function mount(): Promise<() => Closing> {
  const { useClosing } = await import('./useClosing');
  let latest: Closing | null = null;
  function Probe() {
    latest = useClosing();
    return null;
  }
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => root.render(createElement(Probe)));
  cleanups.push(async () => {
    await act(async () => root.unmount());
  });
  return () => {
    if (!latest) throw new Error('the hook has not run');
    return latest;
  };
}

const run = (fn: () => void) => act(async () => fn());

describe('the red light and the close button', () => {
  it('ask while a session is connected, and close the window only on the danger button', async () => {
    sessions.rows = [row(1, 'Tolliver', true), row(2, 'Orla', false)];
    const closing = await mount();
    const prevent = vi.fn();
    await run(() => win.request?.({ preventDefault: prevent }));
    expect(prevent).toHaveBeenCalled();
    expect(closing().asking?.title).toBeTruthy();
    expect(steps).toEqual([]);
    await run(() => closing().asking?.onConfirm());
    // The window lets go of its close request first, or the close would
    // come back to it.
    expect(steps).toEqual(['let go', 'close window']);
    expect(closing().asking).toBeNull();
  });

  it('close at once while no session is connected', async () => {
    sessions.rows = [row(1, 'Tolliver', false)];
    const closing = await mount();
    await run(() => win.request?.({ preventDefault() {} }));
    expect(closing().asking).toBeNull();
    expect(steps).toEqual(['let go', 'close window']);
  });

  it('keep the window open on Cancel', async () => {
    sessions.rows = [row(1, 'Tolliver', true)];
    const closing = await mount();
    await run(() => win.request?.({ preventDefault() {} }));
    await run(() => closing().cancel());
    expect(closing().asking).toBeNull();
    expect(steps).toEqual([]);
  });
});

describe('Close session', () => {
  it('brings the next row down to the front before it closes the selected session', async () => {
    sessions.selected = 2;
    sessions.rows = [row(1, 'Tolliver', false), row(2, 'Orla', false), row(3, 'Maren', false)];
    const closing = await mount();
    await run(() => closing().closeSession());
    expect(steps).toEqual([
      ['select', 3],
      ['session_close', { session: 2 }],
    ]);
  });

  it('brings the row before it to the front when the selected session is last', async () => {
    sessions.selected = 3;
    sessions.rows = [row(1, 'Tolliver', false), row(2, 'Orla', false), row(3, 'Maren', false)];
    const closing = await mount();
    await run(() => closing().closeSession());
    expect(steps).toEqual([
      ['select', 2],
      ['session_close', { session: 3 }],
    ]);
  });

  it('closes a session behind without moving the selection', async () => {
    sessions.rows = [row(1, 'Tolliver', false), row(2, 'Orla', false)];
    const closing = await mount();
    await run(() => closing().closeSession(2));
    expect(steps).toEqual([['session_close', { session: 2 }]]);
  });

  it('asks while the session is connected, and closes it on the danger button', async () => {
    sessions.rows = [row(1, 'Tolliver', false), row(2, 'Orla', true)];
    const closing = await mount();
    await run(() => closing().closeSession(2));
    expect(closing().asking).not.toBeNull();
    expect(steps).toEqual([]);
    await run(() => closing().asking?.onConfirm());
    expect(steps).toEqual([['session_close', { session: 2 }]]);
  });

  it('runs Close window with one session', async () => {
    sessions.rows = [row(1, 'Tolliver', false)];
    const closing = await mount();
    await run(() => closing().closeSession());
    expect(steps).toEqual(['let go', 'close window']);
  });
});

describe('Quit', () => {
  it('quits at once with one session connected', async () => {
    sessions.rows = [row(1, 'Tolliver', true), row(2, 'Orla', false)];
    const closing = await mount();
    await run(() => closing().quit());
    expect(closing().asking).toBeNull();
    expect(steps).toEqual([['app_quit', undefined]]);
  });

  it('asks while two sessions are connected', async () => {
    sessions.rows = [row(1, 'Tolliver', true), row(2, 'Orla', true)];
    const closing = await mount();
    await run(() => closing().quit());
    expect(closing().asking).not.toBeNull();
    expect(steps).toEqual([]);
    await run(() => closing().asking?.onConfirm());
    expect(steps).toEqual([['app_quit', undefined]]);
  });
});
