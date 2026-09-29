// Aliases, macros, timers, presets, loadouts, and the tick as the
// Automation page edits them. Each normalizer turns what the backend
// sends into one shape, so two values that save the same way compare
// equal. Each save plan turns a draft into the calls the existing API
// takes: aliases replace the whole store with the draft's changes
// applied over it, macros and timers go item by item, presets install
// and remove triggers, and loadouts set the active list.

import { draftChanges, saveDraftOnto, type Draft } from './automationDraft';
import { defaultEnabledIds, PRESETS } from './presets';
import { exportAliases, importAliases, type TickConfig } from './session';

const quote = (name: string) => `“${name}”`;

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

const ALIAS_STORE: AliasStoreApi = { exportAliases, importAliases };

/** Every alias the store holds, for display. A reply that does not read
 *  shows as no aliases. */
export async function loadAliases(api: AliasStoreApi = ALIAS_STORE): Promise<AliasRecord[]> {
  return parseJsonList(await api.exportAliases(), normalizeAlias) ?? [];
}

/** Save the Aliases draft over the store as it stands now, like
 *  saveTriggerDraft. An alias #alias or a script added after the page
 *  loaded survives, and a list that does not read stops the save. */
export async function saveAliasDraft(
  draft: Draft<AliasRecord>,
  api: AliasStoreApi = ALIAS_STORE,
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
    if (seen.has(name)) return `Two aliases are named ${quote(name)}. Give each one its own name.`;
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
  return out;
}

export function blankMacro(): MacroRecord {
  return { key: '', command: '', enabled: true };
}

export function validateMacros(list: readonly MacroRecord[]): string | null {
  const seen = new Set<string>();
  for (const m of list) {
    if (!m.key) return 'Press a key for every macro before you save.';
    if (seen.has(m.key)) return `Two macros use ${m.key}. Give each one its own key.`;
    seen.add(m.key);
    if (!m.command.trim()) return `The macro on ${m.key} needs a command.`;
  }
  return null;
}

export interface MacroSavePlan {
  /** Keys to unbind first. */
  remove: string[];
  /** Bindings to set after that. */
  set: MacroRecord[];
}

/** The macros_delete and macros_set calls that make the store match
 *  the draft. A macro whose key changed unbinds the old key. Unbinding
 *  runs first, so a key another macro takes over ends up bound. */
export function macroSavePlan(draft: Draft<MacroRecord>): MacroSavePlan {
  const { added, removed, changed } = draftChanges(draft);
  const remove: string[] = removed.map((item) => item.value.key);
  const set: MacroRecord[] = added.map((item) => item.value);
  for (const { before, after } of changed) {
    if (before.key !== after.key) remove.push(before.key);
    set.push(after);
  }
  return { remove: [...new Set(remove)], set };
}

// ── Timers ──────────────────────────────────────────────────────────

export interface TimerRecord {
  /** The backend's id, or null for a timer the page made. */
  id: number | null;
  name: string;
  interval_secs: number;
  command: string;
  enabled: boolean;
}

export function normalizeTimer(raw: unknown): TimerRecord {
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  const interval = typeof r.interval_secs === 'number' ? Math.floor(r.interval_secs) : 30;
  return {
    id: typeof r.id === 'number' ? r.id : null,
    name: typeof r.name === 'string' ? r.name : '',
    interval_secs: Math.max(1, Number.isFinite(interval) ? interval : 30),
    command: typeof r.command === 'string' ? r.command : '',
    enabled: r.enabled !== false,
  };
}

export function blankTimer(): TimerRecord {
  return { id: null, name: '', interval_secs: 30, command: '', enabled: true };
}

export function validateTimers(list: readonly TimerRecord[]): string | null {
  for (const t of list) {
    if (!t.command.trim()) {
      return t.name.trim()
        ? `The timer ${quote(t.name.trim())} needs a command.`
        : 'Every timer needs a command before you save.';
    }
  }
  return null;
}

export interface TimerSavePlan {
  remove: number[];
  /** Timers to create (id null) or update. */
  set: TimerRecord[];
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
  const remove = removed.map((item) => item.value.id).filter((id): id is number => id !== null);
  const set = [
    ...changed.map((c) => ({ ...c.after, id: c.before.id })),
    ...added.map((item) => ({ ...item.value, id: null })),
  ];
  return { remove, set };
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
 *  No preset has this id, so launch installs nothing for it. */
export const PRESETS_OFF_MARKER = 'none';

export interface PresetToggle {
  id: string;
  enabled: boolean;
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

/** What to store in enabled_presets for these toggles. */
export function storedPresetIds(toggles: readonly PresetToggle[]): string[] {
  const on = toggles.filter((t) => t.enabled).map((t) => t.id);
  return on.length > 0 ? on : [PRESETS_OFF_MARKER];
}

export interface PresetSavePlan {
  install: string[];
  remove: string[];
}

export function presetSavePlan(draft: Draft<PresetToggle>): PresetSavePlan {
  const { changed } = draftChanges(draft);
  return {
    install: changed.filter((c) => c.after.enabled).map((c) => c.after.id),
    remove: changed.filter((c) => !c.after.enabled).map((c) => c.after.id),
  };
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
  const text = String(error instanceof Error ? error.message : error).trim();
  const regex = /invalid regex `([^`]*)`/.exec(text);
  if (regex) return `Vosh could not read the pattern ${regex[1]}. Fix it and save again.`;
  // tick_set_config adds the regex crate's own report, several lowercase
  // lines with a caret under the fault. The field name says enough.
  if (/^invalid reset pattern/i.test(text)) {
    return 'Vosh could not read the Reset on pattern. Fix it and save again.';
  }
  if (/^invalid json/i.test(text) || /expected .* at line \d+/i.test(text)) {
    return 'Vosh could not read that list. Check it and save again.';
  }
  if (/command cannot be empty/i.test(text)) return 'Every item needs a command before you save.';
  if (/key cannot be empty/i.test(text)) return 'Press a key for every macro before you save.';
  return text || 'Vosh could not save your changes.';
}
