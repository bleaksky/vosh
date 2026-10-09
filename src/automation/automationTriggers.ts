// Trigger logic for Settings, Automation. The trigger card shows a
// trigger as a name, a group, one pattern, a Style, and one command to
// send. A stored trigger holds a list of patterns and a list of
// actions, so these helpers read and write the card's fields as views
// over those lists. An edit changes the action it names in place and
// leaves the rest of the list, and its order, as it was.

import { normalizeAlert } from './alertParts';
import { saveDraftOnto, type Draft } from './automationDraft';
import { parseJsonList } from './automationRecords';
import { colorize, decolorize } from './colorTokens';
import { PRESETS } from './presets';
import {
  exportTriggers,
  importTriggers,
  normalizeActions,
  normalizePatterns,
  type HighlightStyle,
  type MatchMode,
  type NamedColor,
  type TriggerAction,
  type TriggerPattern,
  type TriggerRecord,
} from '../ipc/automation';
import { quoted } from '../lib/text';

/** The Style select on the trigger card. */
export type TriggerStyle = 'none' | 'highlight' | 'wash' | 'replace' | 'hide';

export const TRIGGER_STYLE_OPTIONS: readonly { value: TriggerStyle; label: string }[] = [
  { value: 'none', label: 'None' },
  { value: 'highlight', label: 'Highlight' },
  { value: 'wash', label: 'Wash' },
  { value: 'replace', label: 'Replace' },
  { value: 'hide', label: 'Hide' },
];

type Visual = Extract<TriggerAction, { kind: 'highlight' } | { kind: 'gag' } | { kind: 'replace' }>;
type EffectKind = 'send' | 'route' | 'script';

export function isVisualAction(action: TriggerAction): action is Visual {
  return action.kind === 'highlight' || action.kind === 'gag' || action.kind === 'replace';
}

function visualIndex(actions: readonly TriggerAction[]): number {
  return actions.findIndex(isVisualAction);
}

/** The color a new highlight starts with. */
export const DEFAULT_HIGHLIGHT: HighlightStyle = { fg: 'yellow' };

/** The Style a trigger's first visual action reads as: none, a
 *  highlight, a highlight with the full line wash, a replace, or a gag
 *  (Hide). */
export function triggerStyle(actions: readonly TriggerAction[]): TriggerStyle {
  const visual = actions.find(isVisualAction);
  if (!visual) return 'none';
  if (visual.kind === 'gag') return 'hide';
  if (visual.kind === 'replace') return 'replace';
  return visual.style.wash ? 'wash' : 'highlight';
}

function setVisual(actions: readonly TriggerAction[], next: Visual | null): TriggerAction[] {
  const index = visualIndex(actions);
  const out = actions.slice();
  if (index === -1) {
    if (next) out.unshift(next);
    return out;
  }
  if (next) out[index] = next;
  else out.splice(index, 1);
  return out;
}

/** Set the Style. Moving between Highlight and Wash keeps the colors,
 *  and picking the Style a trigger already has changes nothing. */
export function withTriggerStyle(
  actions: readonly TriggerAction[],
  style: TriggerStyle,
): TriggerAction[] {
  if (triggerStyle(actions) === style) return actions.slice();
  const current = actions.find(isVisualAction);
  switch (style) {
    case 'none':
      return setVisual(actions, null);
    case 'hide':
      return setVisual(actions, { kind: 'gag' });
    case 'replace':
      return setVisual(actions, { kind: 'replace', template: '' });
    case 'highlight':
    case 'wash': {
      const base: HighlightStyle =
        current?.kind === 'highlight' ? { ...current.style } : { ...DEFAULT_HIGHLIGHT };
      if (style === 'wash') base.wash = true;
      else delete base.wash;
      return setVisual(actions, { kind: 'highlight', style: base });
    }
  }
}

/** The highlight colors and flags, or null when the Style is not a
 *  highlight. */
