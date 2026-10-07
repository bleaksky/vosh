// Your edits to the presets, laid over the presets as they ship (the
// Presets review, Q1, Q3 and Q5). An edit keeps each row you changed
// alone, as your value and the preset's value you changed it from, so a
// fix to the preset still reaches every row you left alone, and a fix to
// a row you changed shows as one.
//
// A trigger's rows key by field name, as enabled or send. A row in a list
// keys by what the preset holds there, a pattern by its text and each
// Also send by the preset's command, so an edit stays on its row when a
// fix adds one. Replace with keeps the preset's color keys, as {line},
// and so does a highlight color, so a swatch still reaches a trigger you
// edited and a swatch change never reads as a fix.

import {
  effectOf,
  extraEffects,
  HIGHLIGHT_COLORS,
  highlightOf,
  patternSource,
  replaceTemplateOf,
  triggerMode,
  triggerStyle,
  withEffect,
  withEffectAt,
  withHighlight,
  withPatternMode,
  withPatternSource,
  withReplaceTemplate,
  withTriggerMode,
  withTriggerStyle,
  type HighlightPatch,
  type TriggerStyle,
} from './automationTriggers';
import { countPhrase } from './automationDraft';
import { fillColors, presetById, type Preset, type PresetTrigger } from './presets';
import { listJoin } from '../lib/text';
import { indexedRgb, toHex } from '../theme/color';
import type {
  AlertParts,
  MatchMode,
  TriggerAction,
  TriggerPattern,
  TriggerRecord,
  TriggerTarget,
} from '../ipc/automation';
import type { EditRow, EditValue, PresetEdit } from '../ipc/presetEdits';

const PATTERN = 'pattern:';
const COLOR_NOUN = { one: 'color', many: 'colors' };
const TRIGGER_NOUN = { one: 'trigger', many: 'triggers' };
const ALSO = 'also:';

/** The key of the pattern row whose text the preset holds as `text`. */
export const patternKey = (text: string): string => `${PATTERN}${text}`;

/** The key of the Also send row whose command the preset holds as
 *  `command`. */
export const alsoKey = (command: string): string => `${ALSO}${command}`;

const isListKey = (key: string) => key.startsWith(PATTERN) || key.startsWith(ALSO);

/** What a row in a list holds where the list has no such row: the
 *  preset's value of a row you added, and your value of a row you took
 *  out. Then send, Send to pane, Lua, Group and the alert hold it while
 *  they are empty too. */
export const NO_ROW = '';

/** Each row of a trigger by its key. */
export type Rows = Record<string, EditValue>;

const NAMED = new Set<string>(HIGHLIGHT_COLORS.map((c) => c.value));

// A highlight color as its row holds it: a key of the preset's colors in
// braces, as {line}, or a named color of your own.
function colorRow(color: string | undefined): string {
  if (!color) return NO_ROW;
  return NAMED.has(color) ? color : `{${color}}`;
}

function colorOfRow(value: EditValue): string | undefined {
  if (typeof value !== 'string' || value === NO_ROW) return undefined;
  return /^\{(.+)\}$/.exec(value)?.[1] ?? value;
}

// A preset trigger read as a trigger record. Its highlight names its
// color by key, which the helpers carry as they find it.
const asRecord = (t: PresetTrigger) => t as unknown as TriggerRecord;
const asPreset = (t: TriggerRecord) => t as unknown as PresetTrigger;

// A value with every key it leaves empty dropped, since a TOML row holds
// no null.
function plain(value: object): EditValue {
  return JSON.parse(JSON.stringify(value, (_, v: unknown) => (v === null ? undefined : v)));
}

/** Whether two rows hold the same value, the keys of a table in any
 *  order. */
export function same(a: EditValue | undefined, b: EditValue | undefined): boolean {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object') return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    return (
      Array.isArray(a) &&
      Array.isArray(b) &&
      a.length === b.length &&
      a.every((v, i) => same(v, b[i]))
    );
  }
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => same(a[k], b[k]));
}

