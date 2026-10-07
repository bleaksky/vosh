import { describe, expect, it } from 'vitest';
import roomLines from '../../fixtures/room-colors/lines.json';
import roomPreset from '../../fixtures/room-colors/preset.json';
import { enabledPresetIds, PRESETS_OFF_MARKER } from './automationRecords';
import { parseRoutedLine } from '../stores/gmcp/chatStore';
import { KNOWN_WORLDS } from '../lib/knownWorlds';
import {
  defaultEnabledIds,
  type Preset,
  PRESET_CATEGORIES,
  PRESETS,
  PRESETS_ON_BY_DEFAULT,
  presetById,
  presetMacros,
  type PresetSampleLine,
  presetTriggerNames,
  presetTriggers,
} from './presets';
import type { HighlightStyle, TriggerTarget } from '../ipc/automation';

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

// What the trigger engine draws on a line, modeled on process_on_ground in
// crates/automation/src/trigger/engine.rs. Triggers run high priority
// first. A Replace rewrites the text through its template, $1 to $9
// filled from the groups, a highlight colors the text it matches with the
// first span winning, and a base color fills what is left in the default
// color. The runs name each color by its ANSI name, its 256 color index
// or its hex, with bold before it. The model draws no highlight over a
// replaced line, which the engine matches against the rebuilt text, since
// no sample has one.
type Run = [text: string, color: string | null];

interface Drawn {
  fired: string[];
  runs: Run[];
  routes: string[];
}

const ANSI_NAMES = ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'];

function styleColor(style: HighlightStyle): string | null {
  if (!style.fg) return null;
  return style.bold ? `bold ${style.fg}` : style.fg;
}

// Each character of `text`, which holds SGR codes, with the color it
// shows in.
function sgrChars(text: string): { ch: string; color: string | null }[] {
  const chars: { ch: string; color: string | null }[] = [];
  let fg: string | null = null;
  let bold = false;
  // eslint-disable-next-line no-control-regex
  for (const m of text.matchAll(/\x1b\[([0-9;]*)m|([^\x1b])/g)) {
    if (m[2] !== undefined) {
      chars.push({ ch: m[2], color: fg && bold ? `bold ${fg}` : fg });
      continue;
    }
    const codes = m[1].split(';').map((c) => (c === '' ? 0 : Number(c)));
    for (let i = 0; i < codes.length; i++) {
      const c = codes[i];
      if (c === 0) [fg, bold] = [null, false];
      else if (c === 1) bold = true;
      else if (c === 22) bold = false;
      else if (c === 39) fg = null;
      else if (c >= 30 && c <= 37) fg = ANSI_NAMES[c - 30];
      else if (c >= 90 && c <= 97) fg = `bright_${ANSI_NAMES[c - 90]}`;
      else if (c === 38 && codes[i + 1] === 5) {
        fg = String(codes[i + 2]);
        i += 2;
      } else if (c === 38 && codes[i + 1] === 2) {
        fg = `#${codes
          .slice(i + 2, i + 5)
          .map((n) => n.toString(16).padStart(2, '0'))
          .join('')}`;
        i += 4;
      }
    }
  }
  return chars;
}

function draw(preset: Preset, line: PresetSampleLine): Drawn {
  const scope: TriggerTarget = line.target ?? 'line';
  // Which triggers see the line, as MatchScope::matches has it.
  const reaches = (target: TriggerTarget = 'line') =>
    target === 'line' ||
    (target === 'room' && scope !== 'line') ||
    (target === 'room_target' && scope === 'room_target');
  const triggers = presetTriggers(preset)
    .map((t, n) => ({ t, n }))
    .sort((a, b) => b.t.priority - a.t.priority || a.n - b.n)
    .map(({ t }) => t);
  const fired: string[] = [];
  const routes: string[] = [];
  const spans: [RegExp, string | null][] = [];
  let base: string | null = null;
  let text = line.text;
  let replaced = false;
  for (const t of triggers) {
    if (!t.enabled || !reaches(t.target)) continue;
    for (const row of t.patterns) {
      const regex = new RegExp(row.pattern);
      if (!row.enabled || !regex.test(line.text)) continue;
      if (!fired.includes(t.name)) fired.push(t.name);
      const groups = (new RegExp(`${row.pattern}|`).exec('')?.length ?? 1) - 1;
      for (const action of t.actions) {
        if (action.kind === 'replace') {
          text = text.replace(new RegExp(row.pattern, 'g'), (...m: unknown[]) =>
            action.template.replace(/\$(\d)/g, (_, n: string) => {
              const at = Number(n);
              return at <= groups ? String(m[at] ?? '') : '';
            }),
          );
          replaced = true;
        } else if (action.kind === 'highlight' && action.style.base) {
          base ??= styleColor(action.style);
        } else if (action.kind === 'highlight') {
          spans.push([regex, styleColor(action.style)]);
        } else if (action.kind === 'route' && !routes.includes(action.pane)) {
          routes.push(action.pane);
        }
      }
    }
  }
  if (replaced && spans.length > 0) {
    throw new Error(`the model draws no highlight over a replaced line: ${line.text}`);
  }
  const chars = replaced
    ? sgrChars(text)
    : [...line.text].map((ch) => ({ ch, color: null as string | null }));
  const taken = chars.map(() => false);
  for (const [regex, color] of spans) {
    if (!color) continue;
    for (const m of line.text.matchAll(new RegExp(regex.source, 'g'))) {
      const start = m.index;
      const end = start + m[0].length;
      if (end === start || taken.slice(start, end).some(Boolean)) continue;
      for (let i = start; i < end; i++) {
        taken[i] = true;
        chars[i].color = color;
      }
    }
  }
  const runs: Run[] = [];
  for (const { ch, color } of chars) {
    const shown = color ?? base;
    const last = runs.at(-1);
    if (last && last[1] === shown) last[0] += ch;
    else runs.push([ch, shown]);
  }
  return { fired, runs, routes };
}

// What each line of each preset's sample shows, in order, as runs of text
// and their colors. A preset that names a 256 color index paints that
// index on every theme, and one that names an ANSI color paints the
// theme's own.
const SAMPLES_DRAW: Record<string, Run[][]> = {
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
      ['grazes', '210'],
      [' you.', '244'],
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
    [['You raise a level!!', '120']],
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
    [['A Blackwatch villager scurries about, taking care of business.', 'yellow']],
    [['The day has begun.', 'blue']],
  ],
};

