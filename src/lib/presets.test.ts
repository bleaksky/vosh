import { describe, expect, it } from 'vitest';
import roomLines from '../../fixtures/room-colors/lines.json';
import roomPreset from '../../fixtures/room-colors/preset.json';
import { PRESETS_OFF_MARKER } from './automationRecords';
import { parseRoutedLine } from './chatStore';
import {
  defaultEnabledIds,
  PRESET_CATEGORIES,
  PRESETS,
  presetById,
  presetTriggers,
} from './presets';

// The preset that puts the tells you send in the chat pane, run against
// every line the game prints when you talk to one person or your group
// (languages.c compose_tell and compose_grouptell_open). The patterns
// are Rust regex, and these use only what JavaScript reads the same way.
describe('the Tells you send preset', () => {
  const preset = presetById('sent_tells');
  const triggers = preset ? presetTriggers(preset) : [];
  const patterns = triggers.flatMap((t) => t.patterns.map((p) => new RegExp(p.pattern)));
  const panes = triggers.flatMap((t) =>
    t.actions.flatMap((a) => (a.kind === 'route' ? [a.pane] : [])),
  );
  const matches = (text: string) => patterns.some((p) => p.test(text));
  const caught = (text: string) =>
    matches(text) ? parseRoutedLine({ pane: panes[0] ?? '', text }) : undefined;

  it('is on from the start, under Chat', () => {
    expect(preset?.name).toBe('Tells you send');
    expect(defaultEnabledIds()).toContain('sent_tells');
    expect(preset && PRESET_CATEGORIES[preset.category]).toBe('Chat');
    expect(triggers.length).toBeGreaterThan(0);
    expect(triggers.every((t) => t.preset === 'sent_tells')).toBe(true);
  });

  it('routes to the tell channel and does nothing else', () => {
    expect(panes).toEqual(['tell']);
    expect(triggers.flatMap((t) => t.actions.map((a) => a.kind))).toEqual(['route']);
  });

  it('catches a tell you speak or project, in any language, as one you sent', () => {
    const lines: [string, string, string, string][] = [
      ["You tell Tolliver 'omw'", 'Tolliver', 'omw', 'common'],
      ["You tell a city guard in Tol'khan 'it is me'", 'a city guard', 'it is me', "Tol'khan"],
      ["You project to Tolliver 'omw'", 'Tolliver', 'omw', 'common'],
      ["You project to Tolliver in Elvish 'omw'", 'Tolliver', 'omw', 'Elvish'],
    ];
    for (const [text, speaker, message, language] of lines) {
      expect(caught(text), text).toMatchObject({
        pane: 'tell',
        direction: 'sent',
        speaker,
        text: message,
        language,
      });
    }
  });

  it('puts no line for a group tell in the pane', () => {
    for (const text of [
      "You tell your group 'one tick, waiting on mana'",
      "You tell your group in Elvish 'one tick'",
      "You broadcast 'one tick'",
    ]) {
      expect(caught(text) ?? null, text).toBeNull();
    }
  });

  it('leaves the tells you receive to their packet', () => {
    for (const text of [
      "Tolliver tells you 'are you still at the bank?'",
      "[Tolliver] 'omw'",
      'You project your image away from your body.',
    ]) {
      expect(matches(text), text).toBe(false);
    }
  });
});