/** Every row of a preset trigger, keyed as its edits key them. */
export function triggerRows(trigger: PresetTrigger): Rows {
  const t = asRecord(trigger);
  const highlight = highlightOf(t.actions);
  const rows: Rows = {
    enabled: t.enabled,
    priority: t.priority,
    group: t.group ?? NO_ROW,
    target: t.target ?? 'line',
    mode: triggerMode(t),
    style: triggerStyle(t.actions),
    replace: replaceTemplateOf(t.actions),
    fg: colorRow(highlight?.fg),
    bg: colorRow(highlight?.bg),
    bold: highlight?.bold === true,
    underline: highlight?.underline === true,
    inverse: highlight?.inverse === true,
    base: highlight?.base === true,
    send: effectOf(t.actions, 'send'),
    route: effectOf(t.actions, 'route'),
    script: effectOf(t.actions, 'script'),
    alert: t.alert ? plain(t.alert) : NO_ROW,
  };
  for (const effect of extraEffects(t.actions)) {
    if (effect.kind === 'send') rows[alsoKey(effect.value)] = effect.value;
  }
  for (const row of t.patterns) {
    const text = patternSource(row);
    rows[patternKey(text)] = { text, enabled: row.enabled };
  }
  return rows;
}

/** The preset's value now of the row `key`: what `now`, the preset's
 *  rows, holds there, NO_ROW for a row in a list it no longer has, or
 *  undefined for a row it no longer has at all. */
export function nowOf(now: Rows, key: string): EditValue | undefined {
  if (Object.hasOwn(now, key)) return now[key];
  return isListKey(key) ? NO_ROW : undefined;
}

function withAlso(actions: TriggerAction[], command: string, value: EditValue): TriggerAction[] {
  const at = extraEffects(actions).find((e) => e.kind === 'send' && e.value === command);
  if (at) return withEffectAt(actions, at.index, value === NO_ROW ? null : String(value));
  return value === NO_ROW ? actions : [...actions, { kind: 'send', template: String(value) }];
}

function withPatternRow(
  patterns: TriggerPattern[],
  text: string,
  value: EditValue,
  mode: MatchMode,
): TriggerPattern[] {
  const at = patterns.findIndex((row) => patternSource(row) === text);
  if (value === NO_ROW) return at === -1 ? patterns : patterns.filter((_, i) => i !== at);
  const row = value as { text?: EditValue; enabled?: EditValue };
  const source = String(row.text ?? '');
  const enabled = row.enabled !== false;
  if (at === -1) return [...patterns, withPatternMode({ pattern: source, enabled }, mode)];
  return patterns.map((p, i) => (i === at ? { ...withPatternSource(p, source), enabled } : p));
}

/** `trigger` with `values` laid over it, each row by its key. A row
 *  `values` leaves out stays as the trigger has it, so with none the
 *  trigger comes back as it was. */
