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
// mockups do. Every line comes from fixtures/room-colors/lines.json, game
// text built from the server's own format strings and area files, and
// its README says where each one comes from. The patterns are Rust regex
// and use only what JavaScript reads the same way. The Rust tests run the
// same triggers, from preset.json, through the session's own steps.
describe('the Room and time colors preset', () => {
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

  it('is on from the start, under Rooms and time', () => {
    expect(preset?.name).toBe('Room and time colors');
    expect(preset?.description).toBe(
      'Colors the exits green, what is in the room yellow, the time of day blue, and the WiZNET tag magenta.',
    );
    expect(defaultEnabledIds()).toContain('room_and_time');
    expect(preset && PRESET_CATEGORIES[preset.category]).toBe('Rooms and time');
    expect(triggers.every((t) => t.preset === 'room_and_time')).toBe(true);
  });

  it('holds the triggers the Rust tests run', () => {
    expect(triggers).toEqual(roomPreset.triggers);
  });

  it("draws in the theme's own green, yellow, blue and magenta", () => {
    const styles = Object.fromEntries(
      triggers.map((t) => [
        t.name,
        t.actions.map((a) => (a.kind === 'highlight' ? a.style : a.kind)),
      ]),
    );
    expect(styles).toEqual({
      'room.exits': [{ fg: 'green', base: true }],
      'room.contents': [{ fg: 'yellow', base: true }],
      'time.of_day': [{ fg: 'blue' }],
      'wiznet.tag': [{ fg: 'magenta', bold: true }],
    });
  });

  it('colors what is in the room only through the Room target', () => {
    expect(named('room.contents').target).toBe('room');
    expect(lineTriggers.map((t) => t.name)).toEqual(['room.exits', 'time.of_day', 'wiznet.tag']);
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

  it('leaves a say, a tell, a channel, a prompt and a room name alone', () => {
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
// in loadout_store.rs), and Settings stores PRESETS_OFF_MARKER when you
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
