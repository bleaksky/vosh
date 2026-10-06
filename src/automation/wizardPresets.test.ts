import { describe, expect, it } from 'vitest';
import { defaultEnabledIds, PRESETS } from './presets';
import { presetChanges } from './wizardPresets';

const name = (id: string): string => {
  const preset = PRESETS.find((p) => p.id === id);
  if (!preset) throw new Error(`no preset ${id}`);
  return preset.name;
};

describe('the presets each character gains or loses in the shared list', () => {
  // Default had the heals on, the Healer the heals and the herb labels,
  // and Test-Prompt never saved a file. The shared list holds every preset
  // a saved profile had on.
  const plan = {
    source_profiles: ['default', 'Healer', 'Test-Prompt'],
    shared_presets: ['healing_basics', 'herb_labels'],
    profile_presets: [['healing_basics'], ['healing_basics', 'herb_labels'], []],
  };

  it('names each character whose presets change, and how', () => {
    const changes = presetChanges(plan);
    expect(changes.map((c) => c.profile)).toEqual(['default', 'Test-Prompt']);
    expect(changes[0]).toEqual({
      profile: 'default',
      gains: [name('herb_labels')],
      loses: [],
    });
  });

  it('counts a profile that never saved a file as having the defaults on', () => {
    const prompt = presetChanges(plan)[1];
    expect(prompt.gains).toEqual([]);
    expect(prompt.loses).toEqual(
      defaultEnabledIds()
        .filter((id) => id !== 'healing_basics' && id !== 'herb_labels')
        .map(name),
    );
  });

  it('reads every preset off and the defaults', () => {
    const changes = presetChanges({
      source_profiles: ['default', 'Healer'],
      shared_presets: [],
      profile_presets: [['none'], []],
    });
    expect(changes).toEqual([
      { profile: 'default', gains: defaultEnabledIds().map(name), loses: [] },
    ]);
  });
});
