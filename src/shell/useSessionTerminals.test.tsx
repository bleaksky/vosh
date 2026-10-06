import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { dismissToast, getToasts } from '../stores/toasts';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';

// The toasts a drop of the selected session shows (board 7 of the Alerts
// review). Tolliver plays session 1, the selected one, and Orla plays
// session 2 behind it. The reconnect notice takes the place of the
// Connection lost toast, and a drop Vosh will not redial says why.

type Handler = (event: { payload: unknown }) => void;
const handlers = vi.hoisted(() => new Map<string, Handler>());

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, cb: Handler) => {
    handlers.set(event, cb);
    return Promise.resolve(() => handlers.delete(event));
  },
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: () => Promise.resolve() }));
vi.mock('../stores/session/sessionsStore', () => ({
  getSelected: () => 1,
  othersOnProfile: () => [],
  getSessions: () => [
    { id: 1, character: 'Tolliver' },
    { id: 2, character: 'Orla' },
  ],
}));
vi.mock('../stores/session/connectionStore', () => ({ noteConnectionError: () => {} }));
vi.mock('../terminal/terminalRenderer', () => ({ nativeSurfaceEnabled: () => false }));
vi.mock('./launchNotices', () => ({ showLaunchNotices: () => Promise.resolve() }));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let unmount: (() => Promise<void>) | null = null;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    setTimeout: () => 1,
    clearTimeout: () => {},
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  ({ createRoot } = await import('react-dom/client'));
});

beforeEach(async () => {
  const { useSessionTerminals } = await import('./useSessionTerminals');
  function Probe() {
    useSessionTerminals(1, [1, 2]);
    return null;
  }
  const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
  await act(async () => root.render(createElement(Probe)));
  await act(async () => {});
  unmount = () => act(async () => root.unmount());
});

afterEach(async () => {
  await unmount?.();
  for (const toast of getToasts()) dismissToast(toast.id);
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const fire = (event: string, payload: object) =>
  act(async () => handlers.get(event)?.({ payload }));
const drop = (session: number) =>
  fire('session://state', { session, kind: 'disconnected', reason: 'connection reset by server' });
const redial = (session: number, payload: object) =>
  fire('session://reconnect', { session, ...payload });
const shown = () => getToasts().map(({ message, meta }) => [message, meta]);

const LOST = ['Connection lost', 'connection reset by server'];

describe('the toasts of a drop', () => {
  it('give way to the notice at the first wait of a redial', async () => {
    await drop(1);
    expect(shown()).toEqual([LOST]);
    await redial(1, { kind: 'waiting', try: 1, tries: 8, seconds: 3 });
    expect(shown()).toEqual([]);
  });

  it('say why Vosh will not redial after your quit', async () => {
    await drop(1);
    await redial(1, { kind: 'declined', why: 'quit' });
    expect(shown()).toEqual([['Vosh will not reconnect', 'you quit']]);
  });

  it('name a ban and the character another session took', async () => {
    await redial(1, { kind: 'declined', why: 'banned' });
    await redial(1, { kind: 'declined', why: 'taken' });
    expect(shown()).toEqual([
      ['Vosh will not reconnect', 'the game banned this account'],
      ['Vosh will not reconnect', 'another session took Tolliver'],
    ]);
  });

  it('keep Connection lost while Reconnect is off', async () => {
    await drop(1);
    await redial(1, { kind: 'declined', why: 'off' });
    expect(shown()).toEqual([LOST]);
  });

  it('stay quiet for a session behind the selected one', async () => {
    await drop(2);
    await redial(2, { kind: 'declined', why: 'quit' });
    expect(shown()).toEqual([]);
  });
});