export function applyRows(trigger: PresetTrigger, values: Readonly<Rows>): PresetTrigger {
  const has = (key: string) => Object.hasOwn(values, key);
  const t = asRecord(trigger);
  let actions = t.actions;
  if (has('style')) actions = withTriggerStyle(actions, values.style as TriggerStyle);
  if (has('replace')) actions = withReplaceTemplate(actions, String(values.replace));
  const patch: HighlightPatch = {};
  if (has('fg')) patch.fg = colorOfRow(values.fg) as HighlightPatch['fg'];
  if (has('bg')) patch.bg = colorOfRow(values.bg) as HighlightPatch['bg'];
  for (const flag of ['bold', 'underline', 'inverse', 'base'] as const) {
    if (has(flag)) patch[flag] = values[flag] === true;
  }
  if (Object.keys(patch).length > 0) actions = withHighlight(actions, patch);
  // Each Also send first, so it finds its command among the sends past
  // the first while Then send still holds its place.
  let patterns = t.patterns;
  const mode = triggerMode(t);
  for (const [key, value] of Object.entries(values)) {
    if (key.startsWith(ALSO)) actions = withAlso(actions, key.slice(ALSO.length), value);
    if (key.startsWith(PATTERN)) {
      patterns = withPatternRow(patterns, key.slice(PATTERN.length), value, mode);
    }
  }
  for (const kind of ['send', 'route', 'script'] as const) {
    if (has(kind)) actions = withEffect(actions, kind, String(values[kind]));
  }
  let out: TriggerRecord = { ...t, actions, patterns };
  if (has('mode')) out = withTriggerMode(out, values.mode as MatchMode);
  if (has('enabled')) out.enabled = values.enabled === true;
  if (has('priority')) out.priority = Number(values.priority);
  if (has('group')) {
    if (values.group === NO_ROW) delete out.group;
    else out.group = String(values.group);
  }
  if (has('target')) {
    if (values.target === 'line') delete out.target;
    else out.target = values.target as TriggerTarget;
  }
  if (has('alert')) {
    if (values.alert === NO_ROW) delete out.alert;
    else out.alert = values.alert as unknown as AlertParts;
  }
  return asPreset(out);
}

/** What an edit does against the preset as it is now. It applies while
 *  its `was` still matches, folds away once it equals the preset, is
 *  flagged where a fix changed its row and it stays yours, and is a
 *  removed row where the preset no longer has its key. */
export type Hold = 'applies' | 'folds' | 'flagged' | 'removed';

/** Hold the edit of the row `key` against `now`, the preset's value of
 *  that row now, undefined when it no longer has the row. */
export function hold(key: string, row: EditRow, now: EditValue | undefined): Hold {
  if (now === undefined) return 'removed';
  if (same(row.value, now)) return 'folds';
  if (same(row.was, now)) return 'applies';
  if (isListKey(key) && now === NO_ROW) return 'removed';
  return 'flagged';
}

/** Whether a launch tells you of a flagged row: no notice named the
 *  preset's value now yet. So a fix is told once, and a second fix to
 *  the same row is told again. */
export function isNews(row: EditRow, now: EditValue): boolean {
  return row.seen === undefined || !same(row.seen, now);
}

/** A trigger with your edits laid over it. */
export interface Overlay {
  /** The trigger as it runs, the preset's with each edit that applies or
   *  is flagged over it. */
  trigger: PresetTrigger;
  /** Every row of that trigger, as its card shows them. */
  rows: Rows;
  /** What each edit does now, by row key. */
  holds: Record<string, Hold>;
}

/** Lay `edits`, your rows of `trigger`, over it row by row. */
export function overlay(
  trigger: PresetTrigger,
  edits: Readonly<Record<string, EditRow>> = {},
): Overlay {
  const now = triggerRows(trigger);
  const values: Rows = {};
  const holds: Record<string, Hold> = {};
  for (const [key, row] of Object.entries(edits)) {
    holds[key] = hold(key, row, nowOf(now, key));
    if (holds[key] === 'applies' || holds[key] === 'flagged') values[key] = row.value;
  }
  return { trigger: applyRows(trigger, values), rows: { ...now, ...values }, holds };
}

/** The rows a Save sends for one trigger: each row whose value differs
 *  between `loaded`, the card as it loaded, and `left`, the card as you
 *  left it, never the whole trigger against the preset. Each sends the
 *  preset's value now, from `now`, as its `was`, and a row `held`, the
 *  table, flags keeps its `seen`, so it stays flagged until you choose. */
export function diff(
  loaded: Readonly<Rows>,
  left: Readonly<Rows>,
  now: Readonly<Rows>,
  held: Readonly<Record<string, EditRow>> = {},
): Record<string, EditRow> {
  const out: Record<string, EditRow> = {};
  for (const key of new Set([...Object.keys(loaded), ...Object.keys(left)])) {
    const value = left[key] ?? NO_ROW;
    if (same(loaded[key] ?? NO_ROW, value)) continue;
    const was = nowOf(now, key) ?? NO_ROW;
    const seen = held[key]?.seen;
    out[key] = seen === undefined ? { value, was } : { value, was, seen };
  }
  return out;
}

