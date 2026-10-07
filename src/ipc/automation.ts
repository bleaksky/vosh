// Triggers, aliases, macros and timers, their groups and presets, and
// imports from other clients.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  ALIASES_CHANGED,
  GROUPS_CHANGED,
  MACROS_CHANGED,
  MACRO_GROUPS_CHANGED,
  TIMERS_CHANGED,
  TRIGGERS_CHANGED,
} from './events';

export type TriggerAction =
  | { kind: 'highlight'; style: HighlightStyle }
  | { kind: 'gag' }
  | { kind: 'replace'; template: string }
  | { kind: 'send'; template: string }
  | { kind: 'route'; pane: string }
  | { kind: 'script'; body: string };

/** One row inside a multi-pattern trigger. Mirrors Mudlet's
 *  per-pattern editor so a user can keep, e.g., a list of mob names
 *  as separate togglable rows instead of one long pipe regex. */
export interface TriggerPattern {
  /** The regex of a Regex row. A Text or Starts with row from the store
   *  holds the regex its text compiles to, which builds up to 0.8.1
   *  read, and the store reads `text` in its place. An edit changes only
   *  `text`, and the store writes the new regex here on Save. Read and
   *  edit what you typed through `patternSource` and `withPatternSource`. */
  pattern: string;
  enabled: boolean;
  /** How the store reads the row. Left out, and on the wire, while it
   *  is 'regex'. See `MatchMode` in crates/automation/src/trigger/store.rs. */
  mode?: MatchMode;
  /** What you typed in a Text or Starts with row. A row with one of
   *  those modes and no text, as builds before the field saved it,
   *  reads `pattern` as the text, and normalizePatterns copies it here.
   *  A Regex row has none. */
  text?: string;
}

/** 'text' matches a line that is exactly the pattern, with spaces at
 *  either end of the line and of the pattern skipped. 'starts_with'
 *  matches a line that starts with it, after any spaces at the start of
 *  either, and its match runs to the end of the line. Neither has
 *  groups. 'regex' reads the pattern as typed. */
export type MatchMode = 'text' | 'starts_with' | 'regex';

export interface TriggerRecord {
  name: string;
  /** One or more patterns. The trigger fires its actions for every
   *  enabled row that matches the line. The first row is the
   *  "primary" pattern that legacy single-pattern call sites and
   *  list summaries use. */
  patterns: TriggerPattern[];
  priority: number;
  enabled: boolean;
  /** One or more actions. The trigger engine fires every action in
   *  order on each match. */
  actions: TriggerAction[];
  /** Set when this trigger was installed by the Highlights preset
   *  library. Toggling a preset off removes everything tagged with
   *  the preset's id; user-authored triggers leave this empty. */
  preset?: string | null;
  /** Optional user-defined group / folder. Triggers sharing a group
   *  can be bulk-toggled via the per-group switch in the Settings
   *  Triggers tab without losing their individual `enabled` flags.
   *  Undefined / null means ungrouped. */
  group?: string | null;
  /** Which dispatch lane the trigger runs in. 'line' (default) fires
   *  on every completed line of server output. 'prompt' fires on
   *  the partial-prompt buffer the telnet parser flushes on GA/EOR
   *  so #prompt-style triggers can capture from prompt text that
   *  arrives without a trailing newline. 'room' fires only on the
   *  lines a room look lists after its exits line, the armies, the
   *  things and the people in the room (src-tauri/src/session/room_block.rs).
   *  'room_target' fires only on the line of the person you target with
   *  `tar` among them. Omitted on the wire when the value is 'line' (the
   *  backend's default). */
  target?: TriggerTarget;
  /** The alert the trigger rings when it matches, in a table of its own
   *  beside the actions (Alerts Q6). Left out while the trigger rings
   *  none. */
  alert?: AlertParts;
}

