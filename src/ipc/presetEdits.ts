// Your edits to the presets, the [preset_edits] table. Each call names
// the profile it means, or names none and reaches the profile the
// selected session plays. In loadout mode every profile shares the
// catalog's table.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { PRESET_EDITS_CHANGED } from './events';

/** A value a row holds, as TOML keeps it: a switch, a number, a text, a
 *  list or a table. */
export type EditValue = string | number | boolean | EditValue[] | { [key: string]: EditValue };

/** One row you changed: your value and the preset's value you changed it
 *  from. `seen` is the preset's value a launch notice already named.
 *  Mirrors EditRow in src-tauri/src/loadouts/preset_edits.rs. */
export interface EditRow {
  value: EditValue;
  was: EditValue;
  seen?: EditValue;
}

/** Your edits to one preset: its swatches by color key, and its
 *  triggers' rows by trigger name, then by row key. Either is left out
 *  while it holds none. */
export interface PresetEdit {
  colors?: Record<string, EditRow>;
  triggers?: Record<string, Record<string, EditRow>>;
}

/** Your edits, by preset id. A preset with none has no entry. */
export type PresetEdits = Record<string, PresetEdit>;

export async function presetEditsGet(profile?: string | null): Promise<PresetEdits> {
  return invoke('preset_edits_get', { profile });
}

/** Save the rows the page changed in the preset `id`. Rust keeps the
 *  `was` of a row it already holds, keeps its `seen` while the row sent
 *  has one, and drops a row whose value is the preset's. */
export async function presetEditsSet(
  id: string,
  edits: PresetEdit,
  profile?: string | null,
): Promise<void> {
  return invoke('preset_edits_set', { id, edits, profile });
}

/** Hear that a save changed the edits of a profile, which the event
 *  names, null before any profile loads. */
export async function onPresetEditsChanged(
  cb: (profile: string | null) => void,
): Promise<UnlistenFn> {
  return listen<{ profile: string | null }>(PRESET_EDITS_CHANGED, (event) => {
    cb(event.payload.profile);
  });
}
