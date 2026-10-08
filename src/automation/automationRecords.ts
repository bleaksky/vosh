// Aliases, macros, timers, presets, loadouts, and the tick as the
// Automation page edits them. Each normalizer turns what the backend
// sends into one shape, so two values that save the same way compare
// equal. Each save plan turns a draft into the calls the existing API
// takes: aliases replace the whole store with the draft's changes
// applied over it, macros and timers go item by item, presets install
// and remove triggers, and loadouts set the active list.

import { draftChanges, saveDraftOnto, type Draft, type SavedWrite } from './automationDraft';
import { groupKeyOf, searchText, type ListEntry } from './automationList';
import { defaultEnabledIds, type Preset, PRESETS } from './presets';
import {
  deleteMacro,
  exportAliases,
  importAliases,
  setMacro,
  timersDelete,
  timersSet,
  type AlertParts,
  type Macro,
  type PresetSwitch,
} from '../ipc/automation';
import type { PresetEdit } from '../ipc/presetEdits';
import { type TickConfig } from '../ipc/tick';
import { errorText, listJoin, quoted } from '../lib/text';

// ── Aliases ─────────────────────────────────────────────────────────

export interface AliasRecord {
  name: string;
  expansion: string;
  enabled: boolean;
  group?: string;
  /** A Lua body. Present, even when empty, means the alias runs Lua
   *  and ignores the expansion. */
  script?: string;
}

export function normalizeAlias(raw: unknown): AliasRecord {
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  const out: AliasRecord = {
    name: typeof r.name === 'string' ? r.name : '',
    expansion: typeof r.expansion === 'string' ? r.expansion : '',
    enabled: r.enabled !== false,
  };
  const group = typeof r.group === 'string' ? r.group.trim() : '';
  if (group) out.group = group;
  if (typeof r.script === 'string') out.script = r.script;
  return out;
}

export function blankAlias(): AliasRecord {
  return { name: '', expansion: '', enabled: true };
}

/** The alias JSON aliases_import takes. */
export function aliasesForSave(list: readonly AliasRecord[]): string {
  return JSON.stringify(
    list.map((a) => ({
      name: a.name,
      expansion: a.expansion,
      enabled: a.enabled,
      ...(a.group ? { group: a.group } : {}),
      ...(a.script !== undefined ? { script: a.script } : {}),
    })),
    null,
    2,
  );
}

/** An alias's identity. The store keys aliases by name. */
export const aliasKey = (alias: AliasRecord): string => alias.name;

/** The two calls the alias store takes a whole list through. */
export interface AliasStoreApi {
  exportAliases: () => Promise<string>;
  importAliases: (json: string) => Promise<unknown>;
}

/** The alias store of `profile`, or of the profile the selected session
 *  plays when it names none. */
export function aliasStore(profile?: string | null): AliasStoreApi {
  return {
    exportAliases: () => exportAliases(profile),
    importAliases: (json) => importAliases(json, profile),
  };
}

/** Every alias the store holds, for display. A reply that does not read
 *  shows as no aliases. */
export async function loadAliases(api: AliasStoreApi = aliasStore()): Promise<AliasRecord[]> {
  return parseJsonList(await api.exportAliases(), normalizeAlias) ?? [];
}

/** Save the Aliases draft over the store as it stands now, like
 *  saveTriggerDraft. An alias #alias or a script added after the page
 *  loaded survives, and a list that does not read stops the save. */
export async function saveAliasDraft(
  draft: Draft<AliasRecord>,
  api: AliasStoreApi = aliasStore(),
): Promise<void> {
  await saveDraftOnto(
    draft,
    {
      read: async () => {
        const list = parseJsonList(await api.exportAliases(), normalizeAlias);
        if (!list) throw new Error('Vosh could not read your saved aliases, so it saved nothing.');
        return list;
      },
      write: async (values) => {
        await api.importAliases(aliasesForSave(values));
      },
    },
    aliasKey,
  );
}

export function validateAliases(list: readonly AliasRecord[]): string | null {
  const seen = new Set<string>();
  for (const a of list) {
    const name = a.name.trim();
    if (!name) return 'Give every alias a name before you save.';
    if (seen.has(name)) return `Two aliases are named ${quoted(name)}. Give each one its own name.`;
    seen.add(name);
  }
  return null;
}

// ── Macros ──────────────────────────────────────────────────────────

export interface MacroRecord {
  key: string;
  command: string;
  group?: string;
  enabled: boolean;
  /** The id of the preset that added it, absent for one of yours. */
  preset?: string;
}

