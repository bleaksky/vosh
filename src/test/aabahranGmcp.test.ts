import { describe, expect, it } from 'vitest';
import { aabahranFixtureNames, aabahranPacket, splitGmcp } from './aabahranGmcp';

// The package each fixture's file name starts with.
const PACKAGES: Record<string, string> = {
  'char-vitals': 'Char.Vitals',
  'char-affects': 'Char.Affects',
  'char-combat': 'Char.Combat',
  'char-prompt': 'Char.Prompt',
  'char-state': 'Char.State',
  'room-weather': 'Room.Weather',
  'room-info': 'Room.Info',
  'group-info': 'Group.Info',
  'snoop-start': 'Snoop.Start',
  'snoop-output': 'Snoop.Output',
  'snoop-stop': 'Snoop.Stop',
};

describe('the Aabahran GMCP fixtures', () => {
  it('split into the package each file names and a JSON object', () => {
    const names = aabahranFixtureNames();
    expect(names.length).toBeGreaterThanOrEqual(27);
    for (const name of names) {
      const prefix = Object.keys(PACKAGES).find((p) => name.startsWith(p));
      expect(prefix, name).toBeDefined();
      const packet = aabahranPacket(name);
      expect(packet.package, name).toBe(PACKAGES[prefix as string]);
      expect(packet.data, name).toBeTypeOf('object');
      expect(packet.data, name).not.toBeNull();
    }
  });

  it('split a payload the way the backend does', () => {
    expect(splitGmcp('Group.Info {"hidden":true}\n')).toEqual({
      package: 'Group.Info',
      data: { hidden: true },
    });
    expect(splitGmcp('Core.Ping')).toEqual({ package: 'Core.Ping', data: null });
  });

  it('keep the prompt text raw', () => {
    expect(aabahranPacket('char-prompt.gmcp').data).toEqual({
      enabled: true,
      prompt: '%n%P%C<%hhp %mm %vmv> ',
      fprompt: '',
    });
  });
});
