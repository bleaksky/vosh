// The preset triggers and macros of one profile, built in the page from
// presets.ts with your edits laid over them, and installed for that
// profile (Presets Q10). It runs each time a profile opens, at launch, on
// a switch and when #profile load or an import replaces the config, so a
// profile never runs another profile's edits or a stale copy.

import { presetLaunchPlan } from './automationRecords';
import { buildPreset, type RowRef } from './presetEdits';
import { PRESETS, presetMacros } from './presets';
import {
  listMacros,
  listTriggers,
  presetsEnabledSet,
  presetsInstall,
  presetsRemove,
  type PresetSwitch,
} from '../ipc/automation';
import { presetEditsGet, presetEditsSet, type PresetEdits } from '../ipc/presetEdits';
import { getUiConfig } from '../ipc/uiConfig';

/** What a run has to tell you. */
export interface PresetNotice {
  /** The rows you edited that a fix changed, each named once. */
  told: RowRef[];
  /** The rows and the triggers you edited that a fix took away. */
  removed: RowRef[];
}

let running: Promise<unknown> = Promise.resolve();

/** Bring the preset triggers and macros of `profile`, or of the selected
 *  session's profile when it names none, in line with its presets and
 *  your edits. `switches` turn presets on and off first, through
 *  presets_enabled_set. A run waits for the one before it, so two never
 *  interleave. A failed call is logged and the rest still run. */
export function runPresetPlan(
  profile: string | null = null,
  switches: readonly PresetSwitch[] = [],
): Promise<PresetNotice> {
  const run = running.then(() => presetPlan(profile, switches));
  running = run.catch(() => undefined);
  return run;
}

async function presetPlan(
  profile: string | null,
  switches: readonly PresetSwitch[],
): Promise<PresetNotice> {
  const notice: PresetNotice = { told: [], removed: [] };
  let stored: string[];
  try {
    stored = (await getUiConfig(profile)).enabled_presets;
  } catch (e) {
    console.error('[presets] reading the presets that are on failed:', e);
    return notice;
  }
  let edits: PresetEdits = {};
  try {
    edits = await presetEditsGet(profile);
  } catch (e) {
    console.error('[presets] reading your preset edits failed:', e);
  }
  const installed: (string | null | undefined)[] = [];
  try {
    installed.push(...(await listTriggers(profile)).map((t) => t.preset));
  } catch (e) {
    console.error('[presets] listing triggers failed:', e);
  }
  try {
    installed.push(...(await listMacros(profile)).map((m) => m.preset));
  } catch (e) {
    console.error('[presets] listing macros failed:', e);
  }

  // Take out every preset that is off, or that this build no longer has.
  // In loadout mode the triggers, the macros and the list are shared by
  // every profile, and without this a preset you turned off came back
  // after a launch as another character.
  const plan = presetLaunchPlan(stored, installed, switches);
  for (const id of plan.remove) {
    try {
      await presetsRemove(id, profile);
    } catch (e) {
      console.error(`[presets] removing ${id} failed:`, e);
    }
  }

  // Install every preset that is on again, in your colors with your rows
  // laid over it, so this build's fixes reach every row you left alone.
  const on = PRESETS.filter((p) => plan.install.includes(p.id));
  const builds = on.map((p) => ({ id: p.id, ...buildPreset(p, edits[p.id]) }));
  const triggers = builds.flatMap((b) => b.triggers);
  const macros = on.flatMap(presetMacros);
  try {
    if (switches.length > 0) await presetsEnabledSet(switches, triggers, macros, profile);
    else if (triggers.length > 0 || macros.length > 0) {
      await presetsInstall(triggers, macros, profile);
    }
  } catch (e) {
    console.error('[presets] installing the presets failed:', e);
    return notice;
  }

  // Save back what this run told and what folded or went away, so the
  // next run tells nothing twice.
  for (const b of builds) {
    notice.told.push(...b.told);
    notice.removed.push(...b.removed);
    if (!b.write.colors && !b.write.triggers) continue;
    try {
      await presetEditsSet(b.id, b.write, profile);
    } catch (e) {
      console.error(`[presets] saving your edits to ${b.id} failed:`, e);
    }
  }
  return notice;
}