export function normalizeMacro(raw: unknown): MacroRecord {
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  const out: MacroRecord = {
    key: typeof r.key === 'string' ? r.key : '',
    command: typeof r.command === 'string' ? r.command : '',
    enabled: r.enabled !== false,
  };
  const group = typeof r.group === 'string' ? r.group.trim() : '';
  if (group) out.group = group;
  if (typeof r.preset === 'string' && r.preset) out.preset = r.preset;
  return out;
}

export function blankMacro(): MacroRecord {
  return { key: '', command: '', enabled: true };
}

/** Why the draft cannot save yet, or null. Only your macros need a key
 *  and a command of their own, since a preset macro saves nothing but
 *  its group, and yours may use a key a preset macro wants. */
export function validateMacros(list: readonly MacroRecord[]): string | null {
  const seen = new Set<string>();
  for (const m of list) {
    if (m.preset) continue;
    if (!m.key) return 'Press a key for every macro before you save.';
    if (seen.has(m.key)) return `Two macros use ${m.key}. Give each one its own key.`;
    seen.add(m.key);
    if (!m.command.trim()) return `The macro on ${m.key} needs a command.`;
  }
  return null;
}

export interface MacroSavePlan {
  /** Keys to unbind first, each with the draft items that held it. */
  remove: { key: string; uids: string[] }[];
  /** Bindings to set after that, each with its draft item. */
  set: { uid: string; macro: MacroRecord }[];
}

/** The macros_delete and macros_set calls that make the store match
 *  the draft. A macro whose key changed unbinds the old key. Unbinding
 *  runs first, so a key another macro takes over ends up bound.
 *
 *  A preset macro saves only a new group, under the key and preset it
 *  loaded with, and never unbinds its key, since macros_delete removes
 *  your macro on that key. One added or removed in Edit all as JSON
 *  sends nothing, since launch installs the preset's own. A row the JSON
 *  view moved between yours and a preset counts as one removed and one
 *  added. */
export function macroSavePlan(draft: Draft<MacroRecord>): MacroSavePlan {
  const { added, removed, changed } = draftChanges(draft);
  const moved = changed.filter((c) => c.before.preset !== c.after.preset);
  const gone = [...removed, ...moved.map((c) => ({ uid: c.uid, value: c.before }))];
  const made = [...added, ...moved.map((c) => ({ uid: c.uid, value: c.after }))];
  const remove = new Map<string, string[]>();
  const unbind = (key: string, uid: string) => remove.set(key, [...(remove.get(key) ?? []), uid]);
  for (const item of gone) if (!item.value.preset) unbind(item.value.key, item.uid);
  const set = made.flatMap((item) =>
    item.value.preset ? [] : [{ uid: item.uid, macro: item.value }],
  );
  for (const { uid, before, after } of changed) {
    if (before.preset !== after.preset) continue;
    if (!before.preset) {
      if (before.key !== after.key) unbind(before.key, uid);
      set.push({ uid, macro: after });
    } else if ((before.group ?? '') !== (after.group ?? '')) {
      const macro = { ...before };
      if (after.group) macro.group = after.group;
      else delete macro.group;
      set.push({ uid, macro });
    }
  }
  return { remove: [...remove].map(([key, uids]) => ({ key, uids })), set };
}

/** The two calls Macros saves through, one binding at a time. */
export interface MacroStoreApi {
  deleteMacro: (key: string) => Promise<unknown>;
  /** `preset` names the preset of a preset macro, which takes only the
   *  group. Absent for yours. */
  setMacro: (
    key: string,
    command: string,
    group: string | null,
    enabled: boolean,
    preset?: string,
  ) => Promise<unknown>;
}

/** The macros of `profile`, or of the profile the selected session
 *  plays when it names none. */
export function macroStore(profile?: string | null): MacroStoreApi {
  return {
    deleteMacro: (key) => deleteMacro(key, profile),
    setMacro: (key, command, group, enabled, preset) =>
      setMacro(key, command, group, enabled, preset, profile),
  };
}

/** Save the Macros draft one call per binding, the way the old Macros
 *  tab saved rows. Each call the store takes goes to `written` as it
 *  lands, so a Save that stops partway can mark those items saved and
 *  the next Save sends only the rest. An unbound old key leaves its
 *  item as new until the new key binds. */
export async function saveMacroDraft(
  draft: Draft<MacroRecord>,
  written: (write: SavedWrite<MacroRecord>) => void,
  api: MacroStoreApi = macroStore(),
): Promise<void> {
  const plan = macroSavePlan(draft);
  for (const { key, uids } of plan.remove) {
    await api.deleteMacro(key);
    for (const uid of uids) written({ uid, stored: null });
  }
  for (const { uid, macro } of plan.set) {
    await api.setMacro(macro.key, macro.command, macro.group ?? null, macro.enabled, macro.preset);
    written({ uid, stored: macro });
  }
}