/** What an alert does when it rings. Each part turns on and off on its
 *  own. Mirrors `AlertParts` in crates/automation/src/alert.rs, which a
 *  trigger, an alert preset and mud.alert share. */
export interface AlertParts {
  /** Post a system banner, a toast on Windows. */
  banner: boolean;
  /** The tone the main window plays, one of ALERT_TONES in
   *  src/stores/session/alertTones.ts. Left out, none plays. */
  sound?: string;
  /** Bounce the Dock icon once or until you come back, flash the
   *  taskbar on Windows, or set the urgency hint on Linux. Left out,
   *  the alert asks for none. */
  attention?: 'once' | 'until';
  /** Ring only while you are not looking at the session. */
  background: boolean;
  /** The banner shows the words of the line, and not the title alone. */
  words: boolean;
}

export type TriggerTarget = 'line' | 'prompt' | 'room' | 'room_target';

/** Read a `patterns:` list out of a raw wire-shape object, falling
 *  back to the legacy `pattern:` string the backend still emits
 *  alongside for compatibility. Always returns at least one entry. */
export function normalizePatterns(raw: unknown): TriggerPattern[] {
  if (!raw || typeof raw !== 'object') return [{ pattern: '', enabled: true }];
  const r = raw as Record<string, unknown>;
  if (Array.isArray(r.patterns) && r.patterns.length > 0) {
    return r.patterns.map((row) => {
      const rr = (row && typeof row === 'object' ? row : {}) as Record<string, unknown>;
      const out: TriggerPattern = {
        pattern: String(rr.pattern ?? ''),
        enabled: rr.enabled !== false,
      };
      // A save sends the row back as the page holds it, so the mode has
      // to ride along or the store reads the text as a regex. So does the
      // text, or the store reads the regex in `pattern` as the text. A
      // row with no text, as builds before the field saved it, takes
      // `pattern` as its text, so every such row on the page holds one
      // and a row you edit and type back matches its saved copy.
      if (rr.mode === 'text' || rr.mode === 'starts_with') {
        out.mode = rr.mode;
        out.text = typeof rr.text === 'string' ? rr.text : out.pattern;
      }
      return out;
    });
  }
  if (typeof r.pattern === 'string') {
    return [{ pattern: r.pattern, enabled: true }];
  }
  return [{ pattern: '', enabled: true }];
}

/** Normalize the legacy `action: {...}` single shape that older
 *  profile.toml entries still produce on first load. Accepts either
 *  shape and returns the canonical actions array. */
export function normalizeActions(raw: unknown): TriggerAction[] {
  if (!raw || typeof raw !== 'object') return [];
  const r = raw as Record<string, unknown>;
  if (Array.isArray(r.actions)) return r.actions as TriggerAction[];
  if (r.action && typeof r.action === 'object') return [r.action as TriggerAction];
  return [];
}

export type NamedColor =
  | 'black'
  | 'red'
  | 'green'
  | 'yellow'
  | 'blue'
  | 'magenta'
  | 'cyan'
  | 'white'
  | 'bright_black'
  | 'bright_red'
  | 'bright_green'
  | 'bright_yellow'
  | 'bright_blue'
  | 'bright_magenta'
  | 'bright_cyan'
  | 'bright_white';

export interface HighlightStyle {
  fg?: NamedColor;
  bg?: NamedColor;
  bold?: boolean;
  underline?: boolean;
  inverse?: boolean;
  /** Full-line wash: the whole line gets a dim background tint derived
   *  from the highlight color, plus a left-edge accent bar in the
   *  native renderer. */
  wash?: boolean;
  /** Base color: the style fills only the text the game left in its
   *  default color, so the colors the game puts on parts of the line
   *  stay, and other highlights draw over it. No span and no wash. */
  base?: boolean;
}

// Each call that reads or writes a profile's lists names the profile
// it means, as Settings does, or names none and reaches the profile the
// selected session plays. A name no session plays fails with the
// sentence the app gives.

