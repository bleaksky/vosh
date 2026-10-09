import { describe, expect, it } from 'vitest';
import roomLines from '../../fixtures/room-colors/lines.json';
import roomPreset from '../../fixtures/room-colors/preset.json';
import shippedTriggers from '../../fixtures/presets/triggers.json?raw';
import { enabledPresetIds, PRESETS_OFF_MARKER } from './automationRecords';
import { parseRoutedLine } from '../stores/gmcp/chatStore';
import { HIGHLIGHT_COLORS } from './automationTriggers';
import { colorize } from './colorTokens';
import { KNOWN_WORLDS } from '../lib/knownWorlds';
import {
  defaultEnabledIds,
  PRESET_CATEGORIES,
  PRESETS,
  PRESETS_ON_BY_DEFAULT,
  presetById,
  presetMacros,
  presetTriggerNames,
  presetTriggers,
  type PresetTrigger,
} from './presets';
import { drawSample, quotedWords, sampleBars, sampleRunCss, type SampleRun } from './presetSample';
import { ANSI_SLOTS } from '../theme/baseAnsi';
import { contrast, parseHex } from '../theme/color';
import { findTheme } from '../theme/themes';

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

// The preset that colors a room look and the clock, and each change in
// the weather in a blue of its own. Every line comes from
// fixtures/room-colors/lines.json, game text built from the server's own
// format strings and area files, and its README says where each one
// comes from. The patterns are Rust regex and use only what JavaScript
// reads the same way. The Rust tests run the same triggers, from
// preset.json, through the session's own steps.
describe('the Room, time, and weather colors preset', () => {
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

  it('is on from the start, under Rooms, time, and weather', () => {
    expect(preset?.name).toBe('Room, time, and weather colors');
    expect(preset?.description).toBe(
      'Colors the exits green, what is in the room yellow, your target in the room bright red, the time of day blue, a change in the weather pale blue, and the WiZNET tag magenta.',
    );
    expect(defaultEnabledIds()).toContain('room_and_time');
    expect(preset && PRESET_CATEGORIES[preset.category]).toBe('Rooms, time, and weather');
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
    const others = PRESETS.filter((p) => p.id !== 'room_and_time').flatMap((p) =>
      presetTriggers(p),
    );
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
// Launch reads an empty list as the presets on by default
// (PRESETS_ON_BY_DEFAULT in loadouts/presets.rs mirrors the list), and
// Settings stores PRESETS_OFF_MARKER when you turn every preset off. Rust
// tests read this file as text for the rest, so every read between the
// two languages goes from Rust to the page.
describe('the library the Rust side leans on', () => {
  const ids = PRESETS.map((p) => p.id);

  it('keeps the eleven presets on by default that an empty list has always meant', () => {
    const eleven = [
      'healing_basics',
      'defensive_combat',
      'disarm_buff_fade',
      'terror_events',
      'combat_outgoing',
      'combat_incoming',
      'loot_progression',
      'potion_labels',
      'herb_labels',
      'sent_tells',
      'room_and_time',
    ];
    expect(PRESETS_ON_BY_DEFAULT).toEqual(eleven);
    // Each is in the library, so an empty list turns on all eleven.
    expect(defaultEnabledIds()).toEqual(eleven);
    expect(enabledPresetIds([])).toEqual(eleven);
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

// The trigger `name` of preset `id`. It throws when the preset has no such
// trigger, which fails the file as it loads, so a test of an open bug can
// never pass on a trigger that left.
function presetTrigger(id: string, name: string) {
  const found = presetById(id)?.triggers.find((t) => t.name === name);
  if (!found) throw new Error(`${id} has no trigger ${name}`);
  return found;
}

// Whether a pattern of `trigger` that is on matches `line`.
function triggerMatches(trigger: ReturnType<typeof presetTrigger>, line: string): boolean {
  return trigger.patterns.some((p) => p.enabled && new RegExp(p.pattern).test(line));
}

// The cure and heal lines as the game prints them: cure critical, cure
// serious, bless and refresh in magic.c, the herb in skills2.c, and
// poison wearing off in const.c.
describe('the Cures and heals preset', () => {
  it('lights each cure line the game prints, word for word', () => {
    const lines: [string, string][] = [
      ['cure.feel_lot_better', 'You feel a lot better!'],
      ['cure.feel_better', 'You feel better.'],
      ['cure.feel_much_better', 'You feel much better.'],
      ['cure.righteous', 'You feel righteous.'],
      ['cure.less_sick', 'You feel less sick.'],
      ['cure.less_tired', 'You feel less tired.'],
    ];
    expect(presetById('healing_basics')?.triggers.map((t) => t.name)).toEqual(
      lines.map(([name]) => name),
    );
    for (const [name, line] of lines) {
      expect(triggerMatches(presetTrigger('healing_basics', name), line)).toBe(true);
    }
  });
});

// The level up, as gain_exp and advance_level print it in update.c: the
// level on one line, then what you gain on the next, with hit point and
// practice singular when one.
describe('the Gold, experience, and levels preset', () => {
  const skillUp = presetTrigger('loot_progression', 'loot.skill_up');

  // Bug 16. check_improve in skills.c prints the percent you reach
  // after the skill, You have become better at %s! [%d%%], and the skill
  // trigger wants the line to end at the bang, so it misses every skill
  // and spell you improve. Only a song prints the line without the
  // percent (check_improve_song in song.c). Dodge is a row of skill_table
  // in const.c, and the 78 is the hp_pct of fixtures/gmcp/aabahran
  // group-info.gmcp.
  it('marks a skill you improve, as check_improve prints it (bug 16)', () => {
    expect(triggerMatches(skillUp, 'You have become better at dodge! [78%]')).toBe(true);
    // The percent stays, in the color of the line.
    expect(rewritten('loot_progression', 'You have become better at dodge! [78%]')).toBe(
      'You have become better at dodge! [78%]',
    );
  });

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

// The buffs that wear off in Disarms and fading buffs, as update.c sends
// the wear off lines of skill_table in const.c.
describe('the Disarms and fading buffs preset', () => {
  const sanctuary = presetTrigger('disarm_buff_fade', 'buff.sanctuary');

  // Bug 17. The sanctuary trigger wanted The white aura around a
  // character fades, a line the game never prints. When sanctuary leaves
  // someone else, the room reads the line the sanctuary row of skill_table
  // holds for others, The protective aura around $n fades., here with
  // Maren. The nearest line in words, The angry white aura around $n
  // fades., is holy vengeance wearing off (effect.c), not sanctuary.
  it('marks sanctuary fading from another character (bug 17)', () => {
    expect(triggerMatches(sanctuary, 'The protective aura around Maren fades.')).toBe(true);
    expect(rewritten('disarm_buff_fade', 'The protective aura around Maren fades.')).toBe(
      '## The protective aura around Maren fades.',
    );
    // Your own fade comes out once, the same.
    expect(rewritten('disarm_buff_fade', 'The protective aura around your body fades.')).toBe(
      '## The protective aura around your body fades.',
    );
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

// What each line of each preset's sample shows, in order, as runs of text
// and their colors. A preset that names a 256 color index paints that
// index on every theme, and one that names an ANSI color paints the
// theme's own.
const SAMPLES_DRAW: Record<string, SampleRun[][]> = {
  healing_basics: [
    [['You feel a lot better!', 'bright_green']],
    [['You feel less sick.', 'bright_green']],
  ],
  defensive_combat: [
    [["You parry a villager's attack.", '240']],
    [["You dodge a villager's attack.", '240']],
    [["You block a villager's attack with your shield.", '240']],
  ],
  disarm_buff_fade: [
    [
      ['##', 'bold red'],
      [' ', null],
      ['Maren disarms you and sends your PRIMARY weapon flying!', '178'],
    ],
    [
      ['##', 'bold red'],
      [' ', null],
      ['The protective aura around your body fades.', '178'],
    ],
  ],
  terror_events: [
    [['Filled with terror, your weapon slips through your slippery fingers.', 'bold bright_red']],
  ],
  combat_outgoing: [
    [
      ['You ', '253'],
      ['do UNSPEAKABLE things', '214'],
      [' to a villager!', '253'],
    ],
  ],
  combat_incoming: [
    [
      ["A villager's punch ", '244'],
      ['decimates', '210'],
      [' you!', '244'],
    ],
    [
      ["A villager's punch ", '244'],
      ['misses', '152'],
      [' you.', '244'],
    ],
  ],
  loot_progression: [
    [
      ['You receive ', '248'],
      ['1250 ', '230'],
      ['experience points.', '248'],
    ],
    [
      ['You have become better at ', '120'],
      ['dagger', '230'],
      ['!', '120'],
    ],
  ],
  potion_labels: [
    [
      ['You brew a bubbly pink potion ', null],
      ['(cure light)', '248'],
      [' from a large kettle!', null],
    ],
    [
      ['You quaff a bubbly pink potion ', null],
      ['(cure light)', '248'],
      ['.', null],
    ],
  ],
  herb_labels: [
    [
      ['You light some rosemary ', null],
      ['(protection)', '248'],
      [' and begin to smoke it.', null],
    ],
  ],
  sent_tells: [[["You tell Tolliver 'The day has begun.'", null]]],
  room_and_time: [
    [['[Exits: south]', 'green']],
    [['A villager is here, fighting Maren.', 'bright_red']],
    [['Maren is here, fighting a villager.', 'yellow']],
    [['The day has begun.', 'blue']],
    [['It starts to rain.', '#8fa7d9']],
  ],
};

// Every preset that adds triggers carries a sample, lines the game prints
// in its own words, which Get started and the Presets page will show. Each
// runs here through the preset's own triggers, so a sample cannot drift
// from what its preset paints. A preset that only binds macros changes no
// line, so it has no sample.
const TRIGGER_PRESETS = PRESETS.filter((p) => p.triggers.length > 0);

describe('the sample of every preset', () => {
  it('holds one to five lines as the game prints them, each naming a trigger of its preset', () => {
    for (const preset of TRIGGER_PRESETS) {
      expect(preset.sample.length, preset.id).toBeGreaterThanOrEqual(1);
      expect(preset.sample.length, preset.id).toBeLessThanOrEqual(5);
      const names = preset.triggers.map((t) => t.name);
      for (const line of preset.sample) {
        expect(line.text.trim(), preset.id).toBe(line.text);
        expect(line.text, preset.id).not.toMatch(/\x1b/); // eslint-disable-line no-control-regex
        expect(names, `${preset.id} ${line.text}`).toContain(line.shows);
      }
    }
  });

  it('fires the trigger each line means to show, and paints the colors the preset gives it', () => {
    expect(Object.keys(SAMPLES_DRAW)).toEqual(TRIGGER_PRESETS.map((p) => p.id));
    for (const preset of TRIGGER_PRESETS) {
      const drawn = preset.sample.map((line) => drawSample(preset, line));
      preset.sample.forEach((line, n) => {
        expect(drawn[n].fired, `${preset.id} ${line.text}`).toContain(line.shows);
      });
      expect(
        drawn.map((d) => d.runs),
        preset.id,
      ).toEqual(SAMPLES_DRAW[preset.id]);
    }
  });

  it('colors a room line only through its Room target', () => {
    const preset = presetById('room_and_time');
    const line = preset?.sample.find((l) => l.target === 'room');
    if (!preset || !line) throw new Error('the room sample lists a room line');
    const plain = { text: line.text, shows: line.shows };
    expect(drawSample(preset, plain).runs).toEqual([[line.text, null]]);
  });

  it('puts the tell in its sample in the chat pane as one you sent', () => {
    const preset = presetById('sent_tells');
    if (!preset) throw new Error('no sent_tells preset');
    const [line] = preset.sample;
    const drawn = drawSample(preset, line);
    expect(drawn.routes).toEqual(['tell']);
    expect(parseRoutedLine({ pane: 'tell', text: line.text })).toMatchObject({
      direction: 'sent',
      speaker: 'Tolliver',
      text: 'The day has begun.',
    });
  });

  it('routes no other sample anywhere', () => {
    for (const preset of PRESETS.filter((p) => p.id !== 'sent_tells')) {
      for (const line of preset.sample) {
        expect(drawSample(preset, line).routes, `${preset.id} ${line.text}`).toEqual([]);
      }
    }
  });
});

// Numpad movement binds the six directions the game has and adds no
// trigger. It came after the defaults froze, so it starts off, and Get
// started suggests it on no world.
describe('the Numpad movement preset', () => {
  const preset = presetById('numpad_movement');
  if (!preset) throw new Error('no numpad_movement preset');

  it('is off by default, under Movement', () => {
    expect(PRESETS_ON_BY_DEFAULT).not.toContain(preset.id);
    expect(enabledPresetIds([])).not.toContain(preset.id);
    expect(PRESET_CATEGORIES[preset.category]).toBe('Movement');
  });

  it('suggests no world and has no sample, since it changes no line', () => {
    expect(preset.suggest).toEqual([]);
    expect(preset.sample).toEqual([]);
    expect(preset.triggers).toEqual([]);
  });

  it('binds the six keys in the game order n e s w u d, each tagged with the preset', () => {
    expect(presetMacros(preset)).toEqual([
      { key: 'Numpad8', command: 'n', preset: 'numpad_movement' },
      { key: 'Numpad6', command: 'e', preset: 'numpad_movement' },
      { key: 'Numpad2', command: 's', preset: 'numpad_movement' },
      { key: 'Numpad4', command: 'w', preset: 'numpad_movement' },
      { key: 'Numpad9', command: 'u', preset: 'numpad_movement' },
      { key: 'Numpad3', command: 'd', preset: 'numpad_movement' },
    ]);
  });
});

// Get started suggests presets by the world you connect to. Six suit The
// Forsaken Lands, whose lines they match, and none suit another game.
// The presets step lists the five outside Chat, and the Chat step lists
// Tells you send, which the Presets page rings as suggested too.
describe('the worlds each preset suits', () => {
  const suggested = PRESETS.filter((p) => p.suggest.includes('The Forsaken Lands'));

  it('suggests six on The Forsaken Lands, Tells you send among them', () => {
    expect(suggested.map((p) => p.id)).toEqual([
      'healing_basics',
      'combat_outgoing',
      'combat_incoming',
      'loot_progression',
      'sent_tells',
      'room_and_time',
    ]);
  });

  it('lists the five that color a room, a fight, a cure and experience on the presets step', () => {
    expect(suggested.filter((p) => p.category !== 'chat').map((p) => p.id)).toEqual([
      'healing_basics',
      'combat_outgoing',
      'combat_incoming',
      'loot_progression',
      'room_and_time',
    ]);
    expect(suggested.filter((p) => p.category === 'chat').map((p) => p.id)).toEqual(['sent_tells']);
  });

  it('names only worlds Vosh knows, each once', () => {
    const known = KNOWN_WORLDS.map((w) => w.name);
    for (const preset of PRESETS) {
      expect(new Set(preset.suggest).size, preset.id).toBe(preset.suggest.length);
      for (const world of preset.suggest) {
        expect(known, preset.id).toContain(world);
      }
    }
  });

  it('suggests no preset that sends a command', () => {
    for (const preset of PRESETS.filter((p) => p.suggest.length > 0)) {
      const kinds = preset.triggers.flatMap((t) => t.actions.map((a) => a.kind));
      expect(kinds, preset.id).not.toContain('send');
    }
  });
});

describe('the Disarms and fading buffs preset', () => {
  const sends = (name: string) =>
    presetTriggers(presetById('disarm_buff_fade')!)
      .find((t) => t.name === name)!
      .actions.filter((a) => a.kind === 'send')
      .map((a) => a.template);

  it('takes a secondary weapon back with dual and a primary with wield', () => {
    expect(sends('disarm.secondary')).toEqual(['get 1.;dual 1.']);
    expect(sends('disarm.primary')).toEqual(['get 1.;wield 1.']);
  });
});

describe('the preset trigger names an import keeps', () => {
  it('names every trigger of every preset once, on or off', () => {
    const names = presetTriggerNames();
    expect(names).toEqual(PRESETS.flatMap((p) => presetTriggers(p)).map((t) => t.name));
    expect(new Set(names).size).toBe(names.length);
    expect(names).toContain('disarm.secondary');
  });
});

// Each preset names its colors once, by what they mark, and its templates
// and highlights name them by key. The swatch table gives each swatch,
// its color and the triggers it paints.
describe('the colors each preset names', () => {
  // The keys `trigger` names, each once, in the order it names them.
  const keysOf = (trigger: PresetTrigger): string[] => {
    const keys = trigger.actions.flatMap((a) =>
      a.kind === 'highlight'
        ? [a.style.fg]
        : a.kind === 'replace'
          ? [...a.template.matchAll(/\{([a-z_]+)\}/g)].map((m) => m[1]).filter((k) => k !== 'reset')
          : [],
    );
    return [...new Set(keys)];
  };

  it('installs every preset you never changed byte for byte as it shipped', () => {
    const now = PRESETS.map((p) => ({ id: p.id, triggers: presetTriggers(p) }));
    expect(`${JSON.stringify(now, null, 2)}\n`).toBe(shippedTriggers);
    const none = PRESETS.map((p) => ({ id: p.id, triggers: presetTriggers(p, {}) }));
    expect(none).toEqual(now);
  });

  it('gives 24 swatches over the 74 triggers, each painting the triggers of the table', () => {
    const table = Object.fromEntries(
      PRESETS.map((p) => [
        p.id,
        Object.entries(p.colors).map(([key, c]) => [
          c.label,
          c.token,
          c.sits,
          p.triggers.filter((t) => keysOf(t).includes(key)).length,
        ]),
      ]),
    );
    expect(table).toEqual({
      healing_basics: [['The line', 'bright_green', 'highlight', 6]],
      defensive_combat: [
        ['Routine defenses', 'fg:240', 'template', 15],
        ['Shadows envelop', 'fg:253', 'template', 1],
      ],
      disarm_buff_fade: [
        ['The ## mark', 'bold_red', 'template', 7],
        ['The line', 'fg:178', 'template', 7],
      ],
      terror_events: [['The line', 'bright_red', 'highlight', 1]],
      combat_outgoing: [
        ['The rest of the line', 'fg:253', 'template', 2],
        ['The damage verb', 'fg:214', 'template', 1],
        ['A miss', 'fg:152', 'template', 1],
      ],
      combat_incoming: [
        ['The rest of the line', 'fg:244', 'template', 2],
        ['The damage verb', 'fg:210', 'template', 1],
        ['A miss', 'fg:152', 'template', 1],
      ],
      loot_progression: [
        ['What you gain', 'fg:230', 'template', 4],
        ['Skill and level lines', 'fg:120', 'template', 3],
        ['The gold line', 'fg:249', 'template', 1],
        ['The experience line', 'fg:248', 'template', 1],
      ],
      potion_labels: [['The spell', 'fg:248', 'template', 10]],
      herb_labels: [['The spell', 'fg:248', 'template', 18]],
      sent_tells: [],
      room_and_time: [
        ['Exits', 'green', 'highlight', 1],
        ['What is in the room', 'yellow', 'highlight', 1],
        ['Your target', 'bright_red', 'highlight', 1],
        ['Time of day', 'blue', 'highlight', 1],
        ['Weather change', '#8fa7d9', 'template', 1],
        ['WiZNET tag', 'magenta', 'highlight', 1],
      ],
      numpad_movement: [],
    });
    expect(PRESETS.flatMap((p) => Object.keys(p.colors))).toHaveLength(24);
    expect(PRESETS.flatMap((p) => p.triggers)).toHaveLength(74);
  });

  it('names in each template and highlight only keys its preset has, each where it sits', () => {
    for (const preset of PRESETS) {
      for (const trigger of preset.triggers) {
        for (const action of trigger.actions) {
          const keys =
            action.kind === 'highlight'
              ? [action.style.fg]
              : action.kind === 'replace'
                ? keysOf({ ...trigger, actions: [action] })
                : [];
          const sits = action.kind === 'highlight' ? 'highlight' : 'template';
          for (const key of keys) {
            expect(preset.colors[key]?.sits, `${trigger.name} ${key}`).toBe(sits);
          }
        }
      }
    }
  });

  it('keeps a highlight to the sixteen and never names a key a color token holds', () => {
    const sixteen = HIGHLIGHT_COLORS.map((c) => c.value as string);
    for (const preset of PRESETS) {
      for (const [key, color] of Object.entries(preset.colors)) {
        if (color.sits === 'highlight') expect(sixteen, key).toContain(color.token);
        expect(colorize(`{${key}}`), key).toBe(`{${key}}`);
        expect(colorize(`{${color.token}}`), key).not.toBe(`{${color.token}}`);
      }
    }
  });

  it('fills a key with your color over the preset, the mark keeping its bold', () => {
    const preset = presetById('disarm_buff_fade')!;
    const aura = (colors?: Record<string, string>) =>
      presetTriggers(preset, colors).find((t) => t.name === 'buff.protective_aura')!.actions[0];
    expect(aura({ line: '#c3a6ff' })).toEqual({
      kind: 'replace',
      template:
        '\x1b[1;31m##\x1b[0m \x1b[38;2;195;166;255mThe protective aura around your body fades.\x1b[0m',
    });
    expect(aura({ mark: 'fg:141' })).toEqual({
      kind: 'replace',
      template:
        '\x1b[1m\x1b[38;5;141m##\x1b[0m \x1b[38;5;178mThe protective aura around your body fades.\x1b[0m',
    });
    const cures = presetTriggers(presetById('healing_basics')!, { line: 'cyan' });
    expect(
      cures.every((t) => t.actions[0].kind === 'highlight' && t.actions[0].style.fg === 'cyan'),
    ).toBe(true);
  });
});

// The Looks like row draws each run in the theme's colors, as the
// terminal does, so a fixed color darkens on Vellum the way Keep
// highlight colors readable darkens it (readable.rs).
describe('the colors a sample draws in', () => {
  const xterm = findTheme('rubric').xterm;
  const palette = ANSI_SLOTS.map((slot) => xterm[slot]);
  const paint = { palette, ground: xterm.background, brightBold: true };

  it('darkens a fixed color until it reads on a light ground', () => {
    const lifted = sampleRunCss('178', paint).color ?? '';
    const ground = parseHex(xterm.background)!;
    expect(contrast(parseHex('#d7af00')!, ground)).toBeLessThan(4.5);
    expect(contrast(parseHex(lifted)!, ground)).toBeGreaterThanOrEqual(4.5);
    expect(sampleRunCss('#8fa7d9', paint).color).not.toBe('#8fa7d9');
  });

  it('keeps a fixed color as it is while the setting is off', () => {
    expect(sampleRunCss('178', { ...paint, ground: null })).toEqual({
      color: '#d7af00',
      bold: false,
    });
  });

  it('takes a theme color from the palette, bold lifting it to its bright pair', () => {
    expect(sampleRunCss('bright_green', paint)).toEqual({ color: xterm.brightGreen, bold: true });
    expect(sampleRunCss('bold red', paint)).toEqual({ color: xterm.brightRed, bold: true });
    expect(sampleRunCss('bold red', { ...paint, brightBold: false })).toEqual({
      color: xterm.brightRed,
      bold: false,
    });
    expect(sampleRunCss(null, paint)).toEqual({ bold: false });
  });

  it('finds the words a tell quotes, and no others', () => {
    const tell = presetById('sent_tells')!.sample[0].text;
    const at = quotedWords(tell);
    expect(at && tell.slice(...at)).toBe('The day has begun.');
    expect(quotedWords("A villager's punch decimates you!")).toBeNull();
  });

  it('finds the bars a line names beside the words a tell quotes, in order', () => {
    const [xp, skill] = presetById('loot_progression')!.sample;
    const words = (text: string, bars?: readonly string[]) =>
      sampleBars(text, bars).map((at) => text.slice(...at));
    expect(words(xp.text, xp.bars)).toEqual(['1250']);
    expect(words(skill.text, skill.bars)).toEqual(['dagger']);
    const [hit] = presetById('combat_outgoing')!.sample;
    expect(words(hit.text, hit.bars)).toEqual(['a villager']);
    for (const line of presetById('combat_incoming')!.sample) {
      expect(words(line.text, line.bars)).toEqual(["A villager's", 'punch']);
    }
    expect(words("You tell Tolliver 'The day has begun.'", ['Tolliver'])).toEqual([
      'Tolliver',
      'The day has begun.',
    ]);
    expect(words('You feel less sick.', ['dagger'])).toEqual([]);
  });
});