/** The preset `t`, a stored trigger, comes from and its trigger there,
 *  or undefined for a trigger of yours or one this build no longer
 *  builds. */
export function libraryTrigger(
  t: TriggerRecord,
): { preset: Preset; trigger: PresetTrigger } | undefined {
  const preset = t.preset ? presetById(t.preset) : undefined;
  const trigger = preset?.triggers.find((p) => p.name === t.name);
  return preset && trigger ? { preset, trigger } : undefined;
}

// A preset trigger as a card holds it, tagged with its preset and in
// `group`, or in none.
function asCard(
  preset: Preset,
  trigger: PresetTrigger,
  group: string | null | undefined,
): TriggerRecord {
  const { group: _shipped, ...rest } = asRecord(trigger);
  return group ? { ...rest, group, preset: preset.id } : { ...rest, preset: preset.id };
}

/** The card Triggers shows for `stored`, a preset trigger as the store
 *  holds it: its trigger in the library with `edit`, your edits to its
 *  preset, laid over it, in the group the store keeps. Its colors stay
 *  named by key, so Replace with shows {mark} and {line} and a swatch
 *  still reaches what you write there. Undefined for a trigger the
 *  library does not build. */
export function presetCard(
  stored: TriggerRecord,
  edit: PresetEdit | undefined,
): TriggerRecord | undefined {
  const from = libraryTrigger(stored);
  if (!from) return undefined;
  const laid = overlay(from.trigger, edit?.triggers?.[stored.name]).trigger;
  return asCard(from.preset, laid, stored.group);
}

/** The card of `t` with every row as its preset ships it, its group
 *  included, as Reset to preset leaves it. */
export function shippedCard(t: TriggerRecord): TriggerRecord {
  const from = libraryTrigger(t);
  return from ? asCard(from.preset, from.trigger, from.trigger.group) : t;
}

/** The rows of `card` that differ from its trigger as the preset ships
 *  it, each with the preset's value. Empty for a trigger of yours. */
export function changedRows(card: TriggerRecord): Rows {
  const from = libraryTrigger(card);
  if (!from) return {};
  const mine = triggerRows(asPreset(card));
  const ship = triggerRows(from.trigger);
  const out: Rows = {};
  for (const key of new Set([...Object.keys(mine), ...Object.keys(ship)])) {
    const theirs = ship[key] ?? NO_ROW;
    if (!same(mine[key] ?? NO_ROW, theirs)) out[key] = theirs;
  }
  return out;
}

/** What a Save in Triggers sends to preset_edits_set, by preset id: for
 *  each preset trigger the page changed, the rows that differ between
 *  `before`, the card as it loaded, and `after`, the card as you left
 *  it. `edits` are your edits as they loaded, so a flagged row keeps its
 *  seen. */
export function cardEdits(
  changed: readonly { before: TriggerCard; after: TriggerCard }[],
  edits: Readonly<Record<string, PresetEdit>>,
): Map<string, PresetEdit> {
  const out = new Map<string, PresetEdit>();
  for (const { before, after } of changed) {
    const from = libraryTrigger(after);
    if (!from) continue;
    const { preset, trigger } = from;
    const left = triggerRows(asPreset(after));
    const now = triggerRows(trigger);
    const held = edits[preset.id]?.triggers?.[trigger.name];
    const rows = diff(triggerRows(asPreset(before)), left, now, held);
    // Keep mine sends the row with the preset's value now as its was and
    // no seen, which clears the flag.
    for (const key of after.kept ?? []) {
      if (before.kept?.includes(key) || !held?.[key]) continue;
      rows[key] = keepMine({ ...held[key], value: left[key] ?? NO_ROW }, nowOf(now, key) ?? NO_ROW);
    }
    if (Object.keys(rows).length === 0) continue;
    const edit = out.get(preset.id) ?? {};
    out.set(preset.id, { triggers: { ...edit.triggers, [trigger.name]: rows } });
  }
  return out;
}

