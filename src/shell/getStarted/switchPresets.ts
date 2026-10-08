import { runPresetPlan } from '../../automation/presetPlan';
import type { PresetSwitch } from '../../ipc/automation';
import { errorText } from '../../lib/text';
import { showPresetFix } from '../../stores/presetFixStore';
import { pushToast } from '../../stores/toasts';

// A switch in Get started saves at once (First Run Q4, Q17). It turns
// presets on or off for the profile you play through
// presets_enabled_set, in one call however many it flips, and the card
// shows the list vosh://presets-changed brings back.

/** Turn `changes` on or off, and say so when they did not save. */
export function switchPresets(changes: readonly PresetSwitch[]): Promise<void> {
  return runPresetPlan(null, changes).then(showPresetFix, (e: unknown) => {
    pushToast({ kind: 'error', message: "That switch didn't save", meta: errorText(e) });
  });
}