// ── Timers ──────────────────────────────────────────────────────────

export interface TimerRecord {
  /** The backend's id, or null for a timer the page made. */
  id: number | null;
  name: string;
  interval_secs: number;
  command: string;
  enabled: boolean;
  group?: string;
}

export function normalizeTimer(raw: unknown): TimerRecord {
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  const interval = typeof r.interval_secs === 'number' ? Math.floor(r.interval_secs) : 30;
  const out: TimerRecord = {
    id: typeof r.id === 'number' ? r.id : null,
    name: typeof r.name === 'string' ? r.name : '',
    interval_secs: Math.max(1, Number.isFinite(interval) ? interval : 30),
    command: typeof r.command === 'string' ? r.command : '',
    enabled: r.enabled !== false,
  };
  const group = typeof r.group === 'string' ? r.group.trim() : '';
  if (group) out.group = group;
  return out;
}

export function blankTimer(): TimerRecord {
  return { id: null, name: '', interval_secs: 30, command: '', enabled: true };
}

export function validateTimers(list: readonly TimerRecord[]): string | null {
  for (const t of list) {
    if (!t.command.trim()) {
      return t.name.trim()
        ? `The timer ${quoted(t.name.trim())} needs a command.`
        : 'Every timer needs a command before you save.';
    }
  }
  return null;
}

export interface TimerSavePlan {
  /** Timers to delete, each with its draft item. */
  remove: { uid: string; id: number }[];
  /** Timers to create (id null) or update, each with its draft item
   *  and the item's value as the draft holds it. */
  set: { uid: string; timer: TimerRecord; value: TimerRecord }[];
}

/** A timer's identity across a reload. A new timer has no id until it
 *  saves, so the page finds it again by what it holds. */
export function timerKey(timer: TimerRecord): string {
  return `${timer.name.trim()}\n${timer.interval_secs}\n${timer.command.trim()}`;
}

/** The timers_delete and timers_set calls that make the store match the
 *  draft. Ids come from the load, never from the draft values: a changed
 *  timer updates the id it loaded with, and a new one always creates.
 *  The JSON view shows ids, so a copied entry carries its original's id,
 *  and sending that id would overwrite the original. */
export function timerSavePlan(draft: Draft<TimerRecord>): TimerSavePlan {
  const { added, removed, changed } = draftChanges(draft);
  const remove = removed.flatMap((item) =>
    item.value.id === null ? [] : [{ uid: item.uid, id: item.value.id }],
  );
  const set = [
    ...changed.map((c) => ({ uid: c.uid, timer: { ...c.after, id: c.before.id }, value: c.after })),
    ...added.map((item) => ({
      uid: item.uid,
      timer: { ...item.value, id: null },
      value: item.value,
    })),
  ];
  return { remove, set };
}

/** The two calls Timers saves through, one timer at a time. */
export interface TimerStoreApi {
  timersDelete: (id: number) => Promise<unknown>;
  /** Returns every timer the store holds after the write. */
  timersSet: (
    id: number | null,
    name: string,
    intervalSecs: number,
    command: string,
    enabled: boolean,
    group: string | null,
  ) => Promise<unknown[]>;
}

/** The timers of `profile`, or of the profile the selected session
 *  plays when it names none. */
export function timerStore(profile?: string | null): TimerStoreApi {
  return {
    timersDelete: (id) => timersDelete(id, profile),
    timersSet: (id, name, intervalSecs, command, enabled, group) =>
      timersSet(id, name, intervalSecs, command, enabled, group, profile),
  };
}

/** Save the Timers draft one call per timer, the way the old Timers tab
 *  saved cards. Each call the store takes goes to `written` as it lands,
 *  so a Save that stops partway can mark those timers saved and the
 *  next Save does not create them again. A new timer is marked with the
 *  id the store gave it, found as the last timer holding the same name,
 *  interval, and command, since the store adds new timers at the end. */
export async function saveTimerDraft(
  draft: Draft<TimerRecord>,
  written: (write: SavedWrite<TimerRecord>) => void,
  api: TimerStoreApi = timerStore(),
): Promise<void> {
  const plan = timerSavePlan(draft);
  for (const { uid, id } of plan.remove) {
    await api.timersDelete(id);
    written({ uid, stored: null });
  }
  for (const { uid, timer, value } of plan.set) {
    const sent = { ...timer, name: timer.name.trim(), command: timer.command.trim() };
    const list = await api.timersSet(
      sent.id,
      sent.name,
      sent.interval_secs,
      sent.command,
      sent.enabled,
      sent.group ?? null,
    );
    // The store creates a timer for a null id, and also for an id it no
    // longer holds, so look the id up either way.
    const timers = list.map(normalizeTimer);
    let stored: TimerRecord = sent;
    if (sent.id === null || !timers.some((t) => t.id === sent.id)) {
      const key = timerKey(sent);
      const made = [...timers].reverse().find((t) => t.id !== null && timerKey(t) === key);
      if (made) stored = { ...sent, id: made.id };
    }
    written({ uid, stored, sent: value });
  }
}

