import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Drives the reconnect store through a fake Tauri event bus with
// Tolliver's session (1) selected and Orla's (2) behind it, to hold it
// to what the reconnect notice of the Alerts review reads. Each test
// loads fresh store modules.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string) => {
    if (cmd === 'sessions_list') return [];
    throw new Error(`no fake for ${cmd}`);
  },
}));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const TOLLIVER = 1;
const ORLA = 2;

const redial = (session: number, payload: object) =>
  fire('session://reconnect', { session, ...payload });
const WORLD = { host: 'play.theforsakenlands.com', port: 1848, tls: false };
const connecting = (session: number, at = WORLD) =>
  fire('session://state', { session, kind: 'connecting', ...at });
const disconnected = (session: number, reason: string | null) =>
  fire('session://state', { session, kind: 'disconnected', reason });

async function load() {
  const sessions = await import('./sessionsStore');
  const reconnect = await import('./reconnectStore');
  sessions.startSessionsStore();
  reconnect.startReconnectStore();
  await settle();
  return reconnect;
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('the reconnect store', () => {
  it('reads none before a drop', async () => {
    const r = await load();
    expect(r.reconnectOf(TOLLIVER)).toEqual({ kind: 'none' });
  });

  it('counts a wait to the time its try dials', async () => {
    vi.useFakeTimers({ now: 1_000_000, toFake: ['Date'] });
    const r = await load();
    disconnected(TOLLIVER, 'server closed connection');
    redial(TOLLIVER, { kind: 'waiting', try: 1, tries: 8, seconds: 5 });
    expect(r.reconnectOf(TOLLIVER)).toEqual({
      kind: 'waiting',
      try: 1,
      tries: 8,
      until: 1_005_000,
    });
  });

  it('says which try dials, through the connect each try makes', async () => {
    const r = await load();
    redial(TOLLIVER, { kind: 'dialing', try: 2, tries: 8 });
    connecting(TOLLIVER);
    expect(r.reconnectOf(TOLLIVER)).toEqual({ kind: 'dialing', try: 2, tries: 8 });
  });

  it('keeps the wait or the dial when a try fails, since the next step follows', async () => {
    const r = await load();
    redial(TOLLIVER, { kind: 'dialing', try: 3, tries: 8 });
    disconnected(TOLLIVER, null);
    redial(TOLLIVER, {
      kind: 'failed',
      try: 3,
      tries: 8,
      reason: 'the game refused the connection',
    });
    expect(r.reconnectOf(TOLLIVER)).toEqual({ kind: 'dialing', try: 3, tries: 8 });
  });

  it.each([{ kind: 'reached', try: 4 }, { kind: 'cancelled' }, { kind: 'declined', why: 'quit' }])(
    'clears on $kind',
    async (payload) => {
      const r = await load();
      redial(TOLLIVER, { kind: 'waiting', try: 4, tries: 8, seconds: 20 });
      redial(TOLLIVER, payload);
      expect(r.reconnectOf(TOLLIVER)).toEqual({ kind: 'none' });
    },
  );

  it('holds a stopped notice until the next connect', async () => {
    const r = await load();
    redial(TOLLIVER, { kind: 'stopped', tries: 8 });
    disconnected(TOLLIVER, null);
    expect(r.reconnectOf(TOLLIVER)).toEqual({ kind: 'stopped', tries: 8 });
    connecting(TOLLIVER);
    expect(r.reconnectOf(TOLLIVER)).toEqual({ kind: 'none' });
  });

  it('keeps each session apart', async () => {
    const r = await load();
    redial(ORLA, { kind: 'stopped', tries: 8 });
    expect(r.reconnectOf(ORLA)).toEqual({ kind: 'stopped', tries: 8 });
    expect(r.reconnectOf(TOLLIVER)).toEqual({ kind: 'none' });
    connecting(TOLLIVER);
    expect(r.reconnectOf(ORLA)).toEqual({ kind: 'stopped', tries: 8 });
  });

  it('names where the waiting try dials, the address the drop left', async () => {
    const r = await load();
    expect(r.waitingTarget(TOLLIVER)).toBeNull();
    connecting(TOLLIVER);
    disconnected(TOLLIVER, 'server closed connection');
    redial(TOLLIVER, { kind: 'waiting', try: 1, tries: 8, seconds: 5 });
    expect(r.waitingTarget(TOLLIVER)).toEqual(WORLD);
    redial(TOLLIVER, { kind: 'dialing', try: 1, tries: 8 });
    expect(r.waitingTarget(TOLLIVER)).toBeNull();
  });
});
