import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { LuaPane } from '../../ipc/panes';

// Drives the Lua pane store and the plugin rows store through a fake
// Tauri event bus with two sessions, Tolliver's (1) and Orla's (2). Each
// test loads fresh store modules.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const commands = new Map<string, (args: { session?: number }) => unknown>();

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
  invoke: async (cmd: string, args: { session?: number } = {}) => {
    const answer = commands.get(cmd);
    if (!answer) throw new Error(`no fake for ${cmd}`);
    return answer(args);
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

/** The weather pane with the sky it shows. */
const weather = (sky: string): LuaPane => ({
  plugin: 'weather_pane',
  id: 'weather',
  title: 'Weather',
  meta: 'Coastal North',
  blocks: [{ kind: 'row', label: 'Sky', value: sky }],
});

const panes = (session: number, list: LuaPane[], removed: LuaPane[] = []) =>
  fire('session://lua-panes', {
    session,
    panes: list,
    removed: removed.map(({ plugin, id }) => ({ plugin, id })),
  });

/** The sky the weather pane of the session in front shows. */
const sky = (store: typeof import('./luaPanesStore')) => {
  const [pane, ...more] = store.getLuaPanes().values();
  expect(more).toEqual([]);
  const row = pane?.blocks[0];
  return row?.kind === 'row' ? row.value : undefined;
};

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  commands.clear();
  vi.stubGlobal('window', globalThis);
  commands.set('sessions_list', () => []);
  commands.set('lua_panes_get', () => []);
});

async function load() {
  const sessions = await import('./sessionsStore');
  sessions.startSessionsStore();
  const store = await import('./luaPanesStore');
  store.startLuaPanesStore();
  await settle();
  return store;
}

describe('the Lua pane store', () => {
  it('keeps the panes of each session apart and shows the one in front', async () => {
    const store = await load();
    select(TOLLIVER);
    panes(TOLLIVER, [weather('rainy')]);
    panes(ORLA, [weather('cloudless')]);
    expect(sky(store)).toBe('rainy');
    select(ORLA);
    expect(sky(store)).toBe('cloudless');
    panes(TOLLIVER, [weather('lightning')]);
    expect(sky(store)).toBe('cloudless');
    select(TOLLIVER);
    expect(sky(store)).toBe('lightning');
  });

  it('keeps the panes through a disconnect', async () => {
    const store = await load();
    panes(TOLLIVER, [weather('rainy')]);
    fire('session://state', { session: TOLLIVER, kind: 'disconnected', reason: null });
    expect(sky(store)).toBe('rainy');
  });

  it('empties a pane its plugin removed', async () => {
    const store = await load();
    panes(TOLLIVER, [weather('rainy')]);
    panes(TOLLIVER, [], [weather('rainy')]);
    expect(store.getLuaPanes().size).toBe(0);
  });

  it('fills from the snapshot of a session it shows first', async () => {
    commands.set('lua_panes_get', ({ session }) =>
      session === ORLA ? [weather('cloudless')] : [],
    );
    const store = await load();
    select(ORLA);
    await settle();
    expect(sky(store)).toBe('cloudless');
  });

  it('drops a snapshot when an event landed after the ask', async () => {
    let answer: (panes: LuaPane[]) => void = () => undefined;
    commands.set('lua_panes_get', () => new Promise<LuaPane[]>((done) => (answer = done)));
    const store = await load();
    panes(TOLLIVER, [weather('lightning')]);
    answer([weather('rainy')]);
    await settle();
    expect(sky(store)).toBe('lightning');
  });
});

describe('the plugin rows store', () => {
  const row = (on: boolean) => ({
    name: 'weather_pane',
    version: '1.0.0',
    author: 'Orla',
    description: '',
    entry: 'main.lua',
    on,
    stopped: null,
    loaded_ms: null,
    misnamed: false,
  });

  it('reads the rows of the session in front again on each change and selection', async () => {
    const asked: (number | undefined)[] = [];
    let on = true;
    commands.set('plugins_list', ({ session }) => {
      asked.push(session);
      return [row(on)];
    });
    const sessions = await import('./sessionsStore');
    sessions.startSessionsStore();
    const rows = await import('./pluginRowsStore');
    rows.startPluginRowsStore();
    await settle();
    expect(rows.getPluginRows()?.[0].on).toBe(true);
    on = false;
    fire('vosh://plugins-changed', null);
    await settle();
    expect(rows.getPluginRows()?.[0].on).toBe(false);
    select(ORLA);
    await settle();
    expect(asked.at(-1)).toBe(ORLA);
  });
});
