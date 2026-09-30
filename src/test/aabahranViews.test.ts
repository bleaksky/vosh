import { describe, expect, it, vi } from 'vitest';
import views from '../../fixtures/gmcp/aabahran/views.json';
import { parseGroupInfo } from '../lib/groupStore';
import { asNumber, asText } from '../lib/stores/store';
import { parseAffectsPacket } from '../lib/stores/affectsStore';
import { parseCharState } from '../lib/stores/charStateStore';
import { parseCombat } from '../lib/stores/combatStore';
import { parseGamePrompt } from '../lib/stores/gamePromptStore';
import { parseVitalsPacket } from '../lib/stores/vitalsStore';
import { parseRoomWeather } from '../lib/stores/weatherStore';
import { aabahranFixtureNames, aabahranPacket } from './aabahranGmcp';

// The stores' reading of each Aabahran packet against
// fixtures/gmcp/aabahran/views.json, which the prompt engine's tests in
// crates/prompt/tests/views.rs read too (D29). The engine keeps its own
// copy of the packages to draw your prompt, and the panes read these
// stores, so both readings are held to one record. Room.Info has no
// view, since the engine reads its fields through the resolver.

// The stores reach the Tauri bridge when they start. The parsers under
// test never do.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const NO_VIEW = ['room-info.gmcp', 'room-info-rhapsody.gmcp'];

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
    case 'Char.State':
      return parseCharState(data);
    case 'Room.Weather':
      return parseRoomWeather(data);
    case 'Char.Prompt':
      return parseGamePrompt(data);
    default:
      throw new Error(`${name}: no view for ${pkg}`);
  }
}

const record = views as Record<string, unknown>;

describe('the Aabahran packets', () => {
  it('read in the stores as the record says, the same as in the prompt engine', () => {
    const names = aabahranFixtureNames();
    for (const name of names) {
      if (NO_VIEW.includes(name)) {
        expect(record, name).not.toHaveProperty([name]);
        continue;
      }
      expect(record, `views.json has no view of ${name}`).toHaveProperty([name]);
      expect(view(name), name).toEqual(record[name]);
    }
    // Every view names a fixture.
    for (const name of Object.keys(record)) expect(names).toContain(name);
  });
});
