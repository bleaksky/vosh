// Aliases, macros, timers, presets, loadouts, and the tick as the
// Automation page edits them. Each normalizer turns what the backend
// sends into one shape, so two values that save the same way compare
// equal. Each save plan turns a draft into the calls the existing API
// takes: aliases replace the whole store, macros and timers go item by
// item, presets install and remove triggers, and loadouts set the
// active list.

import { draftChanges, type Draft } from './automationDraft';
import { defaultEnabledIds, PRESETS } from './presets';
import type { TickConfig } from './session';

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

export function timerSavePlan(draft: Draft<TimerRecord>): TimerSavePlan {
  const { added, removed, changed } = draftChanges(draft);
  const remove = removed.map((item) => item.value.id).filter((id): id is number => id !== null);
  const set = [...changed.map((c) => c.after), ...added.map((item) => item.value)];
  return { remove, set };
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

// ── Errors ──────────────────────────────────────────────────────────

/** A save error as a sentence. The stores answer with terse messages,
 *  so the common ones read as what to fix. */
export function automationSaveError(error: unknown): string {
  const text = String(error instanceof Error ? error.message : error).trim();
  const regex = /invalid regex `([^`]*)`/.exec(text);
  if (regex) return `Vosh could not read the pattern ${regex[1]}. Fix it and save again.`;
  if (/^invalid json/i.test(text) || /expected .* at line \d+/i.test(text)) {
    return 'Vosh could not read that list. Check it and save again.';
  }
  if (/command cannot be empty/i.test(text)) return 'Every item needs a command before you save.';
  if (/key cannot be empty/i.test(text)) return 'Press a key for every macro before you save.';
  return text || 'Vosh could not save your changes.';
}
