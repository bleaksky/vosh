import { describe, expect, it } from 'vitest';
import type { LuaLine } from '../../ipc/scripts';
import { errorMark, revealLabel, saveStatus, stopNote } from './pluginState';

// What a plugin's page says about the plugin, from its row and the
// Output ring.

const at = (h: number, m: number, s: number) => new Date(2026, 9, 4, h, m, s).getTime();

// Board 1's Output for vitals_alert and board 3's stop of wait_full.
const BOARD_ONE: LuaLine[] = [
  {
    ts_ms: at(21, 14, 3),
    owner: 'plugin:vitals_alert',
    kind: 'error',
    text: "vitals_alert/main.lua:22: attempt to concatenate a nil value (field 'hp_pct')",
    at: { source: 'vitals_alert/main.lua', line: 22 },
  },
  {
    ts_ms: at(21, 14, 31),
    owner: 'plugin:vitals_alert',
    kind: 'note',
    text: 'Vosh reloaded vitals_alert.',
  },
  {
    ts_ms: at(21, 15, 2),
    owner: 'plugin:vitals_alert',
    kind: 'input',
    text: 'print(mud.var("target"))',
  },
  {
    ts_ms: at(21, 15, 2),
    owner: 'plugin:vitals_alert',
    kind: 'print',
    text: 'a bank representative',
  },
];

const STOP = 'Vosh stopped wait_full at main.lua line 5 after 100 ms.';
const BOARD_THREE: LuaLine[] = [
  {
    ts_ms: at(21, 20, 11),
    owner: 'plugin:wait_full',
    kind: 'error',
    text: STOP,
    at: { source: 'wait_full/main.lua', line: 5 },
  },
  {
    ts_ms: at(21, 20, 11),
    owner: 'plugin:wait_full',
    kind: 'note',
    text: 'wait_full stays off until you save it under Scripts in Settings or restart Vosh.',
  },
];

describe('stopNote', () => {
  it('says why Vosh stopped the plugin and how long it stays off', () => {
    expect(stopNote('wait_full', 'time')).toBe(
      'Vosh stopped wait_full because one call ran past 100 ms. It stays off until you save it or restart Vosh.',
    );
    expect(stopNote('wait_full', 'call_memory')).toBe(
      'Vosh stopped wait_full because one call used more than 32 MB. It stays off until you save it or restart Vosh.',
    );
    expect(stopNote('wait_full', 'state_memory')).toBe(
      'Vosh stopped wait_full because your scripts held more than 128 MB. It stays off until you save it or restart Vosh.',
    );
  });
});

describe('errorMark', () => {
  it('marks the line a stop names, with its line of Output', () => {
    expect(errorMark(BOARD_THREE, 'wait_full', 'main.lua')).toEqual({ line: 5, message: STOP });
  });

  it('marks nothing once the plugin loaded again after its error', () => {
    expect(errorMark(BOARD_ONE, 'vitals_alert', 'main.lua')).toBeNull();
    // The same error after the reload marks its line.
    const again = [...BOARD_ONE, { ...BOARD_ONE[0], ts_ms: at(21, 16, 0) }];
    expect(errorMark(again, 'vitals_alert', 'main.lua')).toEqual({
      line: 22,
      message: BOARD_ONE[0].text,
    });
  });

  it('reads only the lines of its own plugin', () => {
    const lines = [...BOARD_THREE, ...BOARD_ONE];
    expect(errorMark(lines, 'wait_full', 'main.lua')).toEqual({ line: 5, message: STOP });
    expect(errorMark(BOARD_THREE, 'vitals_alert', 'main.lua')).toBeNull();
  });

  it('marks nothing when the newest error names another file or no place', () => {
    expect(errorMark(BOARD_THREE, 'wait_full', 'lib/stand.lua')).toBeNull();
    const memory: LuaLine = {
      ts_ms: at(21, 21, 0),
      owner: 'plugin:wait_full',
      kind: 'error',
      text: 'Vosh stopped wait_full. Your scripts hold more than 128 MB.',
    };
    expect(errorMark([...BOARD_THREE, memory], 'wait_full', 'main.lua')).toBeNull();
  });
});

describe('saveStatus', () => {
  it('says when you saved, and whether the plugin loaded again', () => {
    expect(saveStatus({ at: at(21, 14, 31), reloaded: true })).toBe('Reloaded at 21:14');
    expect(saveStatus({ at: at(9, 5, 0), reloaded: false })).toBe('Saved at 09:05');
  });
});

describe('revealLabel', () => {
  it('names the file manager of each platform', () => {
    expect(revealLabel('macos')).toBe('Show in Finder');
    expect(revealLabel('windows')).toBe('Show in Explorer');
    expect(revealLabel('linux')).toBe('Show the folder');
    expect(revealLabel(undefined)).toBe('Show the folder');
  });
});
