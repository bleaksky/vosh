import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import lament from '../../../fixtures/gmcp/aabahran/lament.json';
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
    combat: await import('./combatStore'),
    group: await import('./groupStore'),
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

  it('reads the room, fight and vitals of a session behind for its row', async () => {
    const s = await load();
    const room = aabahranPacket('room-info.gmcp');
    const fight = aabahranPacket('char-combat.gmcp');
    const heard: number[] = [];
    s.room.subscribeRoomOf((session) => heard.push(session));
    gmcp(ORLA, room.package, room.data);
    gmcp(ORLA, fight.package, fight.data);
    vitals(ORLA, 162, 900);
    expect(heard).toEqual([ORLA]);
    expect(s.room.getRoomOf(ORLA)?.name).toBe('The Bank of Aabahran');
    expect(s.combat.getCombatOf(ORLA)?.name).toBe('a Blackwatch guard');
    expect(s.vitals.getVitalsOf(ORLA)).toMatchObject({ hp: 162, maxhp: 900, low: { hp: true } });
    expect(s.room.getRoomOf(TOLLIVER)).toBeNull();
    expect(s.combat.getCombatOf(TOLLIVER)).toBeNull();
    expect(s.vitals.getVitalsOf(TOLLIVER)).toBeNull();
    expect(s.room.getRoom().info).toBeNull();
  });

  it('keeps each session its own vitals history and forgets it at a disconnect', async () => {
    const s = await load();
    vitals(TOLLIVER, 905, 1038);
    gmcp(TOLLIVER, 'Char.Vitals', { hp: 0, maxhp: 0, hidden: true });
    vitals(TOLLIVER, 744, 1038);
    vitals(ORLA, 1020, 1020);
    expect(s.vitals.getVitalsHistoryOf(TOLLIVER).map((sample) => sample.values.hp)).toEqual([
      905, 744,
    ]);
    expect(s.vitals.getVitalsHistoryOf(ORLA)).toHaveLength(1);
    disconnect(TOLLIVER);
    expect(s.vitals.getVitalsHistoryOf(TOLLIVER)).toEqual([]);
    expect(s.vitals.getVitalsHistoryOf(ORLA)).toHaveLength(1);
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

  it('reads the hidden flags of the same session in each view', async () => {
    const s = await load();
    const send = (session: number, name: string) => {
      const p = aabahranPacket(name);
      gmcp(session, p.package, p.data);
    };
    for (const name of ['char-affects.gmcp', 'char-vitals.gmcp', 'char-combat.gmcp']) {
      send(TOLLIVER, name);
    }
    send(TOLLIVER, 'group-info.gmcp');
    const heard = vi.fn();
    s.combat.subscribeCombat(heard);
    // Orla plays on the older build, which sends the true values under
    // lamented tears, and the backend reports them hidden for her alone.
    const older = lament.cases[2];
    for (const name of older.packets) send(ORLA, name);
    fire('session://hidden', { session: ORLA, ...older.hidden });

    expect(heard).not.toHaveBeenCalled();
    expect(s.vitals.getVitals()).toMatchObject({ hp: 850, hidden: false });
    expect(s.combat.getCombat()).toMatchObject({ hp_pct: 54, hidden: false });
    expect(s.group.getGroupState().group.hidden).toBeUndefined();
    expect(s.affects.getAffectsHidden()).toBe(false);

    select(ORLA);
    expect(s.vitals.getVitals()).toMatchObject({ hp: 0, maxhp: 0, hidden: true });
    expect(s.combat.getCombat()).toMatchObject({ hp_pct: null, condition: null, hidden: true });
    expect(s.group.getGroupState().group).toEqual({ hidden: true });
    expect(s.affects.getAffectsHidden()).toBe(true);

    select(TOLLIVER);
    expect(s.vitals.getVitals()).toMatchObject({ hp: 850, hidden: false });
    expect(s.combat.getCombat()).toMatchObject({ hp_pct: 54, hidden: false });
    expect(s.affects.getAffectsHidden()).toBe(false);
  });

  it('holds back what the prompt read under the song in a session behind', async () => {
    const s = await load();
    const zeros = { hp: '0', maxhp: '0', mana: '0', maxmana: '0', move: '0', maxmove: '0' };
    const nothing = { vitals: false, tank: false, opponent: false, affects: false, group: false };
    // The song hides Orla's vitals while Tolliver is in front, and her
    // prompt reads the zeros the game prints meanwhile.
    fire('session://hidden', { session: ORLA, ...nothing, vitals: true });
    fire('session://prompt-vars', { session: ORLA, data: zeros });
    // The song ends, and the game sends her true vitals.
    fire('session://hidden', { session: ORLA, ...nothing });
    const p = aabahranPacket('char-vitals.gmcp');
    gmcp(ORLA, p.package, p.data);

    select(ORLA);
    expect(s.vitals.getVitals()).toMatchObject({ hp: 850, maxhp: 900, hidden: false });
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