// The preset that colors a room look and the clock as the redesign
// mockups do, and each change in the weather in a blue of its own. Every
// line comes from fixtures/room-colors/lines.json, game text built from
// the server's own format strings and area files, and its README says
// where each one comes from. The patterns are Rust regex and use only
// what JavaScript reads the same way. The Rust tests run the same
// triggers, from preset.json, through the session's own steps.
describe('the Room, time and weather colors preset', () => {
  const preset = presetById('room_and_time');
  const triggers = preset ? presetTriggers(preset) : [];
  const named = (name: string) => {
    const found = triggers.find((t) => t.name === name);
    if (!found) throw new Error(`no trigger ${name}`);
    return found;
  };
  const lineTriggers = triggers.filter((t) => t.target === undefined);
  // eslint-disable-next-line no-control-regex
  const plain = (line: string) => line.replace(/\x1b\[[0-9;]*m/g, '');
  // What a trigger's first matching pattern covers on `text`, or null.
  const span = (trigger: (typeof triggers)[number], text: string): string | null => {
    for (const row of trigger.patterns) {
      const m = new RegExp(row.pattern).exec(text);
      if (m) return m[0];
    }
    return null;
  };

  it('is on from the start, under Rooms, time and weather', () => {
    expect(preset?.name).toBe('Room, time and weather colors');
    expect(preset?.description).toBe(
      'Colors the exits green, what is in the room yellow, your target in the room bright red, the time of day blue, a change in the weather pale blue, and the WiZNET tag magenta.',
    );
    expect(defaultEnabledIds()).toContain('room_and_time');
    expect(preset && PRESET_CATEGORIES[preset.category]).toBe('Rooms, time and weather');
    expect(triggers.every((t) => t.preset === 'room_and_time')).toBe(true);
  });

  it('holds the triggers the Rust tests run', () => {
    expect(triggers).toEqual(roomPreset.triggers);
  });

  it("draws in the theme's own green, yellow, bright red, blue and magenta, and the weather in #8fa7d9", () => {
    const styles = Object.fromEntries(
      triggers.map((t) => [
        t.name,
        t.actions.map((a) =>
          a.kind === 'highlight' ? a.style : a.kind === 'replace' ? a.template : a.kind,
        ),
      ]),
    );
    expect(styles).toEqual({
      'room.exits': [{ fg: 'green', base: true }],
      'room.contents': [{ fg: 'yellow', base: true }],
      'room.target': [{ fg: 'bright_red', base: true }],
      'time.of_day': [{ fg: 'blue' }],
      // The whole line in the true color 143 167 217, then a reset.
      'weather.change': ['\x1b[38;2;143;167;217m$0\x1b[0m'],
      'wiznet.tag': [{ fg: 'magenta', bold: true }],
    });
  });

  it('colors your target over the room color, on its line alone', () => {
    const target = named('room.target');
    expect(target.priority).toBeGreaterThan(named('room.contents').priority);
    expect(target.patterns).toEqual(named('room.contents').patterns);
  });

  it('colors what is in the room only through the Room target', () => {
    expect(named('room.contents').target).toBe('room');
    expect(named('room.target').target).toBe('room_target');
    expect(lineTriggers.map((t) => t.name)).toEqual([
      'room.exits',
      'time.of_day',
      'weather.change',
      'wiznet.tag',
    ]);
  });

  it('colors each line it names, over the text it names, and no other trigger touches it', () => {
    const colored = roomLines.lines.filter((l) => 'trigger' in l);
    expect(colored.length).toBeGreaterThan(0);
    for (const entry of colored) {
      if (!('trigger' in entry)) continue;
      const text = plain(entry.line);
      expect(span(named(entry.trigger), text), text).toBe(entry.match);
      for (const other of lineTriggers.filter((t) => t.name !== entry.trigger)) {
        expect(span(other, text), `${other.name} on ${text}`).toBeNull();
      }
    }
  });

  it('colors every time of day message the game sends', () => {
    const times = roomLines.lines.filter((l) => 'trigger' in l && l.trigger === 'time.of_day');
    expect(times).toHaveLength(11);
    expect(named('time.of_day').patterns).toHaveLength(11);
  });

  it('colors every change in the weather the game sends, the whole line', () => {
    const weather = roomLines.lines.filter((l) => 'trigger' in l && l.trigger === 'weather.change');
    // Fourteen from sky_event_text and ten from weather_affect_room.
    expect(weather).toHaveLength(24);
    expect(named('weather.change').patterns).toHaveLength(24);
    for (const entry of weather) {
      expect(span(named('weather.change'), plain(entry.line))).toBe(plain(entry.line));
    }
  });

  it('leaves each weather line to this preset alone', () => {
    const weather = roomLines.lines.filter((l) => 'trigger' in l && l.trigger === 'weather.change');
    const others = PRESETS.filter((p) => p.id !== 'room_and_time').flatMap(presetTriggers);
    expect(others.length).toBeGreaterThan(0);
    for (const entry of weather) {
      const text = plain(entry.line);
      for (const t of others) {
        expect(span(t, text), `${t.name} on ${text}`).toBeNull();
      }
    }
  });

  it('leaves a say, a tell, a channel, a prompt, a room name and the weather report alone', () => {
    const misses = roomLines.lines.filter((l) => !('trigger' in l));
    expect(misses.length).toBeGreaterThan(10);
    for (const entry of misses) {
      const text = plain(entry.line);
      for (const t of lineTriggers) {
        expect(span(t, text), `${t.name} on ${text}`).toBeNull();
      }
    }
  });
});

// The library lives only here, and the Rust side leans on what it holds.
// Launch takes the defaults as every preset there is (presets_on_in_any
// in loadouts/presets.rs), and Settings stores PRESETS_OFF_MARKER when you
// turn every preset off. Rust tests read this file as text for the rest,
// so every read between the two languages goes from Rust to the page.
describe('the library the Rust side leans on', () => {
  const ids = PRESETS.map((p) => p.id);

  it('turns every preset on by default', () => {
    expect(PRESETS.filter((p) => !p.defaultEnabled).map((p) => p.id)).toEqual([]);
    expect(defaultEnabledIds()).toEqual(ids);
  });

  it('gives each preset an id of its own that is never the off marker', () => {
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).not.toContain(PRESETS_OFF_MARKER);
  });
});