// Every preset that adds triggers carries a sample, lines the game prints
// in its own words, which Get started and the Presets page will show. Each
// runs here through the preset's own triggers, so a sample cannot drift
// from what its preset paints. A preset that only binds macros changes no
// line, so it has no sample.
const TRIGGER_PRESETS = PRESETS.filter((p) => p.triggers.length > 0);

describe('the sample of every preset', () => {
  it('holds one to three lines as the game prints them, each naming a trigger of its preset', () => {
    for (const preset of TRIGGER_PRESETS) {
      expect(preset.sample.length, preset.id).toBeGreaterThanOrEqual(1);
      expect(preset.sample.length, preset.id).toBeLessThanOrEqual(3);
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
      const drawn = preset.sample.map((line) => draw(preset, line));
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
    expect(draw(preset, plain).runs).toEqual([[line.text, null]]);
  });

  it('puts the tell in its sample in the chat pane as one you sent', () => {
    const preset = presetById('sent_tells');
    if (!preset) throw new Error('no sent_tells preset');
    const [line] = preset.sample;
    const drawn = draw(preset, line);
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
        expect(draw(preset, line).routes, `${preset.id} ${line.text}`).toEqual([]);
      }
    }
  });
});

// Numpad movement binds the six directions the game has (Scripts board 7,
// Q12) and adds no trigger. It came after the defaults froze, so it
// starts off, and Get started suggests it on no world (First Run Q18).
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
// The presets step lists the five outside Chat (Q5 of the first run
// review), and the Chat step lists Tells you send, which the Presets page
// rings as suggested too.
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
    expect(names).toEqual(PRESETS.flatMap(presetTriggers).map((t) => t.name));
    expect(new Set(names).size).toBe(names.length);
    expect(names).toContain('disarm.secondary');
  });
});