/** An interval as the list shows it: `30 s`, `5 min`, `2 h`. */
export function formatInterval(secs: number): string {
  if (secs >= 3600 && secs % 3600 === 0) return `${secs / 3600} h`;
  if (secs >= 60 && secs % 60 === 0) return `${secs / 60} min`;
  return `${secs} s`;
}

/** A timer's list name: its name, else its command. */
export function timerLabel(timer: TimerRecord): string {
  const name = timer.name.trim();
  if (name) return name;
  return timer.command.split('\n')[0].trim();
}

/** A timer's row in the Timers list, under the heading of its group. */
export function timerEntry(timer: TimerRecord): Omit<ListEntry, 'uid'> {
  return {
    name: timerLabel(timer),
    meta: `Every ${formatInterval(timer.interval_secs)}`,
    group: groupKeyOf(timer.group),
    enabled: timer.enabled,
    text: searchText(timer.name, timer.group, timer.command),
  };
}

// ── Tick ────────────────────────────────────────────────────────────

/** The tick config with blank text fields as null, the way the
 *  backend stores them, and whole seconds of at least 1. */
export function normalizeTick(raw: TickConfig): TickConfig {
  const text = (v: string | null | undefined) => (typeof v === 'string' && v.length > 0 ? v : null);
  const secs = (v: number | null | undefined) =>
    typeof v === 'number' && Number.isFinite(v) && v > 0 ? Math.floor(v) : null;
  return {
    enabled: raw.enabled !== false,
    interval_secs: secs(raw.interval_secs) ?? 60,
    auto_fire: text(raw.auto_fire),
    sound: raw.sound === true,
    reset_pattern: text(raw.reset_pattern),
    warn_at_secs: secs(raw.warn_at_secs),
    warn_message: text(raw.warn_message),
    warn_color: text(raw.warn_color),
  };
}

// ── Presets ─────────────────────────────────────────────────────────

/** Stored in enabled_presets when you turn every preset off. An empty
 *  list already means the defaults, so none needs a value of its own.
 *  No preset has this id, so launch installs nothing for it. PRESETS_OFF
 *  in src-tauri/src/loadouts/presets.rs mirrors it, and a test there reads
 *  this line. */
export const PRESETS_OFF_MARKER = 'none';

export interface PresetToggle {
  id: string;
  enabled: boolean;
  /** What an alert preset does, its parts in the profile's `[alerts]`
   *  table. Absent on a preset of the library. */
  alert?: AlertParts;
  /** Your edits to a preset of the library, its part of the profile's
   *  `[preset_edits]` table. Absent while it holds none. */
  edit?: PresetEdit;
}

/** The presets that are on for a stored enabled_presets list. An empty
 *  list means the defaults. */
export function enabledPresetIds(stored: readonly string[]): string[] {
  const on = new Set(stored.length > 0 ? stored : defaultEnabledIds());
  return PRESETS.filter((p) => on.has(p.id)).map((p) => p.id);
}

/** One toggle per preset, in library order. */
export function presetToggles(stored: readonly string[]): PresetToggle[] {
  const on = new Set(enabledPresetIds(stored));
  return PRESETS.map((p) => ({ id: p.id, enabled: on.has(p.id) }));
}

export interface PresetLaunchPlan {
  install: string[];
  remove: string[];
}

/** What launch does with the preset triggers and macros. `installed`
 *  names the preset of every trigger and macro the stores hold, as its
 *  `preset` tag. Every preset that is on installs again, so this build's
 *  patterns replace older copies. Every preset the stores hold that is
 *  off, or that this build no longer has, comes out, so a preset you
 *  turned off stays off even when its triggers or macros came back from
 *  another profile or an older build. `switches` turn presets on and off
 *  over the stored list first. */
export function presetLaunchPlan(
  stored: readonly string[],
  installed: Iterable<string | null | undefined>,
  switches: readonly PresetSwitch[] = [],
): PresetLaunchPlan {
  const on = new Set(enabledPresetIds(stored));
  for (const s of switches) {
    if (s.on) on.add(s.id);
    else on.delete(s.id);
  }
  const install = PRESETS.filter((p) => on.has(p.id)).map((p) => p.id);
  const remove = new Set<string>();
  for (const id of installed) {
    if (id && !on.has(id)) remove.add(id);
  }
  return { install, remove: [...remove].sort() };
}

