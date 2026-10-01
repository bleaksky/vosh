import { describe, expect, it } from 'vitest';
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
      ["You tell Selune 'omw'", 'Selune', 'omw', 'common'],
      ["You tell a city guard in Tol'khan 'it is me'", 'a city guard', 'it is me', "Tol'khan"],
      ["You project to Selune 'omw'", 'Selune', 'omw', 'common'],
      ["You project to Selune in Elvish 'omw'", 'Selune', 'omw', 'Elvish'],
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
      "Selune tells you 'are you still at the bank?'",
      "[Selune] 'omw'",
      'You project your image away from your body.',
    ]) {
      expect(matches(text), text).toBe(false);
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
