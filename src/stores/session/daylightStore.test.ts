import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../../ipc/session';

// Drives the daylight store through a fake Tauri event bus. Each test
// loads fresh store modules, since they keep their state at module scope.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const commands = new Map<string, (args: unknown) => unknown>();
const calls: [string, unknown][] = [];

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
  invoke: async (cmd: string, args?: unknown) => {
    calls.push([cmd, args]);
    const answer = commands.get(cmd);
    if (!answer) throw new Error(`no fake for ${cmd}`);
    return answer(args);
  },
}));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const row = (id: number, character: string, selected = false): SessionRow => ({
  id,
  name: null,
  character,
  host: 'play.theforsakenlands.com',
  port: 1848,
  tls: false,
  profile: character,
  connected: true,
  since: null,
  selected,
});

const ROWS = [row(1, 'Tolliver', true), row(2, 'Orla')];
const ORLA_SELECTED = [row(1, 'Tolliver'), row(2, 'Orla', true)];

/** What daylight_get answers for each session, and for none. */
let phases: Record<string, string | null> = {};

async function load() {
  const store = await import('./daylightStore');
  const seen: (string | null)[] = [];
  store.subscribeDaylight(() => seen.push(store.getDaylight()));
  await settle();
  return { store, seen };
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  commands.clear();
  calls.length = 0;
  phases = { none: 'night', '1': 'night', '2': 'day' };
  commands.set('sessions_list', () => ROWS);
  commands.set('session_select', () => null);
  commands.set('daylight_get', (args) => {
    const session = (args as { session?: number }).session;
    return phases[session === undefined ? 'none' : String(session)];
  });
});

describe('daylightStore', () => {
  it('reads the selected session phase as the window opens', async () => {
    const { store } = await load();
    expect(store.getDaylight()).toBe('night');
    expect(calls).toContainEqual(['daylight_get', { session: undefined }]);
  });

  it('is null before the game says', async () => {
    phases = { none: null };
    const { store } = await load();
    expect(store.getDaylight()).toBeNull();
  });

  it('takes a turn of the selected session and none of a session behind', async () => {
    const { store, seen } = await load();
    fire('vosh://daylight-changed', { phase: 'day', session: 2 });
    expect(store.getDaylight()).toBe('night');
    fire('vosh://daylight-changed', { phase: 'day', session: 1 });
    expect(store.getDaylight()).toBe('day');
    fire('vosh://daylight-changed', { phase: 'dusk', session: 1 });
    expect(store.getDaylight()).toBe('day');
    expect(seen).toEqual(['night', 'day']);
  });

  it('reads the phase of the session a selection brings to the front', async () => {
    const { store } = await load();
    fire('vosh://sessions-changed', ORLA_SELECTED);
    await settle();
    expect(calls).toContainEqual(['daylight_get', { session: 2 }]);
    expect(store.getDaylight()).toBe('day');
    // Its turns count now, and the first session's no longer do.
    fire('vosh://daylight-changed', { phase: 'night', session: 1 });
    expect(store.getDaylight()).toBe('day');
    fire('vosh://daylight-changed', { phase: 'night', session: 2 });
    expect(store.getDaylight()).toBe('night');
  });

  it('reads again when a banner click selects a session', async () => {
    const { store } = await load();
    commands.set('sessions_list', () => ORLA_SELECTED);
    fire('vosh://session-selected', { session: 2 });
    await settle();
    await settle();
    expect(store.getDaylight()).toBe('day');
  });

  it('keeps a turn heard while a read was on its way', async () => {
    let answer: (phase: string) => void = () => undefined;
    commands.set('daylight_get', () => new Promise((resolve) => (answer = resolve)));
    const { store } = await load();
    fire('vosh://daylight-changed', { phase: 'day', session: 1 });
    answer('night');
    await settle();
    expect(store.getDaylight()).toBe('day');
  });
});
