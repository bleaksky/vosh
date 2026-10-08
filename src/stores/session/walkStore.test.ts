import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { WalkRoute } from '../../panel/map/mapWalk';

// Drives the walk store through a fake Tauri event bus with two
// sessions, Tolliver's (1) and Orla's (2). Each test loads fresh store
// modules, since they keep their state at module scope.

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

const select = (selected: number) =>
  fire(
    'vosh://sessions-changed',
    [TOLLIVER, ORLA].map((id) => ({
      id,
      name: null,
      character: id === TOLLIVER ? 'Tolliver' : 'Orla',
      host: 'play.theforsakenlands.com',
      port: 1848,
      tls: false,
      profile: id === TOLLIVER ? 'Tolliver' : 'Orla',
      connected: true,
      selected: id === selected,
    })),
  );

const walk = (session: number, progress: object) =>
  fire('session://walk', { session, ...progress });

const walking = (done: number, left: string, route = true) => ({
  kind: 'walking',
  done,
  total: 10,
  left,
  route,
});

/** Ten steps north from the fountain, your room first. */
const ROUTE: WalkRoute = {
  cells: Array.from({ length: 11 }, (_, i) => ({ row: 10 - i, col: 10 })),
  rooms: Array.from({ length: 11 }, (_, i) => 3001 + i),
  target: { row: 0, col: 10 },
  kind: 'open',
};

async function load() {
  const store = await import('./walkStore');
  store.startWalkStore();
  await settle();
  select(TOLLIVER);
  return store;
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

describe('walkStore', () => {
  it('starts idle with no route', async () => {
    const store = await load();
    expect(store.getWalk()).toEqual({ progress: { kind: 'idle' }, route: null });
  });

  it('shows the selected session walk and keeps each session apart', async () => {
    const store = await load();
    store.noteWalkRoute(TOLLIVER, ROUTE);
    walk(TOLLIVER, walking(2, '8n'));
    walk(ORLA, walking(1, '2e', false));
    expect(store.getWalk()).toEqual({ progress: walking(2, '8n'), route: ROUTE });
    select(ORLA);
    expect(store.getWalk()).toEqual({ progress: walking(1, '2e', false), route: null });
    select(TOLLIVER);
    expect(store.getWalk().route).toBe(ROUTE);
  });

  it('keeps the route after a stop and drops it on arrival', async () => {
    const store = await load();
    store.noteWalkRoute(TOLLIVER, ROUTE);
    walk(TOLLIVER, { kind: 'stopped', done: 4, total: 10, why: 'lost_sight' });
    expect(store.getWalk().route).toBe(ROUTE);
    store.noteWalkRoute(TOLLIVER, ROUTE);
    walk(TOLLIVER, walking(9, 'n'));
    walk(TOLLIVER, { kind: 'idle' });
    expect(store.getWalk()).toEqual({ progress: { kind: 'idle' }, route: null });
  });

  it('drops the route when a walk you typed takes its place', async () => {
    const store = await load();
    store.noteWalkRoute(TOLLIVER, ROUTE);
    walk(TOLLIVER, walking(1, '9n'));
    walk(TOLLIVER, walking(0, '3e', false));
    expect(store.getWalk().route).toBeNull();
  });

  it('keeps a route sent while a walk you typed finishes its step', async () => {
    const store = await load();
    walk(TOLLIVER, walking(1, '2e', false));
    store.noteWalkRoute(TOLLIVER, ROUTE);
    walk(TOLLIVER, walking(2, 'e', false));
    expect(store.getWalk().route).toBe(ROUTE);
    walk(TOLLIVER, walking(0, '10n'));
    expect(store.getWalk().route).toBe(ROUTE);
  });

  it('resets the session that disconnects and only that one', async () => {
    const store = await load();
    store.noteWalkRoute(TOLLIVER, ROUTE);
    walk(TOLLIVER, walking(3, '7n'));
    store.noteWalkRoute(ORLA, ROUTE);
    walk(ORLA, walking(1, '9n'));
    fire('session://state', { session: ORLA, kind: 'disconnected', reason: null });
    expect(store.getWalk()).toEqual({ progress: walking(3, '7n'), route: ROUTE });
    fire('session://state', { session: TOLLIVER, kind: 'disconnected', reason: null });
    expect(store.getWalk()).toEqual({ progress: { kind: 'idle' }, route: null });
    select(ORLA);
    expect(store.getWalk()).toEqual({ progress: { kind: 'idle' }, route: null });
  });
});
