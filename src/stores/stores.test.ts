import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { aabahranPacket } from '../test/aabahranGmcp';

// Drives the stores through a fake Tauri event bus, so the channel
// names, the disconnect handling and the start wiring are checked the
// way the app runs them. Each test loads fresh store modules.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const commands = new Map<string, unknown>();

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
    if (!commands.has(cmd)) throw new Error(`no fake for ${cmd}`);
    return commands.get(cmd);
  },
}));

// The tick sound is Web Audio, so the store tests only check when it
// plays.
const playTickSound = vi.hoisted(() => vi.fn());
vi.mock('./session/tickSound', () => ({ playTickSound }));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

// A GMCP packet, the prompt values and the affect fulls come inside
// {session, data}, here from session 1.
const gmcp = (pkg: string, data: unknown) =>
  fire(`session://gmcp/${pkg.replace(/\./g, '-')}`, { session: 1, data });
const promptVars = (data: unknown) => fire('session://prompt-vars', { session: 1, data });
const fulls = (data: unknown) => fire('vosh://affect-full-changed', { session: 1, data });
/** Send one packet from fixtures/gmcp/aabahran, as the backend emits it. */
const packet = (name: string) => {
  const p = aabahranPacket(name);
  gmcp(p.package, p.data);
};
const disconnect = () => fire('session://state', { kind: 'disconnected', reason: null });
const connect = () => {
  const at = { host: 'play.example', port: 4000, tls: false };
  fire('session://state', { kind: 'connecting', ...at });
  fire('session://state', { kind: 'connected', ...at });
};

// Let the listen and invoke promises settle.
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