/** Keep mine on a flagged row: its `was` moves to the preset's value
 *  `now` and its `seen` clears, so the flag goes and a later fix to the
 *  row asks again. */
export function keepMine(row: EditRow, now: EditValue): EditRow {
  return { value: row.value, was: now };
}

/** The rows of `trigger`, a preset trigger as the library builds it,
 *  that a fix changed while `held`, your edits to it, keeps them yours
 *  (board 4), each with the preset's value now. */
export function flaggedRows(
  trigger: PresetTrigger,
  held: Readonly<Record<string, EditRow>> | undefined,
): Rows {
  const now = triggerRows(trigger);
  const out: Rows = {};
  for (const [key, row] of Object.entries(held ?? {})) {
    const value = nowOf(now, key);
    if (value !== undefined && hold(key, row, value) === 'flagged') out[key] = value;
  }
  return out;
}

/** The swatches of `preset` that a fix changed while `edit` keeps your
 *  color, each with the preset's color now. */
export function flaggedColors(
  preset: Preset,
  edit: PresetEdit | undefined,
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, row] of Object.entries(edit?.colors ?? {})) {
    const now = preset.colors[key]?.token;
    if (now !== undefined && hold(key, row, now) === 'flagged') out[key] = now;
  }
  return out;
}

/** How many rows of `preset`, swatches and trigger rows, a fix changed
 *  while `edit` keeps them yours. */
export function flagCount(preset: Preset, edit: PresetEdit | undefined): number {
  let count = Object.keys(flaggedColors(preset, edit)).length;
  for (const t of preset.triggers) {
    count += Object.keys(flaggedRows(t, edit?.triggers?.[t.name])).length;
  }
  return count;
}

/** A preset trigger's card in Triggers. `kept` names the flagged rows you
 *  chose Keep mine on, a change in the page until Save. */
export type TriggerCard = TriggerRecord & { kept?: string[] };

/** The flagged rows `card` still shows, each with the preset's value
 *  now: those of `held`, your edits to its trigger as they loaded, less
 *  the rows you chose on in the page, Keep mine by `kept` and Take the
 *  fix by holding the preset's value. */
export function cardFlags(
  card: TriggerCard,
  held: Readonly<Record<string, EditRow>> | undefined,
): Rows {
  const from = libraryTrigger(card);
  if (!from || !held) return {};
  const mine = triggerRows(asPreset(card));
  const out: Rows = {};
  for (const [key, now] of Object.entries(flaggedRows(from.trigger, held))) {
    if (!card.kept?.includes(key) && !same(mine[key] ?? NO_ROW, now)) out[key] = now;
  }
  return out;
}

/** `card` with its row `key` set to `value`. */
export function withRow(card: TriggerCard, key: string, value: EditValue): TriggerCard {
  return asRecord(applyRows(asPreset(card), { [key]: value }));
}

/** The row Rust drops at once, since its value is its was. */
export const dropped = (row: EditRow): EditRow => ({ value: row.was, was: row.was });

/** A row of a preset the launch notice names. `trigger` is null for a
 *  swatch, and `row` is null for a whole trigger the preset no longer
 *  builds. */
export interface RowRef {
  preset: string;
  trigger: string | null;
  row: string | null;
}

/** What the corner notice says after a preset plan run (board 4): the
 *  message, the trigger or the preset it names first as its meta, and
 *  the Settings link Show opens. */
export interface FixNotice {
  message: string;
  meta: string;
  /** The meta is a trigger name, which reads in the terminal face. */
  mono: boolean;
  link: string;
}

/** The corner notice for the rows a run `told` of and the rows and the
 *  triggers it `removed`, or null when it names none. Rows a fix changed
 *  come first, then the triggers a fix took away. */