// What a preset's replace makes of a line, as the trigger engine does
// it: the first pattern that matches rewrites the line through its
// template, $1 to $9 filled from the groups. Colors stripped, so the test
// reads the words. Null when no pattern matches.
function rewritten(id: string, line: string): string | null {
  const preset = presetById(id);
  for (const trigger of preset ? presetTriggers(preset) : []) {
    for (const p of trigger.patterns) {
      const m = new RegExp(p.pattern).exec(line);
      if (!m) continue;
      for (const action of trigger.actions) {
        if (action.kind !== 'replace') continue;
        const out = action.template.replace(/\$(\d)/g, (_, n: string) => m[Number(n)] ?? '');
        // eslint-disable-next-line no-control-regex
        return out.replace(/\x1b\[[0-9;]*m/g, '');
      }
    }
  }
  return null;
}

// The level up, as gain_exp and advance_level print it in update.c: the
// level on one line, then what you gain on the next, with hit point and
// practice singular when one.
describe('the Gold, experience, and levels preset', () => {
  it('marks the line that says you raised a level', () => {
    expect(rewritten('loot_progression', 'You raise a level!!')).toBe('You raise a level!!');
  });

  it('marks what you gain and keeps the game words', () => {
    for (const line of [
      'You gain:  12/1032 hit points, 9/809 mana, 5/935 move, and 4 practices.',
      'You gain:  7/1027 hit points, 3/803 mana, 1/931 move, and 1 practice.',
    ]) {
      expect(rewritten('loot_progression', line), line).toBe(line);
    }
  });
});

// The bubbly potions do_brew makes from food in skills5.c, each named
// a bubbly <color> potion, and the spell each casts.
describe('the Potion labels preset', () => {
  it('names the spell of every bubbly potion the game brews', () => {
    const potions: [string, string][] = [
      ['red', 'cure blind'],
      ['green', 'haste'],
      ['crimson', 'frenzy'],
      ['white', 'sanctuary'],
      ['orange', 'fireball'],
      ['clear', 'invisibility'],
      ['pink', 'cure light'],
      ['brown', 'cure serious'],
      ['blue', 'armor'],
      ['grey', 'bless'],
    ];
    for (const [color, spell] of potions) {
      const name = `a bubbly ${color} potion`;
      expect(rewritten('potion_labels', name), color).toBe(`${name} (${spell})`);
    }
  });
});
