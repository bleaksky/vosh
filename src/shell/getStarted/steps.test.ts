import { describe, expect, it } from 'vitest';
import {
  doneByFacts,
  doneCount,
  stepMeta,
  stepsFor,
  suggestedPresets,
  type GetStartedFacts,
} from './steps';

const FORSAKEN = { host: 'play.theforsakenlands.com', port: 1848, tls: false };
const OTHER = { host: 'mud.example.net', port: 4000, tls: false };

const NOTHING: GetStartedFacts = {
  character: null,
  enabledPresets: ['none'],
  panes: ['map', 'affects'],
  tracked: 0,
  promptPlace: null,
};

describe('stepsFor', () => {
  it('lists five steps on The Forsaken Lands, two of them after you log in', () => {
    const steps = stepsFor(FORSAKEN);
    expect(steps.map((s) => s.id)).toEqual(['connect', 'presets', 'panes', 'affects', 'prompt']);
    expect(steps.filter((s) => s.afterLogin).map((s) => s.id)).toEqual(['affects', 'prompt']);
    expect(steps.map((s) => s.title)).toEqual([
      'Connect to The Forsaken Lands',
      'Color what the game prints',
      'Add Chat and Group',
      'Track the affects you keep up',
      'Customize your prompt',
    ]);
    expect(steps[0]?.line).toBe(
      "Vosh dials play.theforsakenlands.com on port 1848, and you log in at the game's own prompt.",
    );
  });

  it('lists two steps on another game, named for its host', () => {
    const steps = stepsFor(OTHER);
    expect(steps.map((s) => s.id)).toEqual(['connect', 'prompt']);
    expect(steps[0]?.title).toBe('Connect to mud.example.net');
    expect(steps.some((s) => s.afterLogin)).toBe(false);
  });
});

describe('suggestedPresets', () => {
  it('suggests the five that change how lines look, and leaves Chat to its step', () => {
    expect(
      suggestedPresets(FORSAKEN.host)
        .map((p) => p.name)
        .sort(),
    ).toEqual([
      'Cures and heals',
      'Damage to you',
      'Gold, experience, and levels',
      'Room, time and weather colors',
      'Your damage verbs',
    ]);
    expect(suggestedPresets(OTHER.host)).toEqual([]);
  });
});

describe('doneByFacts', () => {
  it('finishes nothing on a fresh install', () => {
    expect(doneByFacts(FORSAKEN.host, NOTHING)).toEqual([]);
  });

  it('never counts the defaults before the presets are read', () => {
    expect(doneByFacts(FORSAKEN.host, { ...NOTHING, enabledPresets: null })).toEqual([]);
  });

  it('finishes presets on a suggestion, panes on Chat or Group and affects on one tracked', () => {
    const id = suggestedPresets(FORSAKEN.host)[0]?.id ?? '';
    const facts = { ...NOTHING, enabledPresets: [id], panes: ['map', 'group'], tracked: 1 };
    expect(doneByFacts(FORSAKEN.host, facts)).toEqual(['presets', 'panes', 'affects']);
  });

  it('finishes nothing by facts on another game', () => {
    const facts = { ...NOTHING, enabledPresets: [], panes: ['chat'], tracked: 3 };
    expect(doneByFacts(OTHER.host, facts)).toEqual([]);
  });
});

describe('stepMeta', () => {
  it('names what each step holds now', () => {
    const facts: GetStartedFacts = {
      character: 'Orla',
      enabledPresets: suggestedPresets(FORSAKEN.host).map((p) => p.id),
      panes: ['map', 'chat', 'group'],
      tracked: 3,
      promptPlace: 'Pinned',
    };
    const metas = stepsFor(FORSAKEN).map((s) => stepMeta(s.id, FORSAKEN.host, facts));
    expect(metas).toEqual(['Orla', '5 on', 'Chat and Group', '3 tracked', 'Pinned']);
  });

  it('holds nothing while nothing is set', () => {
    const metas = stepsFor(FORSAKEN).map((s) => stepMeta(s.id, FORSAKEN.host, NOTHING));
    expect(metas).toEqual([null, null, null, null, null]);
  });

  it('names one social pane alone', () => {
    expect(stepMeta('panes', FORSAKEN.host, { ...NOTHING, panes: ['group'] })).toBe('Group');
  });
});

describe('doneCount', () => {
  it('counts only the steps the list shows', () => {
    expect(doneCount(stepsFor(OTHER), ['connect', 'presets'])).toBe(1);
  });
});
