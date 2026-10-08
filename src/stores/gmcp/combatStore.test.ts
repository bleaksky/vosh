import { describe, expect, it } from 'vitest';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { nextFight, parseCombat, type CombatOpponent } from './combatStore';

const fixture = (name: string) => parseCombat(aabahranPacket(name).data);

describe('parseCombat', () => {
  it('reads the Aabahran payload', () => {
    expect(
      parseCombat({
        target: 'a member of the Blackwatch Guard',
        condition: 'big nasty wounds',
        hp_pct: 38,
      }),
    ).toEqual({
      name: 'a member of the Blackwatch Guard',
      hp_pct: 38,
      condition: 'big nasty wounds',
      hidden: false,
      tank: null,
    });
    expect(fixture('char-combat.gmcp')).toEqual({
      name: 'a Blackwatch guard',
      hp_pct: 54,
      condition: 'quite a few wounds',
      hidden: false,
      tank: null,
    });
  });

  it('reads the empty payload as no fight', () => {
    expect(parseCombat({})).toBeNull();
    expect(parseCombat({ target: '  ' })).toBeNull();
    expect(parseCombat(null)).toBeNull();
    expect(fixture('char-combat-end.gmcp')).toBeNull();
  });

  it('clamps health and tolerates missing fields', () => {
    expect(parseCombat({ target: 'rat', hp_pct: '-4' })).toEqual({
      name: 'rat',
      hp_pct: 0,
      condition: null,
      hidden: false,
      tank: null,
    });
    expect(parseCombat({ target: 'rat', hp_pct: 140.6 })?.hp_pct).toBe(100);
    expect(parseCombat({ target: 'rat' })?.hp_pct).toBeNull();
  });

  it('reads withheld health as hidden and keeps the name', () => {
    expect(fixture('char-combat-hidden.gmcp')).toEqual({
      name: 'a Blackwatch guard',
      hp_pct: null,
      condition: null,
      hidden: true,
      tank: null,
    });
    // A hidden packet never shows a number, even one it carries.
    expect(parseCombat({ target: 'rat', hp_pct: 40, condition: 'awful', hidden: true })).toEqual({
      name: 'rat',
      hp_pct: null,
      condition: null,
      hidden: true,
      tank: null,
    });
  });

  it('reads the groupmate your opponent hits', () => {
    expect(fixture('char-combat-tank.gmcp')?.tank).toEqual({ name: 'Tester', hp_pct: 78 });
    expect(fixture('char-combat-tank-hidden.gmcp')).toEqual({
      name: 'a Blackwatch guard',
      hp_pct: null,
      condition: null,
      hidden: true,
      tank: { name: 'Tester', hp_pct: null },
    });
    expect(parseCombat({ target: 'rat', tank: { name: ' ' } })?.tank).toBeNull();
    expect(parseCombat({ target: 'rat', tank: 'Tester' })?.tank).toBeNull();
    expect(parseCombat({ target: 'rat', tank: { name: 'Tester', hp_pct: -12 } })?.tank).toEqual({
      name: 'Tester',
      hp_pct: 0,
    });
  });
});

describe('nextFight', () => {
  const guard = (hp_pct: number | null, hidden = false): CombatOpponent => ({
    name: 'a Blackwatch guard',
    hp_pct,
    condition: null,
    hidden,
    tank: null,
  });
  const yours = { hp: 905, maxhp: 1038, mana: 870, maxmana: 870, move: 402, maxmove: 521 };

  it('starts with your vitals and the first health, and adds each change', () => {
    const begun = nextFight(null, guard(100), yours);
    expect(begun).toEqual({ name: 'a Blackwatch guard', start: yours, healths: [100] });
    const same = nextFight(begun, guard(100), null);
    expect(same).toBe(begun);
    const hit = nextFight(nextFight(same, guard(61), null), guard(null, true), null);
    expect(hit).toEqual({ name: 'a Blackwatch guard', start: yours, healths: [100, 61] });
  });

  it('starts over for a new opponent and forgets the fight as it ends', () => {
    const begun = nextFight(null, guard(54), yours);
    const next = nextFight(begun, { ...guard(90), name: 'Orla' }, null);
    expect(next).toEqual({ name: 'Orla', start: null, healths: [90] });
    expect(nextFight(next, null, yours)).toBeNull();
  });
});
