import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { aabahranChatPacket, aabahranPacket } from '../../test/aabahranGmcp';

// Drives the GMCP stores through a fake Tauri event bus with two
// sessions, Tolliver's (1) and Orla's (2), to hold each store to one
// state for each session. Each test loads fresh store modules.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const commands = new Map<string, (args: { session?: number }) => unknown>();
/** The session each snapshot command was asked for, by command. */
const asked = new Map<string, number[]>();

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
    if (args.session !== undefined) asked.set(cmd, [...(asked.get(cmd) ?? []), args.session]);
    const answer = commands.get(cmd);
    if (!answer) throw new Error(`no fake for ${cmd}`);
    return answer(args);
  },
}));

vi.mock('../session/tickSound', () => ({ playTickSound: vi.fn() }));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const TOLLIVER = 1;
const ORLA = 2;

/** One packet from a session, as the backend emits it. */
const gmcp = (session: number, pkg: string, data: unknown) =>
  fire(`session://gmcp/${pkg.replace(/\./g, '-')}`, { session, data });
const vitals = (session: number, hp: number, maxhp: number) =>
  gmcp(session, 'Char.Vitals', { hp, maxhp, mana: 760, maxmana: 820, move: 250, maxmove: 250 });
const disconnect = (session: number) =>
  fire('session://state', { session, kind: 'disconnected', reason: null });

/** The list the app sends, with `selected` the one selected. */
const select = (selected: number, ids = [TOLLIVER, ORLA]) =>
  fire(
    'vosh://sessions-changed',
    ids.map((id) => ({
      id,
      name: null,
      character: id === TOLLIVER ? 'Tolliver' : 'Orla',
      host: 'play.theforsakenlands.com',
      port: id === TOLLIVER ? 1848 : 1825,
      tls: false,
      profile: id === TOLLIVER ? 'Tolliver' : 'Orla',
      connected: true,
      selected: id === selected,
    })),
  );

async function load() {
  const stores = await import('../index');
  stores.startStores();
  await settle();
  return {
    vitals: await import('./vitalsStore'),
    affects: await import('./affectsStore'),
    chat: await import('./chatStore'),
    room: await import('./roomStore'),
    world: await import('./worldStore'),
  };
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  commands.clear();
  asked.clear();
  vi.stubGlobal('window', globalThis);
  commands.set('ui_get_config', () => ({}));
  commands.set('target_get', () => ({ name: null, room_idx: null, quick_keys: [] }));
  commands.set('tick_get_config', () => ({ enabled: false, interval_secs: 30 }));
  commands.set('sessions_list', () => []);
  commands.set('hidden_get', () => null);
  commands.set('affects_snapshot_get', () => null);
  commands.set('affect_full_get', () => ({}));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('a GMCP store with two sessions', () => {
  it('keeps the Char.Vitals of each session apart', async () => {
    const s = await load();
    vitals(TOLLIVER, 850, 900);
    vitals(ORLA, 1020, 1020);
    expect(s.vitals.getVitals()).toMatchObject({ hp: 850, maxhp: 900 });
    select(ORLA);
    expect(s.vitals.getVitals()).toMatchObject({ hp: 1020, maxhp: 1020 });
    select(TOLLIVER);
    expect(s.vitals.getVitals()).toMatchObject({ hp: 850, maxhp: 900 });
  });

  it('gives each session its own chat, room and game time', async () => {
    const s = await load();
    const tell = aabahranChatPacket('tell.gmcp');
    const yell = aabahranChatPacket('yell.gmcp');
    gmcp(TOLLIVER, tell.package, tell.data);
    gmcp(TOLLIVER, 'World.Time', { hour: 6, sunlight: 'rise' });
    const room = aabahranPacket('room-info.gmcp');
    gmcp(TOLLIVER, room.package, room.data);
    gmcp(ORLA, yell.package, yell.data);
    gmcp(ORLA, 'World.Time', { hour: 21, sunlight: 'dark' });

    expect(s.chat.getChatLines().map((l) => l.speaker)).toEqual(['Tolliver']);
    expect(s.world.getWorld().time?.hour).toBe(6);
    expect(s.room.getRoom().info?.name).toBe('The Bank of Aabahran');
    select(ORLA);
    expect(s.chat.getChatLines().map((l) => l.speaker)).toEqual(['a Blackwatch villager']);
    expect(s.world.getWorld().time?.hour).toBe(21);
    expect(s.room.getRoom().info).toBeNull();
  });

  it('leaves the other session as it was at a disconnect', async () => {
    const s = await load();
    const tell = aabahranChatPacket('tell.gmcp');
    vitals(TOLLIVER, 850, 900);
    gmcp(TOLLIVER, tell.package, tell.data);
    vitals(ORLA, 1020, 1020);
    gmcp(ORLA, tell.package, tell.data);
    const before = s.vitals.getVitals();
    const heard = vi.fn();
    s.chat.subscribeChatLines(heard);

    disconnect(ORLA);
    // Nothing Tolliver's panes read moved.
    expect(s.vitals.getVitals()).toBe(before);
    expect(s.chat.getChatLines()).toHaveLength(1);
    expect(heard).not.toHaveBeenCalled();

    select(ORLA);
    expect(s.vitals.getVitals()).toBeNull();
    expect(s.chat.getChatLines()).toEqual([]);
  });

  it('publishes the selected session again on a selection, with no low latch carried over', async () => {
    const s = await load();
    const tell = aabahranChatPacket('tell.gmcp');
    gmcp(ORLA, tell.package, tell.data);
    // Tolliver falls under 20 percent and his health reads low.
    vitals(TOLLIVER, 150, 1000);
    // Orla sits at 22 percent, which reads low only for a vital that did.
    vitals(ORLA, 220, 1000);
    expect(s.vitals.getVitals()?.low.hp).toBe(true);
    const heard = vi.fn();
    s.chat.subscribeChatLines(heard);

    select(ORLA);
    expect(heard).toHaveBeenCalledTimes(1);
    expect(s.chat.getChatLines()).toHaveLength(1);
    expect(s.vitals.getVitals()).toMatchObject({ hp: 220, low: { hp: false } });
    select(TOLLIVER);
    expect(heard).toHaveBeenCalledTimes(2);
    expect(s.vitals.getVitals()).toMatchObject({ hp: 150, low: { hp: true } });
    // Within one session the latch holds, so Tolliver at 22 percent
    // still reads low.
    vitals(TOLLIVER, 220, 1000);
    expect(s.vitals.getVitals()?.low.hp).toBe(true);
  });

  it('asks each session for its snapshot once, the first time it is selected', async () => {
    commands.set('affects_snapshot_get', ({ session }) =>
      session === ORLA ? aabahranPacket('char-affects.gmcp').data : null,
    );
    const s = await load();
    expect(s.affects.getAffects()).toBeNull();
    select(ORLA);
    await settle();
    expect(s.affects.getAffects()?.map((a) => a.name)).toEqual([
      'bless',
      'armor',
      'bagatelle of bravado',
    ]);
    select(TOLLIVER);
    select(ORLA);
    await settle();
    for (const cmd of ['affects_snapshot_get', 'affect_full_get', 'hidden_get']) {
      expect(asked.get(cmd), cmd).toEqual([TOLLIVER, ORLA]);
    }
  });

  it('drops the state of a session that leaves the list', async () => {
    const s = await load();
    vitals(ORLA, 1020, 1020);
    select(TOLLIVER, [TOLLIVER]);
    // The app never gives a closed session's id to another. It comes back
    // here only to show that nothing of it stayed.
    select(ORLA);
    expect(s.vitals.getVitals()).toBeNull();
  });
});
