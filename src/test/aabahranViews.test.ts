import { describe, expect, it, vi } from 'vitest';
import views from '../../fixtures/gmcp/aabahran/views.json';
import { parseGroupInfo } from '../stores/gmcp/groupStore';
import { asNumber, asText } from '../stores/store';
import { parseAffectsPacket } from '../stores/gmcp/affectsStore';
import { parseCombat } from '../stores/gmcp/combatStore';
import { parseGamePrompt } from '../stores/gmcp/gamePromptStore';
import { parseVitalsPacket } from '../stores/gmcp/vitalsStore';
import { parseRoomInfo } from '../stores/gmcp/roomStore';
import { aabahranFixtureNames, aabahranPacket } from './aabahranGmcp';

// The stores' reading of each Aabahran packet against
// fixtures/gmcp/aabahran/views.json, which the prompt engine's tests in
// crates/prompt/tests/views.rs read too. The engine keeps its own
// copy of the packages to draw your prompt, and the panes read these
// stores, so both readings are held to one record. A package no store
// reads keeps its record for the engine alone (ENGINE_ONLY).
//
// A view's `map` holds what the Map pane's room strip reads where it
// differs from the prompt engine on purpose. Under rhapsody of delusion
// the game sends a made up room with all six exits and room 0 behind
// each. The Exits piece shows the six, as %e prints them, and the strip
// lists only exits with a room behind them. The strip colors a sector
// only when it has one, and the engine prints the game's -1.

// The stores reach the Tauri bridge when they start. The parsers under
// test never do.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

/** The stores' view of one packet, in the record's shape. */
function view(name: string): unknown {
  const { package: pkg, data } = aabahranPacket(name);
  switch (pkg) {
    case 'Char.Vitals': {
      const { values, hidden } = parseVitalsPacket(data);
      return { ...values, hidden };
    }
    case 'Char.Affects': {
      const { list, hidden } = parseAffectsPacket(data);
      return {
        affects: list.map((a) => ({ name: a.name, kind: a.kind, duration: a.duration })),
        hidden,
      };
    }
    case 'Char.Combat': {
      const fight = parseCombat(data);
      if (fight === null) return null;
      return {
        opponent: fight.name,
        hp_pct: fight.hp_pct,
        condition: fight.condition,
        hidden: fight.hidden,
        tank: fight.tank,
      };
    }
    case 'Group.Info': {
      const group = parseGroupInfo(data);
      return {
        leader: asText(group.leader) ?? null,
        members: (group.members ?? []).map((m) => ({
          id: asNumber(m.id),
          name: asText(m.name),
          level: asNumber(m.level),
          class: asText(m.class),
          hp_pct: asNumber(m.hp_pct),
          mana_pct: asNumber(m.mana_pct),
          move_pct: asNumber(m.move_pct),
          tnl: asNumber(m.tnl),
        })),
        hidden: group.hidden === true,
      };
    }
    case 'Char.Prompt':
      return parseGamePrompt(data);
    case 'Room.Info': {
      const info = parseRoomInfo(data);
      if (info === null) return null;
      return {
        name: info.name,
        num: info.vnum,
        area: info.area,
        terrain: info.terrain,
        sector: info.sector,
        region: info.region,
        exits: info.exits,
      };
    }
    default:
      throw new Error(`${name}: no view for ${pkg}`);
  }
}

const record = views as Record<string, unknown>;

/** Packages the prompt engine reads and no page store does yet. Their
 *  records stay for crates/prompt/tests/views.rs. A pane that shows one
 *  brings its store back and moves the package into view(). */
const ENGINE_ONLY = new Set(['Char.State', 'Room.Weather']);

/** Packages neither the engine nor a GMCP store reads, so they keep no
 *  record. The snoop store takes Snoop through its own events. */
const NO_VIEW = new Set(['Snoop.Start', 'Snoop.Output', 'Snoop.Stop']);

/** The record's view as the stores read it, with the Map pane's own
 *  readings laid over the engine's. */
function storeView(name: string): unknown {
  const want = record[name];
  if (!want || typeof want !== 'object' || !('map' in want)) return want;
  const { map, ...rest } = want as Record<string, unknown>;
  return { ...rest, ...(map as Record<string, unknown>) };
}

describe('the Aabahran packets', () => {
  it('read in the stores as the record says, the same as in the prompt engine', () => {
    const names = aabahranFixtureNames();
    for (const name of names) {
      if (NO_VIEW.has(aabahranPacket(name).package)) continue;
      expect(record, `views.json has no view of ${name}`).toHaveProperty([name]);
      if (ENGINE_ONLY.has(aabahranPacket(name).package)) continue;
      expect(view(name), name).toEqual(storeView(name));
    }
    // Every view names a fixture.
    for (const name of Object.keys(record)) expect(names).toContain(name);
  });
});