export function fixNotice(told: readonly RowRef[], removed: readonly RowRef[]): FixNotice | null {
  const changed = [...told, ...removed.filter((r) => r.row !== null)];
  const gone = removed.filter((r) => r.row === null);
  const first = changed[0] ?? gone[0];
  if (!first) return null;
  const n = changed.length > 0 ? changed.length : gone.length;
  const message =
    changed.length > 0
      ? `A preset fix changed ${n === 1 ? 'a row' : `${n} rows`} you edited`
      : `A preset fix removed ${n === 1 ? 'a trigger' : `${n} triggers`} you edited`;
  // A trigger the preset still builds opens in Triggers, and a swatch or
  // a trigger it took away opens the preset's card.
  const inTriggers = first.trigger !== null && first.row !== null;
  const link = inTriggers
    ? `automation:triggers#triggers:${first.trigger}`
    : `automation:presets#presets:${first.preset}`;
  const meta = first.trigger ?? presetById(first.preset)?.name ?? first.preset;
  return { message, meta, mono: first.trigger !== null, link };
}

/** A preset built with your edits laid over it. */
export interface PresetBuild {
  /** Its triggers as the store holds them, in your colors with your rows
   *  over them. A trigger keeps no group where your edits hold none, so
   *  Rust keeps the group of its stored copy. */
  triggers: TriggerRecord[];
  /** What to save back through preset_edits_set: `seen` on each flag
   *  this build tells, and each row that folds or was removed taken out.
   *  Empty when nothing changes. */
  write: PresetEdit;
  /** The flagged rows this build tells of for the first time. */
  told: RowRef[];
  /** The rows and the triggers the preset no longer has that held your
   *  edits. Their edits come out, so each is named once. */
  removed: RowRef[];
}

/** Build `preset` in your colors with `edit`, your edits to it, laid over
 *  each trigger, held against the preset as it is now. */
export function buildPreset(preset: Preset, edit: PresetEdit = {}): PresetBuild {
  const build: PresetBuild = { triggers: [], write: {}, told: [], removed: [] };
  // Settle one edit: note what the launch tells and what to save back.
  const settle = (
    into: Record<string, EditRow>,
    key: string,
    row: EditRow,
    now: EditValue | undefined,
    verdict: Hold,
    trigger: string | null,
  ) => {
    const ref: RowRef = { preset: preset.id, trigger, row: key };
    if (verdict === 'flagged' && now !== undefined && isNews(row, now)) {
      into[key] = { value: row.value, was: now, seen: now };
      build.told.push(ref);
    } else if (verdict === 'folds') {
      into[key] = dropped(row);
    } else if (verdict === 'removed') {
      into[key] = dropped(row);
      build.removed.push(ref);
    }
  };

  const colors: Record<string, string> = {};
  const colorWrites: Record<string, EditRow> = {};
  for (const [key, row] of Object.entries(edit.colors ?? {})) {
    const now = preset.colors[key]?.token;
    const verdict = hold(key, row, now);
    if (verdict === 'applies' || verdict === 'flagged') colors[key] = String(row.value);
    settle(colorWrites, key, row, now, verdict, null);
  }
  if (Object.keys(colorWrites).length > 0) build.write.colors = colorWrites;

  const triggerWrites: Record<string, Record<string, EditRow>> = {};
  const edited = edit.triggers ?? {};
  for (const t of preset.triggers) {
    const rows = edited[t.name] ?? {};
    const laid = overlay(t, rows);
    const now = triggerRows(t);
    const writes: Record<string, EditRow> = {};
    for (const [key, row] of Object.entries(rows)) {
      settle(writes, key, row, nowOf(now, key), laid.holds[key], t.name);
    }
    if (Object.keys(writes).length > 0) triggerWrites[t.name] = writes;
    build.triggers.push(fillColors(preset, laid.trigger, colors));
  }
  for (const [name, rows] of Object.entries(edited)) {
    if (preset.triggers.some((t) => t.name === name)) continue;
    build.removed.push({ preset: preset.id, trigger: name, row: null });
    triggerWrites[name] = Object.fromEntries(
      Object.entries(rows).map(([key, row]) => [key, dropped(row)]),
    );
  }
  if (Object.keys(triggerWrites).length > 0) build.write.triggers = triggerWrites;
  return build;
}