export function highlightOf(actions: readonly TriggerAction[]): HighlightStyle | null {
  const visual = actions.find(isVisualAction);
  return visual?.kind === 'highlight' ? visual.style : null;
}

/** A change to highlight colors or flags. Undefined clears a field. */
export type HighlightPatch = { [K in keyof HighlightStyle]?: HighlightStyle[K] | undefined };

/** Change highlight colors or flags. A field set to undefined or false
 *  leaves the stored style. Does nothing unless the Style is a
 *  highlight. */
export function withHighlight(
  actions: readonly TriggerAction[],
  patch: HighlightPatch,
): TriggerAction[] {
  const current = highlightOf(actions);
  if (!current) return actions.slice();
  const style: HighlightStyle = { ...current };
  for (const key of Object.keys(patch) as (keyof HighlightStyle)[]) {
    const value = patch[key];
    if (value === undefined || value === false) delete style[key];
    else (style as Record<string, unknown>)[key] = value;
  }
  return setVisual(actions, { kind: 'highlight', style });
}

/** The Replace template, or an empty string. */
export function replaceTemplateOf(actions: readonly TriggerAction[]): string {
  const visual = actions.find(isVisualAction);
  return visual?.kind === 'replace' ? visual.template : '';
}

export function withReplaceTemplate(
  actions: readonly TriggerAction[],
  template: string,
): TriggerAction[] {
  if (triggerStyle(actions) !== 'replace') return actions.slice();
  return setVisual(actions, { kind: 'replace', template });
}

function effectIndex(actions: readonly TriggerAction[], kind: EffectKind): number {
  return actions.findIndex((a) => a.kind === kind);
}

function effectValue(action: TriggerAction | undefined): string {
  if (!action) return '';
  if (action.kind === 'send') return action.template;
  if (action.kind === 'route') return action.pane;
  if (action.kind === 'script') return action.body;
  return '';
}

function makeEffect(kind: EffectKind, value: string): TriggerAction {
  if (kind === 'send') return { kind: 'send', template: value };
  if (kind === 'route') return { kind: 'route', pane: value };
  return { kind: 'script', body: value };
}

/** The first effect of a kind as text: the command a send sends, the
 *  pane a route copies to, or a script's Lua body. */
export function effectOf(actions: readonly TriggerAction[], kind: EffectKind): string {
  return effectValue(actions[effectIndex(actions, kind)]);
}

/** Set the first effect of a kind. An empty value removes it, and a
 *  value where there was none adds it at the end. */
export function withEffect(
  actions: readonly TriggerAction[],
  kind: EffectKind,
  value: string,
): TriggerAction[] {
  const index = effectIndex(actions, kind);
  const out = actions.slice();
  if (value === '') {
    if (index !== -1) out.splice(index, 1);
    return out;
  }
  const next = makeEffect(kind, value);
  if (index === -1) out.push(next);
  else out[index] = next;
  return out;
}

/** Effects past the first of their kind, with their place in the
 *  action list. Imports and the JSON view can make these, and the
 *  Advanced rows keep them in reach. */
export function extraEffects(
  actions: readonly TriggerAction[],
): { index: number; kind: EffectKind; value: string }[] {
  const seen = new Set<string>();
  const out: { index: number; kind: EffectKind; value: string }[] = [];
  actions.forEach((action, index) => {
    if (action.kind !== 'send' && action.kind !== 'route' && action.kind !== 'script') return;
    if (!seen.has(action.kind)) {
      seen.add(action.kind);
      return;
    }
    out.push({ index, kind: action.kind, value: effectValue(action) });
  });
  return out;
}

/** Change or remove (null) the effect at `index`. */
export function withEffectAt(
  actions: readonly TriggerAction[],
  index: number,
  value: string | null,
): TriggerAction[] {
  const action = actions[index];
  if (!action || (action.kind !== 'send' && action.kind !== 'route' && action.kind !== 'script')) {
    return actions.slice();
  }
  const out = actions.slice();
  if (value === null) out.splice(index, 1);
  else out[index] = makeEffect(action.kind, value);
  return out;
}

