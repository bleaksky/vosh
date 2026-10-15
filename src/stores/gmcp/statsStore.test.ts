import { describe, expect, it, vi } from 'vitest';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { parseStats } from './statsStore';

// The store reaches the Tauri bridge when it starts. The parser never
// does.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

describe('parseStats', () => {
  it('reads the score sheet the game sends', () => {
    const stats = parseStats(aabahranPacket('char-stats.gmcp').data);
    expect(stats?.stats.str).toEqual({ cur: 22, perm: 20, max: 23, over: 1 });
    expect(stats?.stats.wis).toEqual({ cur: 15, perm: 17, max: 19, over: 0 });
    expect(stats?.luck).toEqual({ bar: 4, of: 6 });
    expect(stats?.hitroll).toBe(31);
    expect(stats?.hitrollPct).toBe(90);
    expect(stats?.damroll).toBe(27);
    expect(stats?.armor.blunt).toEqual({ ac: -280, pct: 74 });
    expect(stats?.saves.breath).toBe(2);
  });

  it('leaves out what the packet leaves out, and reads nothing from nothing', () => {
    const stats = parseStats({ str: { cur: 18, max: 20 }, ac: { slash: -100 } });
    // A stat with no trained value reads as trained where it stands.
    expect(stats?.stats.str).toEqual({ cur: 18, perm: 18, max: 20, over: 0 });
    expect(stats?.stats.int).toBeNull();
    expect(stats?.luck).toBeNull();
    expect(stats?.hitroll).toBeNull();
    expect(stats?.armor.slash).toEqual({ ac: -100, pct: null });
    expect(stats?.armor.magic).toBeNull();
    expect(stats?.saves.spell).toBeNull();
    expect(parseStats(null)).toBeNull();
    expect(parseStats([1, 2])).toBeNull();
    expect(parseStats('Char.Stats')).toBeNull();
  });

  it('keeps the luck bar inside its steps', () => {
    expect(parseStats({ luck: { bar: 9, of: 6 } })?.luck).toEqual({ bar: 6, of: 6 });
    expect(parseStats({ luck: { bar: -2, of: 6 } })?.luck).toEqual({ bar: 0, of: 6 });
    expect(parseStats({ luck: { bar: 2, of: 0 } })?.luck).toBeNull();
  });
});
