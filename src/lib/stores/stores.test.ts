import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const gmcp = (pkg: string, payload: unknown) =>
  fire(`session://gmcp/${pkg.replace(/\./g, '-')}`, payload);
const disconnect = () => fire('session://state', { kind: 'disconnected', reason: null });

// Let the listen and invoke promises settle.
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

async function load() {
  const stores = await import('./index');
  stores.startStores();
  await settle();
  return {
    vitals: await import('./vitalsStore'),
    affects: await import('./affectsStore'),
    tracked: await import('./trackedAffectsStore'),
    combat: await import('./combatStore'),
    world: await import('./worldStore'),
    room: await import('./roomStore'),
    target: await import('./targetStore'),
    tick: await import('./tickStore'),
    chipStyle: await import('./chipStyleStore'),
  };
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  commands.clear();
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
    expect(s.combat.getCombat()).toEqual({ name: 'a guard', hp_pct: 12, condition: 'awful' });
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
    fire('session://prompt-vars', { hp: '150' });
    expect(s.vitals.getVitals()).toMatchObject({ hp: 150, maxhp: 1000, low: { hp: true } });
  });

  it('seed the tracked list and follow broadcasts and profile switches', async () => {
    const s = await load();
    expect(s.tracked.getTrackedAffects().map((t) => t.name)).toEqual(['sanctuary', 'haste']);
    fire('vosh://tracked-affects-changed', ['fly']);
    expect(s.tracked.getTrackedAffects()).toEqual([{ name: 'fly', label: null }]);
    commands.set('ui_get_config', { tracked_affects: [{ name: 'armor', label: 'AC' }] });
    fire('vosh://profile-switched', 'Selune');
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

  it('count up from the last tick, warn at your threshold, and hide when reports stop', async () => {
    const s = await load();
    vi.useFakeTimers();
    const tick = (remaining_ms: number) =>
      fire('session://tick', {
        enabled: true,
        interval_ms: 30_000,
        remaining_ms,
        fired: false,
        sound: false,
      });
    tick(14_000);
    expect(s.tick.getTick()).toEqual({ active: true, secsSinceTick: 16, warnAt: 8, warn: false });
    tick(7_500);
    expect(s.tick.getTick().warn).toBe(true);
    vi.advanceTimersByTime(2_000);
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
    fire('vosh://profile-switched', 'Erelei');
    await settle();
    expect(s.chipStyle.getChipStyle()).toBe('icon_value');
  });
});