/** The main pattern, the first row. */
export function mainPattern(trigger: TriggerRecord): TriggerPattern {
  return trigger.patterns[0] ?? { pattern: '', enabled: true };
}

export function withMainPattern(
  trigger: TriggerRecord,
  patch: Partial<TriggerPattern>,
): TriggerRecord {
  const patterns = trigger.patterns.length > 0 ? trigger.patterns.slice() : [mainPattern(trigger)];
  patterns[0] = { ...patterns[0], ...patch };
  return { ...trigger, patterns };
}

function isTextRow(row: TriggerPattern): boolean {
  return row.mode === 'text' || row.mode === 'starts_with';
}

/** What you typed in a row, the one the Pattern fields show: the text of
 *  a Text or Starts with row, the regex of a Regex row. */
export function patternSource(row: TriggerPattern): string {
  return isTextRow(row) ? (row.text ?? row.pattern) : row.pattern;
}

/** A row with what you typed set to `value`. A Text or Starts with row
 *  takes it in `text` and keeps `pattern` as the store sent it. The store
 *  reads `text` and writes the new regex in `pattern` when it saves, and
 *  a row you type back as it was matches its saved copy, so the page
 *  reads as saved again. */
export function withPatternSource(row: TriggerPattern, value: string): TriggerPattern {
  return isTextRow(row) ? { ...row, text: value } : { ...row, pattern: value };
}

/** The modes the Pattern row offers, in the order it lists them. */
export const MATCH_MODE_OPTIONS: readonly { value: MatchMode; label: string }[] = [
  { value: 'text', label: 'Text' },
  { value: 'starts_with', label: 'Starts with' },
  { value: 'regex', label: 'Regex' },
];

/** What each mode does, the line under the Pattern label. */
export const MATCH_MODE_DESCRIPTIONS: Readonly<Record<MatchMode, string>> = {
  text: 'Matches a line that is exactly this text.',
  starts_with: 'Matches any line that starts with this text.',
  regex: 'A regular expression. Groups fill $1 and on.',
};

/** The mode the Pattern row shows: the main row's, or Regex when it has
 *  none, as the store reads a row with no mode. */
export function triggerMode(trigger: TriggerRecord): MatchMode {
  return mainPattern(trigger).mode ?? 'regex';
}

/** A row read in another mode, keeping what you typed. A Regex row has
 *  no mode or text on the wire. A Text or Starts with row takes what you
 *  typed in `text`, and the store writes the regex it compiles to in
 *  `pattern` when it saves. A row that was already Text or Starts with
 *  keeps the regex the store sent, so moving between those two and back
 *  reads as saved, and so does Regex to Text and back. */
export function withPatternMode(row: TriggerPattern, mode: MatchMode): TriggerPattern {
  const source = patternSource(row);
  if (mode === 'regex') return { pattern: source, enabled: row.enabled };
  return {
    pattern: isTextRow(row) ? row.pattern : source,
    enabled: row.enabled,
    mode,
    text: source,
  };
}

/** Read every pattern in `mode`, More patterns included, since one
 *  control covers them all. */
export function withTriggerMode(trigger: TriggerRecord, mode: MatchMode): TriggerRecord {
  const patterns = trigger.patterns.length > 0 ? trigger.patterns : [mainPattern(trigger)];
  return { ...trigger, patterns: patterns.map((row) => withPatternMode(row, mode)) };
}

/** An empty pattern row in `mode`. */
export function blankPattern(mode: MatchMode): TriggerPattern {
  return withPatternMode({ pattern: '', enabled: true }, mode);
}

/** Set what you typed in the main pattern. */
export function withMainPatternSource(trigger: TriggerRecord, value: string): TriggerRecord {
  return withMainPattern(trigger, withPatternSource(mainPattern(trigger), value));
}