/** The hex of a preset color that never follows the theme: one of the
 *  256 fixed colors past the sixteen, as fg:178, or a true color. Null
 *  for a theme color. */
export function fixedColorHex(token: string): string | null {
  const fixed = /^fg:(\d+)$/.exec(token);
  if (fixed) {
    const n = Number(fixed[1]);
    return n >= 16 && n < 256 ? toHex(indexedRgb(n, [])) : null;
  }
  return /^#[0-9a-f]{6}$/i.test(token) ? token.toLowerCase() : null;
}

// An edit with its empty parts left out, or undefined when it holds none,
// as Rust keeps it.
function tidy(edit: PresetEdit): PresetEdit | undefined {
  const out: PresetEdit = {};
  if (edit.colors && Object.keys(edit.colors).length > 0) out.colors = edit.colors;
  const triggers = Object.entries(edit.triggers ?? {}).filter(([, r]) => Object.keys(r).length > 0);
  if (triggers.length > 0) out.triggers = Object.fromEntries(triggers);
  return out.colors || out.triggers ? out : undefined;
}

/** `edit`, your edits to `preset`, with its swatch `key` set to `value`,
 *  or cleared with null. A value that is the preset's own token is no
 *  edit, so its row goes at once. A row you change again keeps the
 *  preset's value you first changed it from. Undefined once no edit is
 *  left. */
export function withColorEdit(
  preset: Preset,
  edit: PresetEdit | undefined,
  key: string,
  value: string | null,
): PresetEdit | undefined {
  const token = preset.colors[key].token;
  const colors = { ...edit?.colors };
  const held = colors[key];
  delete colors[key];
  if (value !== null && value !== token) {
    // A flagged swatch you change again stays flagged until you choose.
    colors[key] =
      held?.seen === undefined
        ? { value, was: held?.was ?? token }
        : { value, was: held.was, seen: held.seen };
  }
  return tidy({ ...edit, colors });
}

/** `edit` with Keep mine on its flagged swatch `key`: your color stays,
 *  and its was moves to the preset's color now. */
export function withColorKept(preset: Preset, edit: PresetEdit, key: string): PresetEdit {
  const row = edit.colors?.[key];
  if (!row) return edit;
  return { ...edit, colors: { ...edit.colors, [key]: keepMine(row, preset.colors[key].token) } };
}

/** Whether two rows hold the same value, was and seen. Keep mine
 *  changes only the was. */
const sameRow = (a: EditRow, b: EditRow) =>
  same(a.value, b.value) && same(a.was, b.was) && same(a.seen, b.seen);

/** What Save sends to preset_edits_set for `preset` to turn `before`,
 *  your edits as they loaded, into `after`, the edits the card holds: each
 *  row that changed, and each row that went as one that says the
 *  preset's value, so Rust drops it. A swatch whose hex is the preset's
 *  fixed color folds away the same way. Null when nothing changed. */