/** The keys of `preset` one of your macros keeps, in the preset's order.
 *  `macros` is every macro the store holds. Rust holds the preset's macro
 *  off on a key yours keeps (hold_taken_keys in
 *  src-tauri/src/loadouts/presets.rs), so while the store holds it, it
 *  tells. In loadout mode a macro of yours in a group the character keeps
 *  off keeps no key there. With the preset off, a key one of your macros
 *  uses is the one it would keep. */
export function keysYourMacrosKeep(preset: Preset, macros: readonly Macro[]): string[] {
  const yours = new Set(macros.filter((m) => !m.preset).map((m) => m.key));
  const theirs = new Map(macros.filter((m) => m.preset === preset.id).map((m) => [m.key, m]));
  return (preset.macros ?? [])
    .map((m) => m.key)
    .filter((key) => {
      const held = theirs.get(key);
      return held ? held.enabled === false : yours.has(key);
    });
}

/** What a preset's card says when your macros keep keys the preset
 *  wants, and the card of a preset macro held off by yours. No board
 *  draws more than one such key, so two or more share one plural
 *  sentence, the keys in the order given. */
export function keptKeyNote(held: readonly Omit<Macro, 'preset'>[]): string {
  const keys = listJoin(held.map((m) => m.key));
  const sends = listJoin(held.map((m) => m.command));
  return held.length === 1
    ? `Your macro on ${keys} keeps the key, so ${sends} has none until you move it.`
    : `Your macros on ${keys} keep their keys, so ${sends} have none until you move them.`;
}

// ── Loadouts ────────────────────────────────────────────────────────

export interface LoadoutToggle {
  name: string;
  active: boolean;
}

export function loadoutToggles(
  loadouts: readonly { name: string }[],
  active: readonly string[],
): LoadoutToggle[] {
  const on = new Set(active);
  return loadouts.map((l) => ({ name: l.name, active: on.has(l.name) }));
}

/** The active list loadouts_set_active takes. */
export function activeLoadouts(toggles: readonly LoadoutToggle[]): string[] {
  return toggles.filter((t) => t.active).map((t) => t.name);
}

// ── JSON ────────────────────────────────────────────────────────────

/** Read a JSON list and normalize each entry, or null when the text is
 *  not a JSON list. */
export function parseJsonList<T>(text: string, normalize: (raw: unknown) => T): T[] | null {
  try {
    const parsed: unknown = JSON.parse(text);
    return Array.isArray(parsed) ? parsed.map(normalize) : null;
  } catch {
    return null;
  }
}

/** A list as the JSON view shows it. */
export function jsonListText(values: readonly unknown[]): string {
  return JSON.stringify(values, null, 2);
}

// ── Errors ──────────────────────────────────────────────────────────

/** A save error as a sentence. The stores answer with terse messages,
 *  so the common ones read as what to fix. */
export function automationSaveError(error: unknown): string {
  const text = errorText(error);
  const regex = /invalid regex `([^`]*)`/.exec(text);
  if (regex) return `Vosh could not read the pattern ${regex[1]}. Fix it and save again.`;
  // tick_set_config answers a bad Reset on pattern with its own sentence,
  // which passes through below.
  if (/^invalid json/i.test(text) || /expected .* at line \d+/i.test(text)) {
    return 'Vosh could not read that list. Check it and save again.';
  }
  if (/command cannot be empty/i.test(text)) return 'Every item needs a command before you save.';
  if (/key cannot be empty/i.test(text)) return 'Press a key for every macro before you save.';
  return text || 'Vosh could not save your changes.';
}

/** An Import error as a sentence. import_apply answers with terse
 *  lowercase messages, and reading the file can fail on its own. `step`
 *  says which of the two failed. A message that already reads as a
 *  sentence passes through. */
export function importErrorMessage(error: unknown, step: 'read' | 'import'): string {
  const text = errorText(error);
  if (/could not detect import format/i.test(text)) {
    return 'Vosh could not tell which client made this file. Choose its format and import again.';
  }
  if (/unknown import format/i.test(text)) {
    return 'Vosh does not read that format. Choose one from the list and import again.';
  }
  if (/^[A-Z][^\n:;]*[.?]$/.test(text)) return text;
  return step === 'read'
    ? 'Vosh could not read that file. Choose it again or paste its contents.'
    : 'Vosh could not import that file.';
}