/** Set a trigger's group. Blank means no group. */
export function withGroup<T extends { group?: string | null }>(item: T, group: string): T {
  const trimmed = group.trim();
  const next = { ...item };
  if (trimmed) next.group = trimmed;
  else delete next.group;
  return next;
}

/** Every named color the highlight selects offer, with its label. */
export const HIGHLIGHT_COLORS: readonly { value: NamedColor; label: string }[] = (
  [
    'black',
    'red',
    'green',
    'yellow',
    'blue',
    'magenta',
    'cyan',
    'white',
    'bright_black',
    'bright_red',
    'bright_green',
    'bright_yellow',
    'bright_blue',
    'bright_magenta',
    'bright_cyan',
    'bright_white',
  ] as const
).map((value) => {
  const words = value.replace('_', ' ');
  return { value, label: words.charAt(0).toUpperCase() + words.slice(1) };
});

/** A stored trigger as the editor holds it: every field present in one
 *  shape, a blank group left out, color codes in send and replace
 *  templates shown as `{red}` tokens, and the alert table read as Rust
 *  reads it. */
export function normalizeTrigger(raw: unknown): TriggerRecord {
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  const out: TriggerRecord = {
    name: typeof r.name === 'string' ? r.name : String(r.name ?? ''),
    patterns: normalizePatterns(r),
    priority: typeof r.priority === 'number' && Number.isFinite(r.priority) ? r.priority : 0,
    enabled: r.enabled !== false,
    actions: normalizeActions(r).map((a) =>
      a.kind === 'replace' || a.kind === 'send' ? { ...a, template: decolorize(a.template) } : a,
    ),
  };
  if (typeof r.preset === 'string' && r.preset.length > 0) out.preset = r.preset;
  const group = typeof r.group === 'string' ? r.group.trim() : '';
  if (group) out.group = group;
  if (r.target === 'prompt' || r.target === 'room' || r.target === 'room_target') {
    out.target = r.target;
  }
  const alert = normalizeAlert(r.alert);
  if (alert) out.alert = alert;
  return out;
}

/** The wire form Save sends: tokens back to color codes. */
export function triggerForSave(trigger: TriggerRecord): TriggerRecord {
  return {
    ...trigger,
    actions: trigger.actions.map((a) =>
      a.kind === 'replace' || a.kind === 'send' ? { ...a, template: colorize(a.template) } : a,
    ),
  };
}

/** A new trigger: no name, one empty Text pattern, Style None. */
export function blankTrigger(): TriggerRecord {
  return {
    name: '',
    patterns: [blankPattern('text')],
    priority: 5,
    enabled: true,
    actions: [],
  };
}

/** Why the triggers cannot save yet, or null. Every trigger needs a
 *  name, names must differ (the store keys by name, so a second one
 *  would replace the first), and a trigger needs a pattern. A trigger of
 *  yours never takes the name of a trigger in the preset library, its
 *  preset on or off, since the next launch would put the preset's in its
 *  place. */
export function validateTriggers(list: readonly TriggerRecord[]): string | null {
  for (const t of list) {
    if (t.preset) continue;
    const name = t.name.trim();
    const preset = PRESETS.find((p) => p.triggers.some((pt) => pt.name === name));
    if (preset) return `${preset.name} uses the name ${name}. Give your trigger its own name.`;
  }
  const seen = new Set<string>();
  for (const t of list) {
    const name = t.name.trim();
    if (!name) return 'Give every trigger a name before you save.';
    if (seen.has(name)) {
      return `Two triggers are named ${quoted(name)}. Give each one its own name.`;
    }
    seen.add(name);
    if (!t.patterns.some((p) => patternSource(p).trim().length > 0)) {
      return `The trigger ${quoted(name)} needs a pattern.`;
    }
  }
  return null;
}

/** A trigger's identity. The store keys triggers by name. */
export const triggerKey = (trigger: TriggerRecord): string => trigger.name;