export async function exportTriggers(profile?: string | null): Promise<string> {
  return invoke('triggers_export', { profile });
}

export async function importTriggers(json: string, profile?: string | null): Promise<number> {
  return invoke('triggers_import', { json, profile });
}

export async function listTriggers(): Promise<TriggerRecord[]> {
  return invoke('triggers_list');
}

export async function exportAliases(profile?: string | null): Promise<string> {
  return invoke('aliases_export', { profile });
}

export async function importAliases(json: string, profile?: string | null): Promise<number> {
  return invoke('aliases_import', { json, profile });
}

/** What a preset install did. `removed` names each stored trigger of
 *  those presets that their built set no longer carries, which the
 *  install took out. */
export interface PresetsInstalled {
  installed: number;
  removed: string[];
}

/** Install the triggers and macros of the presets that are on. Each
 *  carries its preset's id, and each preset comes whole. */
export async function presetsInstall(
  triggers: TriggerRecord[],
  macros: Macro[],
  profile?: string | null,
): Promise<PresetsInstalled> {
  return invoke('presets_install', { triggers, macros, profile });
}

export async function presetsRemove(presetId: string, profile?: string | null): Promise<number> {
  return invoke('presets_remove', { presetId, profile });
}

// Keyboard macro bindings. A Macro maps a canonical key string
// (produced by canonicalKeyFromEvent below) to a command line that
// the input layer will fire when that key combo is pressed.
export interface Macro {
  key: string;
  command: string;
  /** Optional user-defined group / folder. Like trigger/alias groups,
   *  bulk-disabled via the Settings UI without losing individual
   *  bindings. */
  group?: string | null;
  /** False keeps the binding but lets the key fall through as if it
   *  were not bound. The backend omits the field while it is on, so
   *  absent means on. */
  enabled?: boolean;
  /** The id of the preset that added it, absent for one of yours. A
   *  preset macro on a key one of yours uses comes with enabled false. */
  preset?: string | null;
}

/** One row in any groups-list response: name + current enabled state.
 *  Backend returns these sorted by name. */
export interface GroupToggle {
  name: string;
  enabled: boolean;
}

export async function listMacros(profile?: string | null): Promise<Macro[]> {
  return invoke('macros_list', { profile });
}

/** Bind or rebind a key. `enabled` turns the binding on or off. Leave
 *  it out to keep an existing binding's state, or make a new one on.
 *  With `preset`, change only the group of the macro that preset added
 *  on `key`, which leaves your macro on the same key alone. */
export async function setMacro(
  key: string,
  command: string,
  group: string | null = null,
  enabled?: boolean,
  preset?: string,
  profile?: string | null,
): Promise<Macro[]> {
  return invoke('macros_set', {
    key,
    command,
    group: group && group.length > 0 ? group : null,
    enabled: enabled ?? null,
    preset: preset ?? null,
    profile,
  });
}

export async function deleteMacro(key: string, profile?: string | null): Promise<Macro[]> {
  return invoke('macros_delete', { key, profile });
}

/** One interval timer: fire `command` every `interval_secs` seconds while
 *  connected. `id` is a stable backend-assigned handle. A timer in a
 *  group that is off waits as one that is off does. */
export interface Timer {
  id: number;
  name: string;
  interval_secs: number;
  command: string;
  enabled: boolean;
  /** Left out while the timer is in no group. */
  group?: string | null;
}

export async function timersList(profile?: string | null): Promise<Timer[]> {
  return invoke('timers_list', { profile });
}

/** Create (id null) or update (existing id) a timer. A null group puts
 *  it in none. Returns the full list. */
export async function timersSet(
  id: number | null,
  name: string,
  intervalSecs: number,
  command: string,
  enabled: boolean,
  group: string | null,
  profile?: string | null,
): Promise<Timer[]> {
  return invoke('timers_set', {
    id: id ?? null,
    name,
    intervalSecs,
    command,
    enabled,
    group,
    profile,
  });
}

