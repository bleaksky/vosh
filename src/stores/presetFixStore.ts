// The corner notice a preset fix leaves in the main window. Each run of
// the preset plan in this window hands its rows here, and the notice
// stays until you close it. A run that tells of nothing new leaves the
// notice as it is.

import { fixNotice, type FixNotice } from '../automation/presetEdits';
import type { PresetNotice } from '../automation/presetPlan';
import { createStore } from './store';

export const presetFixStore = createStore<FixNotice | null>(null);

/** Show what a plan run tells, when it tells anything. */
export function showPresetFix({ told, removed }: PresetNotice): void {
  const notice = fixNotice(told, removed);
  if (notice) presetFixStore.set(notice);
}

export function closePresetFix(): void {
  presetFixStore.set(null);
}