/** The two calls the trigger store takes a whole list through. */
export interface TriggerStoreApi {
  exportTriggers: () => Promise<string>;
  importTriggers: (json: string) => Promise<unknown>;
}

/** The trigger store of `profile`, or of the profile the selected
 *  session plays when it names none. */
export function triggerStore(profile?: string | null): TriggerStoreApi {
  return {
    exportTriggers: () => exportTriggers(profile),
    importTriggers: (json) => importTriggers(json, profile),
  };
}

/** Every trigger the store holds, for display. A reply that does not
 *  read shows as no triggers. */
export async function loadTriggers(
  api: TriggerStoreApi = triggerStore(),
): Promise<TriggerRecord[]> {
  return parseJsonList(await api.exportTriggers(), normalizeTrigger) ?? [];
}

// The store's list as it wrote it, each field kept, so a change to one
// field writes every other back untouched. Throws when it does not read.
async function storedList(api: TriggerStoreApi): Promise<unknown[]> {
  let list: unknown;
  try {
    list = JSON.parse(await api.exportTriggers());
  } catch {
    list = null;
  }
  if (!Array.isArray(list)) {
    throw new Error('Vosh could not read your saved triggers, so it changed nothing.');
  }
  return list;
}

type Stored = { name?: unknown; group?: string | null };
const isStored = (t: unknown): t is Stored => t !== null && typeof t === 'object';

/** Set the trigger named `name` to match Prompts, as Match does in the
 *  Triggers editor. The prompt card offers it for a Line trigger that
 *  matched your prompt as a line, which no longer sees it once the
 *  profile reads your prompt. It reads the store's list again and writes
 *  it back with only that trigger's target changed, every other field as
 *  the store wrote it. */
export async function moveTriggerToPrompts(
  name: string,
  api: TriggerStoreApi = triggerStore(),
): Promise<void> {
  const list = await storedList(api);
  const at = list.findIndex((t) => isStored(t) && t.name === name);
  if (at < 0) throw new Error(`Vosh no longer has a trigger named ${quoted(name)}.`);
  const next = [...list];
  next[at] = { ...(list[at] as Stored), target: 'prompt' };
  await api.importTriggers(JSON.stringify(next, null, 2));
}

/** Put each stored trigger `groups` names in its group there, blank for
 *  none, every other field as the store wrote it. A name the store does
 *  not hold is passed over, and with none to change it writes nothing. */
export async function setTriggerGroups(
  groups: ReadonlyMap<string, string>,
  api: TriggerStoreApi = triggerStore(),
): Promise<void> {
  const list = await storedList(api);
  let changed = false;
  const next = list.map((t) => {
    const group = isStored(t) ? groups.get(String(t.name)) : undefined;
    if (!isStored(t) || group === undefined) return t;
    const moved = withGroup(t, group);
    changed ||= moved.group !== t.group;
    return moved;
  });
  if (changed) await api.importTriggers(JSON.stringify(next, null, 2));
}

/** Save the Triggers draft. The store takes a whole list, so Save reads
 *  it again first and writes it back with only the draft's additions,
 *  changes, and removals applied. A trigger #trigger or a script added
 *  after the page loaded survives. When the store's list does not read,
 *  Save writes nothing, since writing would drop what it could not read. */
export async function saveTriggerDraft(
  draft: Draft<TriggerRecord>,
  api: TriggerStoreApi = triggerStore(),
): Promise<void> {
  await saveDraftOnto(
    draft,
    {
      read: async () => {
        const list = parseJsonList(await api.exportTriggers(), normalizeTrigger);
        if (!list) throw new Error('Vosh could not read your saved triggers, so it saved nothing.');
        return list;
      },
      write: async (values) => {
        await api.importTriggers(JSON.stringify(values.map(triggerForSave), null, 2));
      },
    },
    triggerKey,
  );
}
