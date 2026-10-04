import { describe, expect, it } from 'vitest';
import { buildSections, groupOfSectionKey, type ListEntry } from './automationList';
import { loadoutHoldNote, switchesByName, withSwitch } from './groupSwitches';

describe('the group switches', () => {
  it('reads the reply groups_list sends', () => {
    const byName = switchesByName([
      { name: 'combat', enabled: true, loadouts: { on: true, by: ['Healer'] } },
      { name: 'loot', enabled: false },
      { name: '', enabled: true },
      'junk',
    ]);
    expect([...byName.keys()]).toEqual(['combat', 'loot']);
    expect(byName.get('combat')).toEqual({
      name: 'combat',
      enabled: true,
      loadouts: { on: true, by: ['Healer'] },
    });
    expect(byName.get('loot')).toEqual({ name: 'loot', enabled: false });
    expect(switchesByName(undefined).size).toBe(0);
  });

  it('turns one switch and leaves the map alone when nothing changes', () => {
    const byName = switchesByName([{ name: 'loot', enabled: true }]);
    const off = withSwitch(byName, 'loot', false);
    expect(off.get('loot')?.enabled).toBe(false);
    expect(byName.get('loot')?.enabled).toBe(true);
    expect(withSwitch(byName, 'loot', true)).toBe(byName);
    expect(withSwitch(byName, 'missing', false)).toBe(byName);
  });

  it('finds the group of a section by its key, and none for the rest', () => {
    const entry = (uid: string, group: string, preset = false): ListEntry => ({
      uid,
      name: uid,
      group,
      enabled: true,
      text: uid,
      preset,
    });
    const sections = buildSections([
      entry('a', ''),
      entry('b', 'combat'),
      entry('c', '', true),
      entry('d', 'From presets'),
    ]);
    // A group you named From presets is a group, and the presets
    // section after it is not.
    expect(sections.map((s) => groupOfSectionKey(s.key))).toEqual([
      null,
      'combat',
      'From presets',
      null,
    ]);
  });

  it('names the loadouts that decide a group', () => {
    expect(loadoutHoldNote({ on: true, by: ['Healer'] })).toBe(
      'The Healer loadout turns this group on.',
    );
    expect(loadoutHoldNote({ on: true, by: ['Healer', 'Warrior'] })).toBe(
      'The Healer and Warrior loadouts turn this group on.',
    );
    expect(loadoutHoldNote({ on: false, by: ['Healer'] })).toBe(
      'The Healer loadout leaves this group off.',
    );
    expect(loadoutHoldNote({ on: false, by: ['Healer', 'Warrior', 'Scout'] })).toBe(
      'The Healer, Warrior, and Scout loadouts leave this group off.',
    );
    expect(loadoutHoldNote({ on: false, by: [] })).toBe(
      'Every loadout is off, so this group stays off.',
    );
  });
});
