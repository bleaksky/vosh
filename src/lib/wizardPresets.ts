import { enabledPresetIds } from './automationRecords';
import { PRESETS } from './presets';
import type { MigrationPlan } from './session';

// In loadout mode every character shares one list of presets that are
// on, the one the shared catalog wizard builds from the profile files.
// The preview says who gains and who loses a preset by it.

/** The presets one character gains and loses when the shared list takes
 *  the place of its own, by name in library order. */
export interface PresetChange {
  profile: string;
  gains: string[];
  loses: string[];
}

/** Each character whose presets change under the shared list, in profile
 *  order. A profile that never saved a file holds the defaults, which
 *  turn on the presets on by default, as per profile mode reads it. */
export function presetChanges(
  plan: Pick<MigrationPlan, 'source_profiles' | 'shared_presets' | 'profile_presets'>,
): PresetChange[] {
  const shared = new Set(enabledPresetIds(plan.shared_presets));
  const changes: PresetChange[] = [];
  plan.source_profiles.forEach((profile, n) => {
    const own = new Set(enabledPresetIds(plan.profile_presets[n] ?? []));
    const gains = PRESETS.filter((p) => shared.has(p.id) && !own.has(p.id)).map((p) => p.name);
    const loses = PRESETS.filter((p) => own.has(p.id) && !shared.has(p.id)).map((p) => p.name);
    if (gains.length > 0 || loses.length > 0) changes.push({ profile, gains, loses });
  });
  return changes;
}