export function editsToSave(
  preset: Preset,
  before: PresetEdit | undefined,
  after: PresetEdit | undefined,
): PresetEdit | null {
  const out: PresetEdit = {};
  const colors: Record<string, EditRow> = {};
  const keys = new Set([...Object.keys(before?.colors ?? {}), ...Object.keys(after?.colors ?? {})]);
  for (const key of keys) {
    const was = before?.colors?.[key];
    const row = after?.colors?.[key];
    if (row && was && sameRow(row, was)) continue;
    const token = preset.colors[key]?.token ?? String((row ?? was)!.was);
    const folds =
      !row || (typeof row.value === 'string' && row.value.toLowerCase() === fixedColorHex(token));
    colors[key] = folds ? { value: token, was: token } : row;
  }
  if (Object.keys(colors).length > 0) out.colors = colors;

  const triggers: Record<string, Record<string, EditRow>> = {};
  const names = new Set([
    ...Object.keys(before?.triggers ?? {}),
    ...Object.keys(after?.triggers ?? {}),
  ]);
  for (const name of names) {
    const from = before?.triggers?.[name] ?? {};
    const to = after?.triggers?.[name] ?? {};
    const rows: Record<string, EditRow> = {};
    for (const key of new Set([...Object.keys(from), ...Object.keys(to)])) {
      const was = from[key];
      const row = to[key];
      if (row && was && sameRow(row, was)) continue;
      rows[key] = row ?? dropped(was);
    }
    if (Object.keys(rows).length > 0) triggers[name] = rows;
  }
  if (Object.keys(triggers).length > 0) out.triggers = triggers;
  return out.colors || out.triggers ? out : null;
}

/** The triggers of `preset` whose group edit went between `before`,
 *  your edits as they loaded, and `after`, the edits the card holds, each
 *  with the group the preset ships it in, blank for none. The store keeps
 *  a trigger's group too, and the preset plan falls back to it, so Reset
 *  to preset puts each of these back there as the trigger's own Reset
 *  does. */
export function groupsReset(
  preset: Preset,
  before: PresetEdit | undefined,
  after: PresetEdit | undefined,
): Map<string, string> {
  const out = new Map<string, string>();
  for (const t of preset.triggers) {
    if (before?.triggers?.[t.name]?.group && !after?.triggers?.[t.name]?.group) {
      out.set(t.name, t.group ?? '');
    }
  }
  return out;
}

/** What Your changes on a preset's card names: the swatches you changed,
 *  by key in the preset's order, and the triggers you edited, by name in
 *  the preset's order, any it no longer builds last. */
export interface EditSummary {
  colors: string[];
  triggers: string[];
}

export function editSummary(preset: Preset, edit: PresetEdit | undefined): EditSummary {
  const colors = Object.keys(preset.colors).filter((k) => edit?.colors?.[k] !== undefined);
  const edited = Object.keys(edit?.triggers ?? {});
  const order = preset.triggers.map((t) => t.name);
  const triggers = [
    ...order.filter((n) => edited.includes(n)),
    ...edited.filter((n) => !order.includes(n)),
  ];
  return { colors, triggers };
}

/** What the Your changes line of `preset` says for `edit`: each color
 *  you changed, as `The line color`, and each trigger you edited by name,
 *  or past two, how many, as `2 colors and 3 triggers`. */
export type ChangesLine = { count: string } | { colors: string[]; triggers: string[] };

/** The Your changes line of `preset` for `edit`, the card's and the
 *  shared catalog wizard's. Null while you changed nothing. */
export function changesLine(preset: Preset, edit: PresetEdit | undefined): ChangesLine | null {
  const { colors, triggers } = editSummary(preset, edit);
  const count = colors.length + triggers.length;
  if (count === 0) return null;
  if (count > 2) {
    return {
      count: listJoin([
        ...(colors.length > 0 ? [countPhrase(colors.length, COLOR_NOUN)] : []),
        ...(triggers.length > 0 ? [countPhrase(triggers.length, TRIGGER_NOUN)] : []),
      ]),
    };
  }
  return { colors: colors.map((key) => `${preset.colors[key].label} color`), triggers };
}

/** Whether `edit` holds any edit, the pencil in the list. */
export function hasEdits(edit: PresetEdit | undefined): boolean {
  return tidy(edit ?? {}) !== undefined;
}

/** Your swatch colors in `edit`, by key, the colors Looks like draws. */
export function editColors(edit: PresetEdit | undefined): Record<string, string> {
  return Object.fromEntries(
    Object.entries(edit?.colors ?? {}).map(([key, row]) => [key, String(row.value)]),
  );
}