export async function timersDelete(id: number, profile?: string | null): Promise<Timer[]> {
  return invoke('timers_delete', { id, profile });
}

// --- Group switches, one on each group heading in Settings, Automation ---

/** A list whose items sit in groups, named as its Automation list is. */
export type GroupList = 'triggers' | 'aliases' | 'macros' | 'timers';

/** What the loadouts decide about a group while they decide it. Every
 *  launch, profile switch and Loadouts save lays it over the group
 *  again, so the switch waits. */
export interface LoadoutHold {
  /** Whether the loadouts turn the group on. */
  on: boolean;
  /** The active loadouts that decide. Empty while every loadout is off. */
  by: string[];
}

/** One group heading's switch. */
export interface GroupSwitch {
  name: string;
  /** Whether the group is on now. */
  enabled: boolean;
  /** Set while the loadouts decide the group. */
  loadouts?: LoadoutHold;
}

/** The switch of each group in one list, sorted by name. */
export async function listGroupSwitches(
  list: GroupList,
  profile?: string | null,
): Promise<GroupSwitch[]> {
  return invoke('groups_list', { list, profile });
}

/** Turn a whole group of one list on or off. Returns every switch of the
 *  list. Fails for a group the loadouts decide. */
export async function setGroupEnabled(
  list: GroupList,
  group: string,
  enabled: boolean,
  profile?: string | null,
): Promise<GroupSwitch[]> {
  return invoke('groups_set_enabled', { list, group, enabled, profile });
}

/** A group of any list turned on or off: #group, Lua, or a switch in
 *  Settings. */
export async function subscribeGroupsChanged(cb: () => void): Promise<UnlistenFn> {
  return listen<string>(GROUPS_CHANGED, () => cb());
}

// --- Macro groups, which the command line follows ---

export async function listMacroGroups(): Promise<GroupToggle[]> {
  return invoke('macros_groups_list');
}

/** The trigger list changed: Settings saved it, or #trigger, an import,
 *  a preset, or a script edited it. */
export async function subscribeTriggersChanged(cb: () => void): Promise<UnlistenFn> {
  return listen<string>(TRIGGERS_CHANGED, () => cb());
}

/** The alias list changed: Settings saved it, or #alias, an import, or a
 *  Lua mud.alias edited it. */
export async function subscribeAliasesChanged(cb: () => void): Promise<UnlistenFn> {
  return listen<string>(ALIASES_CHANGED, () => cb());
}

export async function subscribeMacroGroupsChanged(
  cb: (group: string) => void,
): Promise<UnlistenFn> {
  return listen<string>(MACRO_GROUPS_CHANGED, (event) => {
    cb(event.payload);
  });
}

export async function subscribeMacrosChanged(cb: (macros: Macro[]) => void): Promise<UnlistenFn> {
  return listen<Macro[]>(MACROS_CHANGED, (event) => {
    cb(event.payload);
  });
}

export async function subscribeTimersChanged(cb: (timers: Timer[]) => void): Promise<UnlistenFn> {
  return listen<Timer[]>(TIMERS_CHANGED, (event) => {
    cb(event.payload);
  });
}

// Multi-format config importer. `format` is "mushclient", "mudlet",
// "gmud", or "" to auto-detect. Returns counts + the unsupported
// items the backend could not model.
export type ImportFormat = 'mushclient' | 'mudlet' | 'gmud' | 'cmud' | '';

export interface ImportSummary {
  aliases: number;
  triggers: number;
  macros: number;
  vars: number;
  unsupported: [string, string][];
  unparsed: string[];
  rejected: string[];
}

export async function detectImportFormat(text: string): Promise<string | null> {
  return invoke('import_detect', { text });
}

export async function applyImport(
  format: ImportFormat,
  text: string,
  profile?: string | null,
): Promise<ImportSummary> {
  return invoke('import_apply', { format, text, profile });
}