async function load() {
  const stores = await import('./index');
  stores.startStores();
  await settle();
  return {
    vitals: await import('./gmcp/vitalsStore'),
    affects: await import('./gmcp/affectsStore'),
    tracked: await import('./config/trackedAffectsStore'),
    combat: await import('./gmcp/combatStore'),
    world: await import('./gmcp/worldStore'),
    room: await import('./gmcp/roomStore'),
    target: await import('./session/targetStore'),
    tick: await import('./session/tickStore'),
    chipStyle: await import('./config/chipStyleStore'),
    echoMark: await import('./config/echoMarkStore'),
    tickCount: await import('./config/tickCountStore'),
    gameTime: await import('./config/gameTimeStore'),
    vitalsOptions: await import('./config/vitalsOptionsStore'),
    affectsDisplay: await import('./config/affectsDisplayStore'),
    chatColors: await import('./config/chatColorsStore'),
    affectFull: await import('./gmcp/affectFullStore'),
    group: await import('./gmcp/groupStore'),
    gamePrompt: await import('./gmcp/gamePromptStore'),
  };
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  commands.clear();
  playTickSound.mockClear();
  vi.stubGlobal('window', globalThis);
  commands.set('ui_get_config', { tracked_affects: [{ name: 'sanctuary' }, 'haste'] });
  commands.set('target_get', { name: 'guard', room_idx: 2, quick_keys: [] });
  commands.set('tick_get_config', {
    enabled: true,
    interval_secs: 30,
    auto_fire: null,
    sound: false,
    reset_pattern: null,
    warn_at_secs: 8,
    warn_message: null,
    warn_color: null,
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('stores on the event bus', () => {
  it('fill from GMCP and clear what a disconnect makes stale', async () => {
    const s = await load();
    expect(s.affects.getAffects()).toBeNull();

    gmcp('Char.Vitals', { hp: 186, maxhp: 1020, mana: 800, maxmana: 800, move: 1, maxmove: 0 });
    gmcp('Char.Affects', { affects: [{ kind: 'spell', name: 'haste', duration: 8 }] });
    gmcp('Char.Combat', { target: 'a guard', condition: 'awful', hp_pct: 12 });
    gmcp('World.Time', { hour: 8, sunlight: 'light' });
    gmcp('World.Moons', { moons: [{ name: 'Lysenties', active: true, phase: 2 }] });
    gmcp('Room.Info', { num: 5279, name: 'The Bank', area: 'Blackwatch', exits: { south: 1 } });
    gmcp('Room.Chars', [{ name: 'a villager', npc: true }]);

    expect(s.vitals.getVitals()).toMatchObject({ hp: 186, low: { hp: true, move: false } });
    expect(s.affects.getAffects()?.map((a) => a.name)).toEqual(['haste']);
    expect(s.combat.getCombat()).toEqual({
      name: 'a guard',
      hp_pct: 12,
      condition: 'awful',
      hidden: false,
      tank: null,
    });
    expect(s.world.getWorld().time?.hour).toBe(8);
    expect(s.world.moonLabel(s.world.getWorld().moons)).toBe('Lysenties waxing');
    expect(s.room.getRoom().info?.exits).toEqual(['south']);

    disconnect();
    expect(s.vitals.getVitals()).toBeNull();
    expect(s.affects.getAffects()).toBeNull();
    expect(s.combat.getCombat()).toBeNull();
    // The last game time stays for the status line, the moons go.
    expect(s.world.getWorld().time?.hour).toBe(8);
    expect(s.world.getWorld().moons).toBeNull();
    // The last room stays, like the last map.
    expect(s.room.getRoom().info?.name).toBe('The Bank');
    expect(s.room.getRoom().people).toHaveLength(1);

    // A new connection may reach another world, so the time clears.
    fire('session://state', { kind: 'connecting', host: 'example.org', port: 4000, tls: false });
    expect(s.world.getWorld()).toEqual({ time: null, moons: null });
  });

  it('show the last affects list at once in a window that opens between ticks', async () => {
    commands.set('affects_snapshot_get', {
      affects: [{ kind: 'spell', name: 'sanctuary', duration: 12 }],
    });
    const s = await load();
    expect(s.affects.getAffects()?.map((a) => a.name)).toEqual(['sanctuary']);
    gmcp('Char.Affects', { affects: [{ name: 'haste', duration: 3 }] });
    expect(s.affects.getAffects()?.map((a) => a.name)).toEqual(['haste']);
  });

  it('keep a list or a disconnect that lands before the snapshot answers', async () => {
    const late = () => {
      let answer: (value: unknown) => void = () => undefined;
      commands.set(
        'affects_snapshot_get',
        new Promise((resolve) => {
          answer = resolve;
        }),
      );
      return (value: unknown) => answer(value);
    };
    const snapshot = { affects: [{ name: 'sanctuary', duration: 12 }] };

    let answer = late();
    let s = await load();
    gmcp('Char.Affects', { affects: [{ name: 'haste', duration: 3 }] });
    answer(snapshot);
    await settle();
    expect(s.affects.getAffects()?.map((a) => a.name)).toEqual(['haste']);

    vi.resetModules();
    handlers.clear();
    answer = late();
    s = await load();
    disconnect();
    answer(snapshot);
    await settle();
    expect(s.affects.getAffects()).toBeNull();
  });

  it('start empty when no connection has sent a list', async () => {
    commands.set('affects_snapshot_get', null);
    const s = await load();
    expect(s.affects.getAffects()).toBeNull();
  });

  it('notify subscribers once per change and not for repeats', async () => {
    const s = await load();
    const seen = vi.fn();
    s.combat.subscribeCombat(seen);
    gmcp('Char.Combat', { target: 'a guard', hp_pct: 50 });
    gmcp('Char.Combat', { target: 'a guard', hp_pct: 50 });
    gmcp('Char.Combat', {});
    expect(seen).toHaveBeenCalledTimes(2);
  });

  it('lay prompt vars over Char.Vitals', async () => {
    const s = await load();
    gmcp('Char.Vitals', { hp: 900, maxhp: 1000 });
    promptVars({ hp: '150' });
    expect(s.vitals.getVitals()).toMatchObject({ hp: 150, maxhp: 1000, low: { hp: true } });
  });

  it('hide your vitals under lamented tears and never fill them from prompt vars', async () => {
    const s = await load();
    packet('char-vitals.gmcp');
    promptVars({ hp: '150', maxhp: '900' });
    expect(s.vitals.getVitals()).toMatchObject({ hp: 150, hidden: false, low: { hp: true } });

    packet('char-vitals-hidden.gmcp');
    expect(s.vitals.getVitals()).toEqual({
      hp: 0,
      maxhp: 0,
      mana: 0,
      maxmana: 0,
      move: 0,
      maxmove: 0,
      low: { hp: false, mana: false, move: false },
      hidden: true,
    });
    // A prompt capture that lands while hidden changes nothing.
    promptVars({ hp: '850', maxhp: '900' });
    expect(s.vitals.getVitals()?.hidden).toBe(true);
    expect(s.vitals.getVitals()?.hp).toBe(0);

    // The next packet without the flag shows them again.
    packet('char-vitals.gmcp');
    expect(s.vitals.getVitals()).toMatchObject({ hp: 850, maxhp: 900, hidden: false });

    packet('char-vitals-hidden.gmcp');
    disconnect();
    expect(s.vitals.getVitals()).toBeNull();
  });

  it('fill your vitals from the game after the song, not from a prompt read under it', async () => {
    const s = await load();
    const shown = { hp: 850, maxhp: 900, mana: 760, maxmana: 820, move: 250, maxmove: 250 };
    const notLow = { hp: false, mana: false, move: false };
    packet('char-vitals.gmcp');
    promptVars({
      hp: '850',
      maxhp: '900',
      mana: '760',
      maxmana: '820',
      move: '250',
      maxmove: '250',
    });

    // Under the song the capture reads the zeros the text prompt prints.
    packet('char-vitals-hidden.gmcp');
    const lament = { hp: '0', maxhp: '0', mana: '0', maxmana: '0', move: '0', maxmove: '0' };
    promptVars(lament);

    // The song ends. Char.Vitals goes out before the text prompt, and
    // AFK or prompt off keeps the capture from firing at all.
    packet('char-vitals.gmcp');
    expect(s.vitals.getVitals()).toEqual({ ...shown, low: notLow, hidden: false });
    for (let i = 0; i < 3; i++) packet('char-vitals.gmcp');
    expect(s.vitals.getVitals()).toEqual({ ...shown, low: notLow, hidden: false });

    // A prompt trigger that sends the same map again changes nothing.
    promptVars(lament);
    expect(s.vitals.getVitals()).toEqual({ ...shown, low: notLow, hidden: false });

    // A capture with new numbers wins again, each var on its own.
    promptVars({ ...lament, hp: '150', maxhp: '900' });
    expect(s.vitals.getVitals()).toEqual({
      ...shown,
      hp: 150,
      low: { ...notLow, hp: true },
      hidden: false,
    });
  });

  it('fill your vitals from the game after the song when the prompt shows no max', async () => {
    // The default prompt `<%hhp %mm %vmv>` reads the current values only.
    const s = await load();
    packet('char-vitals.gmcp');
    promptVars({ hp: '850', mana: '760', move: '250' });
    packet('char-vitals-hidden.gmcp');
    promptVars({ hp: '0', mana: '0', move: '0' });
    packet('char-vitals.gmcp');
    expect(s.vitals.getVitals()).toMatchObject({
      hp: 850,
      maxhp: 900,
      mana: 760,
      move: 250,
      low: { hp: false, mana: false, move: false },
    });
  });

  it('fill your vitals from the game after the song when no prompt came during it', async () => {
    const s = await load();
    packet('char-vitals.gmcp');
    // Your last prompt before you went AFK.
    promptVars({ hp: '150', maxhp: '900' });
    packet('char-vitals-hidden.gmcp');
    packet('char-vitals.gmcp');
    expect(s.vitals.getVitals()).toMatchObject({ hp: 850, maxhp: 900, low: { hp: false } });

    // A disconnect lets go of what the song held back.
    disconnect();
    packet('char-vitals.gmcp');
    promptVars({ hp: '150', maxhp: '900' });
    expect(s.vitals.getVitals()).toMatchObject({ hp: 150, low: { hp: true } });
  });

  it('hide your affects and your group apart from each other', async () => {
    const s = await load();
    packet('char-affects.gmcp');
    packet('group-info.gmcp');
    expect(s.affects.getAffectsHidden()).toBe(false);
    expect(s.group.getGroupState().group.members).toHaveLength(2);

    // Only Char.Affects hides. The roster stays.
    packet('char-affects-hidden.gmcp');
    expect(s.affects.getAffects()).toEqual([]);
    expect(s.affects.getAffectsHidden()).toBe(true);
    expect(s.group.getGroupState().group.members).toHaveLength(2);

    // Group.Info hides on its own packet and drops the roster from
    // before the song.
    packet('group-info-hidden.gmcp');
    expect(s.group.getGroupState().group).toEqual({ hidden: true });

    packet('char-affects.gmcp');
    expect(s.affects.getAffectsHidden()).toBe(false);
    expect(s.affects.getAffects()?.map((a) => a.name)).toEqual([
      'bless',
      'armor',
      'bagatelle of bravado',
    ]);
    expect(s.group.getGroupState().group).toEqual({ hidden: true });

    packet('group-info-solo.gmcp');
    expect(s.group.getGroupState().group).toEqual({});

    packet('char-affects-hidden.gmcp');
    packet('group-info-hidden.gmcp');
    disconnect();
    expect(s.affects.getAffects()).toBeNull();
    expect(s.affects.getAffectsHidden()).toBe(false);
    expect(s.group.getGroupState().group).toEqual({});
  });

  it('read a hidden affects list from the snapshot a new window asks for', async () => {
    commands.set('affects_snapshot_get', aabahranPacket('char-affects-hidden.gmcp').data);
    const s = await load();
    expect(s.affects.getAffects()).toEqual([]);
    expect(s.affects.getAffectsHidden()).toBe(true);
  });

  it('follow the opponent health the game withholds and the tank it names', async () => {
    const s = await load();
    const seen = vi.fn();
    s.combat.subscribeCombat(seen);
    packet('char-combat-tank.gmcp');
    expect(s.combat.getCombat()).toMatchObject({
      hp_pct: 54,
      hidden: false,
      tank: { name: 'Tester', hp_pct: 78 },
    });
    packet('char-combat-tank-hidden.gmcp');
    expect(s.combat.getCombat()).toMatchObject({
      hp_pct: null,
      condition: null,
      hidden: true,
      tank: { name: 'Tester', hp_pct: null },
    });
    packet('char-combat-tank-hidden.gmcp');
    packet('char-combat.gmcp');
    expect(s.combat.getCombat()).toMatchObject({ hp_pct: 54, hidden: false, tank: null });
    packet('char-combat-end.gmcp');
    expect(s.combat.getCombat()).toBeNull();
    expect(seen).toHaveBeenCalledTimes(4);
  });

  it('keep the prompt settings until you disconnect', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2026-09-30T12:58:02-05:00'));
    const s = await load();
    expect(s.gamePrompt.getGamePrompt()).toBeNull();
    connect();
    packet('char-prompt.gmcp');
    // The first settings since you connected came at login.
    expect(s.gamePrompt.getGamePrompt()).toEqual({
      enabled: true,
      prompt: '%n%P%C<%hhp %mm %vmv> ',
      fprompt: '',
      receivedAt: Date.parse('2026-09-30T12:58:02-05:00'),
      atLogin: true,
    });

    vi.setSystemTime(new Date('2026-09-30T13:04:00-05:00'));
    packet('char-prompt-off.gmcp');
    expect(s.gamePrompt.getGamePrompt()).toMatchObject({
      enabled: false,
      receivedAt: Date.parse('2026-09-30T13:04:00-05:00'),
      atLogin: false,
    });

    disconnect();
    expect(s.gamePrompt.getGamePrompt()).toBeNull();

    // The next connection's first settings came at its login.
    connect();
    packet('char-prompt.gmcp');
    expect(s.gamePrompt.getGamePrompt()?.atLogin).toBe(true);
    packet('char-prompt-fight.gmcp');
    expect(s.gamePrompt.getGamePrompt()?.atLogin).toBe(false);
  });

  it('take settings that come with no connect seen as later ones', async () => {
    // A window that loads while you play hears no connect, so it never
    // calls a packet the login one.
    const s = await load();
    packet('char-prompt.gmcp');
    expect(s.gamePrompt.getGamePrompt()?.atLogin).toBe(false);
  });

  it('seed the tracked list and follow broadcasts and profile switches', async () => {
    const s = await load();
    expect(s.tracked.getTrackedAffects().map((t) => t.name)).toEqual(['sanctuary', 'haste']);
    fire('vosh://tracked-affects-changed', ['fly']);
    expect(s.tracked.getTrackedAffects()).toEqual([{ name: 'fly', label: null }]);
    commands.set('ui_get_config', { tracked_affects: [{ name: 'armor', label: 'AC' }] });
    fire('vosh://profile-switched', 'Tolliver');
    await settle();
    expect(s.tracked.getTrackedAffects()).toEqual([{ name: 'armor', label: 'AC' }]);
  });

  it('seed the target and let a broadcast win', async () => {
    const s = await load();
    expect(s.target.getTargetState().name).toBe('guard');
    fire('session://target', { name: 'rat', room_idx: null, quick_keys: [] });
    expect(s.target.getTargetState().name).toBe('rat');
    disconnect();
    expect(s.target.getTargetState()).toEqual({ name: null, room_idx: null, quick_keys: [] });
  });

  it('resolve the room area once Map.Tiles names it', async () => {
    const s = await load();
    gmcp('Room.Info', { num: 5279, name: 'The Bank', area: 'Blackwatch Village' });
    expect(s.room.getRoom().info?.areaColor).toBeNull();
    gmcp('Map.Tiles', {
      g: [[{ h: 1, ar: 52 }]],
      areas: { '52': { name: 'Blackwatch Village', color: '#a3be8c' } },
    });
    expect(s.room.getRoom().info).toMatchObject({ areaVnum: 52, areaColor: '#a3be8c' });
  });

  it('keep the name, terrain and region of the room you last saw together', async () => {
    const s = await load();
    packet('room-info.gmcp');
    const bank = {
      name: 'The Bank of Aabahran',
      sector: 0,
      terrain: 'inside',
      region: 'Temperate',
    };
    expect(s.room.getRoom().info).toMatchObject(bank);
    // A dark room sends no Room.Info, while Room.Weather still comes
    // with each prompt. The room keeps what its own packet said.
    packet('room-weather.gmcp');
    expect(s.room.getRoom().info).toMatchObject(bank);
    disconnect();
    expect(s.room.getRoom().info).toMatchObject(bank);
    // Under rhapsody of delusion the game sends a room with no sector
    // and no region.
    connect();
    packet('room-info-rhapsody.gmcp');
    expect(s.room.getRoom().info).toMatchObject({
      name: 'A Wondrous Place',
      sector: null,
      terrain: 'unknown',
      region: null,
    });
  });

  it('count up from the last tick, warn at your threshold, and hide when reports stop', async () => {
    const s = await load();
    vi.useFakeTimers();
    const tick = (elapsed_ms: number, synced = false) =>
      fire('session://tick', {
        enabled: true,
        interval_ms: 30_000,
        remaining_ms: Math.max(0, 30_000 - elapsed_ms),
        elapsed_ms,
        overdue: elapsed_ms >= 30_000,
        synced,
        fired: false,
        sound: false,
      });
    tick(16_000);
    expect(s.tick.getTick()).toEqual({
      active: true,
      secsSinceTick: 16,
      secsLeft: 14,
      intervalSecs: 30,
      warnAt: 8,
      warn: false,
      overdue: false,
      synced: false,
    });
    tick(22_500);
    expect(s.tick.getTick().warn).toBe(true);
    // The game runs late. The count goes on past the interval.
    tick(33_000, true);
    expect(s.tick.getTick()).toMatchObject({
      secsSinceTick: 33,
      secsLeft: -3,
      warn: true,
      overdue: true,
      synced: true,
    });
    vi.advanceTimersByTime(2_000);
    expect(s.tick.getTick().active).toBe(false);
  });

  it('show the running tick when the profile at launch saved it off', async () => {
    commands.set('tick_get_config', {
      ...(commands.get('tick_get_config') as object),
      enabled: false,
    });
    const s = await load();
    fire('session://tick', {
      enabled: true,
      interval_ms: 30_000,
      remaining_ms: 18_000,
      elapsed_ms: 12_000,
      overdue: false,
      synced: false,
      fired: false,
      sound: false,
    });
    expect(s.tick.getTick()).toMatchObject({ active: true, secsSinceTick: 12, warnAt: 8 });
  });

  it('play the tick sound once when the tick lands with the sound on', async () => {
    await load();
    const report = (fired: boolean, sound: boolean) =>
      fire('session://tick', {
        enabled: true,
        interval_ms: 30_000,
        remaining_ms: 30_000,
        elapsed_ms: 0,
        overdue: false,
        synced: true,
        fired,
        sound,
      });
    report(false, true);
    expect(playTickSound).not.toHaveBeenCalled();
    report(true, false);
    expect(playTickSound).not.toHaveBeenCalled();
    report(true, true);
    expect(playTickSound).toHaveBeenCalledTimes(1);
  });

  it('hide the tick at once when Settings turns it off', async () => {
    const s = await load();
    fire('session://tick', {
      enabled: true,
      interval_ms: 30_000,
      remaining_ms: 18_000,
      elapsed_ms: 12_000,
      overdue: false,
      synced: false,
      fired: false,
      sound: false,
    });
    expect(s.tick.getTick().active).toBe(true);
    fire('vosh://tick-config-changed', {
      ...(commands.get('tick_get_config') as object),
      enabled: false,
    });
    expect(s.tick.getTick().active).toBe(false);
  });

  it('follow the tick and time style Settings saves and each profile keeps', async () => {
    commands.set('ui_get_config', { tracked_affects: [], chip_style: 'caption_value' });
    const s = await load();
    expect(s.chipStyle.getChipStyle()).toBe('caption_value');
    fire('vosh://chip-style-changed', 'icon_value');
    expect(s.chipStyle.getChipStyle()).toBe('icon_value');
    fire('vosh://chip-style-changed', 'sparkles');
    expect(s.chipStyle.getChipStyle()).toBe('value_only');
    commands.set('ui_get_config', { tracked_affects: [], chip_style: 'icon_value' });
    fire('vosh://profile-switched', 'Ilsabet');
    await settle();
    expect(s.chipStyle.getChipStyle()).toBe('icon_value');
  });

  it('follow the echo mark Settings saves and each profile keeps', async () => {
    commands.set('ui_get_config', {
      tracked_affects: [],
      input_echo_mark: 'own',
      input_echo_mark_text: 'you:',
    });
    const s = await load();
    expect(s.echoMark.getEchoMarkOptions()).toEqual({
      mark: 'own',
      text: 'you:',
      color: null,
      dim: false,
    });
    const sent = { mark: 'gt', text: 'you:', color: '#c6a46a', dim: true };
    fire('vosh://input-echo-mark-changed', sent);
    const heard = s.echoMark.getEchoMarkOptions();
    expect(heard).toEqual(sent);
    // The same options again keep the snapshot.
    fire('vosh://input-echo-mark-changed', { ...sent });
    expect(s.echoMark.getEchoMarkOptions()).toBe(heard);
    fire('vosh://input-echo-mark-changed', { mark: 'caret' });
    expect(s.echoMark.getEchoMarkOptions()).toEqual({
      mark: 'chevron',
      text: '',
      color: null,
      dim: false,
    });
    commands.set('ui_get_config', { tracked_affects: [], input_echo_mark: 'off' });
    fire('vosh://profile-switched', 'Orla');
    await settle();
    expect(s.echoMark.getEchoMarkOptions().mark).toBe('off');
  });

  it('follow the tick count Settings saves and each profile keeps', async () => {
    commands.set('ui_get_config', { tracked_affects: [], tick_count: 'down' });
    const s = await load();
    expect(s.tickCount.getTickCount()).toBe('down');
    fire('vosh://tick-count-changed', 'down_past_zero');
    expect(s.tickCount.getTickCount()).toBe('down_past_zero');
    fire('vosh://tick-count-changed', 'backwards');
    expect(s.tickCount.getTickCount()).toBe('up');
    commands.set('ui_get_config', { tracked_affects: [] });
    fire('vosh://tick-count-changed', 'down');
    fire('vosh://profile-switched', 'Ilsabet');
    await settle();
    // A profile saved before the setting counts up.
    expect(s.tickCount.getTickCount()).toBe('up');
  });

  it('keep a tick count Settings saved over a slower config read', async () => {
    let answer: (value: unknown) => void = () => undefined;
    commands.set('ui_get_config', { tracked_affects: [], tick_count: 'down' });
    const s = await load();
    // A profile switch starts a read that lands late.
    commands.set('ui_get_config', new Promise((resolve) => (answer = resolve)));
    fire('vosh://profile-switched', 'Ilsabet');
    fire('vosh://tick-count-changed', 'down_past_zero');
    answer({ tracked_affects: [], tick_count: 'up' });
    await settle();
    expect(s.tickCount.getTickCount()).toBe('down_past_zero');
  });

  it('follow the game time clock Settings saves and each profile keeps', async () => {
    commands.set('ui_get_config', { tracked_affects: [], game_time: '12h' });
    const s = await load();
    expect(s.gameTime.getGameTime()).toBe('12h');
    fire('vosh://game-time-changed', '24h');
    expect(s.gameTime.getGameTime()).toBe('24h');
    fire('vosh://game-time-changed', '12h');
    expect(s.gameTime.getGameTime()).toBe('12h');
    fire('vosh://game-time-changed', 'sundial');
    expect(s.gameTime.getGameTime()).toBe('24h');
    commands.set('ui_get_config', { tracked_affects: [] });
    fire('vosh://game-time-changed', '12h');
    fire('vosh://profile-switched', 'Maren');
    await settle();
    // A profile saved before the setting reads the 24 hour clock.
    expect(s.gameTime.getGameTime()).toBe('24h');
  });

  it('keep a game time clock Settings saved over a slower config read', async () => {
    let answer: (value: unknown) => void = () => undefined;
    commands.set('ui_get_config', { tracked_affects: [] });
    const s = await load();
    commands.set('ui_get_config', new Promise((resolve) => (answer = resolve)));
    fire('vosh://profile-switched', 'Maren');
    fire('vosh://game-time-changed', '12h');
    answer({ tracked_affects: [], game_time: '24h' });
    await settle();
    expect(s.gameTime.getGameTime()).toBe('12h');
  });

  it('seed the affect fulls, follow each change, and clear them on a disconnect', async () => {
    commands.set('affect_full_get', { armor: 48, sanctuary: 10 });
    const s = await load();
    expect(s.affectFull.getAffectFull()).toEqual({ armor: 48, sanctuary: 10 });
    fulls({ armor: 48, sanctuary: 10, fly: 53, bad: 'x' });
    const heard = s.affectFull.getAffectFull();
    expect(heard).toEqual({ armor: 48, sanctuary: 10, fly: 53 });
    // The same map again keeps the snapshot, so nothing renders.
    fulls({ fly: 53, armor: 48, sanctuary: 10 });
    expect(s.affectFull.getAffectFull()).toBe(heard);
    disconnect();
    expect(s.affectFull.getAffectFull()).toEqual({});
  });

  it('keep an affect full change over a slower first read', async () => {
    let answer: (value: unknown) => void = () => undefined;
    commands.set('affect_full_get', new Promise((resolve) => (answer = resolve)));
    const s = await load();
    fulls({ armor: 40 });
    answer({ armor: 48 });
    await settle();
    expect(s.affectFull.getAffectFull()).toEqual({ armor: 40 });
  });

  it('follow the affects display Settings or the pane menu saves and each profile keeps', async () => {
    commands.set('ui_get_config', {
      tracked_affects: [],
      affects_style: 'countdown',
      affects_marker: 'square',
    });
    const s = await load();
    // The hours read 2 and 1 for a profile that never set them.
    const hours = { running_out: 2, almost_gone: 1 };
    expect(s.affectsDisplay.getAffectsDisplay()).toEqual({
      style: 'countdown',
      marker: 'square',
      tint: false,
      ...hours,
    });
    fire('vosh://affects-display-changed', {
      style: 'chips',
      marker: 'none',
      tint: true,
      ...hours,
    });
    const heard = s.affectsDisplay.getAffectsDisplay();
    expect(heard).toEqual({ style: 'chips', marker: 'none', tint: true, ...hours });
    // The same display again keeps the snapshot, so nothing renders.
    fire('vosh://affects-display-changed', {
      style: 'chips',
      marker: 'none',
      tint: true,
      ...hours,
    });
    expect(s.affectsDisplay.getAffectsDisplay()).toBe(heard);
    // A new threshold alone renders again.
    fire('vosh://affects-display-changed', {
      style: 'chips',
      marker: 'none',
      tint: true,
      running_out: 5,
      almost_gone: 2,
    });
    expect(s.affectsDisplay.getAffectsDisplay()).toEqual({
      style: 'chips',
      marker: 'none',
      tint: true,
      running_out: 5,
      almost_gone: 2,
    });
    fire('vosh://affects-display-changed', { style: 'grid', marker: 'check' });
    expect(s.affectsDisplay.getAffectsDisplay()).toEqual({
      style: 'timers',
      marker: 'dot',
      tint: false,
      ...hours,
    });
    commands.set('ui_get_config', {
      tracked_affects: [],
      affects_tint: true,
      affects_running_out_hours: 4,
      affects_almost_gone_hours: 0,
    });
    fire('vosh://profile-switched', 'Ilsabet');
    await settle();
    expect(s.affectsDisplay.getAffectsDisplay()).toEqual({
      style: 'timers',
      marker: 'dot',
      tint: true,
      running_out: 4,
      almost_gone: 0,
    });
  });

  it('follow the chat colors the pane menu picks and each profile keeps', async () => {
    commands.set('ui_get_chat_colors', { say: 'brightBlue' });
    const s = await load();
    const entries = () => [...s.chatColors.getChatColors().entries()];
    expect(entries()).toEqual([['say', 'brightBlue']]);
    fire('vosh://chat-colors-changed', { say: 'brightBlue', tell: 'red' });
    const heard = s.chatColors.getChatColors();
    expect(entries()).toEqual([
      ['say', 'brightBlue'],
      ['tell', 'red'],
    ]);
    // The same table again keeps the snapshot, so nothing renders.
    fire('vosh://chat-colors-changed', { tell: 'red', say: 'brightBlue' });
    expect(s.chatColors.getChatColors()).toBe(heard);
    fire('vosh://chat-colors-changed', {});
    expect(entries()).toEqual([]);
    commands.set('ui_get_chat_colors', { gtell: 'cyan' });
    fire('vosh://profile-switched', 'Ilsabet');
    await settle();
    expect(entries()).toEqual([['gtell', 'cyan']]);
  });

  it('keep the chat colors a pick sent over a slower read', async () => {
    let answer: (value: unknown) => void = () => undefined;
    commands.set('ui_get_chat_colors', {});
    const s = await load();
    commands.set('ui_get_chat_colors', new Promise((resolve) => (answer = resolve)));
    fire('vosh://profile-switched', 'Ilsabet');
    fire('vosh://chat-colors-changed', { say: 'red' });
    answer({ say: 'blue' });
    await settle();
    expect(s.chatColors.getChatColors().get('say')).toBe('red');
  });

  it('keep an affects display a pick sent over a slower config read', async () => {
    let answer: (value: unknown) => void = () => undefined;
    commands.set('ui_get_config', { tracked_affects: [], affects_style: 'countdown' });
    const s = await load();
    commands.set('ui_get_config', new Promise((resolve) => (answer = resolve)));
    fire('vosh://profile-switched', 'Ilsabet');
    fire('vosh://affects-display-changed', { style: 'chips', marker: 'dot', tint: false });
    answer({ tracked_affects: [], affects_style: 'timers' });
    await settle();
    expect(s.affectsDisplay.getAffectsDisplay().style).toBe('chips');
  });

  it('follow the vitals options Settings saves and each profile keeps', async () => {
    commands.set('ui_get_config', {
      tracked_affects: [],
      vitals_density: 'line',
      vitals_values: 'percent',
      vitals_meter: 'bar',
    });
    const s = await load();
    expect(s.vitalsOptions.getVitalsOptions()).toEqual({
      style: 'line',
      place: 'panel',
      order: ['hp', 'mana', 'move'],
      off: [],
      opponent: 'top',
      colors: {},
      values: 'percent',
      meter: 'bar',
      warn_thirds: false,
      hide_when_pinned: true,
      hit: false,
    });
    const sent = {
      style: 'gauges',
      place: 'status',
      order: ['move', 'hp', 'mana'],
      off: ['mana'],
      opponent: 'bottom',
      colors: { mana: 12 },
      values: 'current',
      meter: 'none',
      warn_thirds: true,
      hide_when_pinned: false,
      hit: true,
    };
    fire('vosh://vitals-options-changed', sent);
    const heard = s.vitalsOptions.getVitalsOptions();
    expect(heard).toEqual(sent);
    // The same options again keep the snapshot, so nothing renders.
    fire('vosh://vitals-options-changed', structuredClone(sent));
    expect(s.vitalsOptions.getVitalsOptions()).toBe(heard);
    // A new color is a new snapshot.
    fire('vosh://vitals-options-changed', { ...sent, colors: { mana: 4 } });
    expect(s.vitalsOptions.getVitalsOptions().colors).toEqual({ mana: 4 });
    commands.set('ui_get_config', {
      tracked_affects: [],
      vitals_style: 'pips',
      vitals_warn_thirds: true,
    });
    fire('vosh://profile-switched', 'Ilsabet');
    await settle();
    expect(s.vitalsOptions.getVitalsOptions()).toEqual({
      style: 'pips',
      place: 'panel',
      order: ['hp', 'mana', 'move'],
      off: [],
      opponent: 'top',
      colors: {},
      values: 'current-max',
      meter: 'line',
      warn_thirds: true,
      hide_when_pinned: true,
      hit: false,
    });
  });
});
