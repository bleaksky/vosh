import { createContext, Fragment, type ReactNode } from 'react';
import { alertOrNone } from '../../automation/alertParts';
import { extraEffects, mainPattern, patternSource } from '../../automation/automationTriggers';
import { patternKey, type Rows } from '../../automation/presetEdits';
import type { Preset } from '../../automation/presets';
import type { AlertParts, TriggerRecord } from '../../ipc/automation';
import type { EditValue } from '../../ipc/presetEdits';

/** Opens a preset's card in Presets, by id, from the note at the head
 *  of a preset trigger's card. */
export const OpenPresetContext = createContext<(id: string) => void>(() => {});

export const PATTERN_NOUN = { one: 'pattern', many: 'patterns' };

/** The rows under Advanced, by the key their edits keep. */
export const ADVANCED_ROWS: readonly string[] = [
  'priority',
  'target',
  'fg',
  'bg',
  'bold',
  'underline',
  'inverse',
  'route',
  'script',
];

/** The rows under Advanced past ADVANCED_ROWS whose fix opens it. */
export const opensAdvanced = (key: string, main: string) =>
  ADVANCED_ROWS.includes(key) || (key.startsWith('pattern:') && key !== main);

/** What a flagged row shows, or null for a row no fix flags: `say` puts
 *  the preset's value now in words. */
export type Flag = (key: string, say: (value: EditValue) => ReactNode) => ReactNode;

/** The preset's text, in the MUD font, or that it leaves it empty. */
export function presetText(value: EditValue | undefined): ReactNode {
  const text = typeof value === 'string' ? value : '';
  return text ? <span className="st-auto-mono">{text}</span> : 'it empty';
}

export const presetSwitch = (value: EditValue | undefined) => (value === true ? 'it on' : 'it off');

/** `description` with the line of a changed row after it, or either one
 *  alone. */
export function withChanged(description: ReactNode, changed: ReactNode): ReactNode {
  if (changed === null) return description;
  return (
    <>
      {description}
      {changed}
    </>
  );
}

/** What a preset trigger's card knows of its preset: the trigger as
 *  the preset ships it, each row you changed with the preset's value,
 *  and the color your swatch or the preset gives each key. */
export interface FromPreset {
  preset: Preset;
  ship: TriggerRecord;
  changed: Rows;
  colorOf: (key: string) => string;
}

/** The color keys of `preset` that `template` names, in its order. */
export function colorKeys(preset: Preset, template: string): string[] {
  const keys = [...template.matchAll(/\{([a-z_]+)\}/g)].map((m) => m[1]);
  return [...new Set(keys)].filter((key) => Object.hasOwn(preset.colors, key));
}

/** `parts` joined as a sentence joins them, `a and b`, `a, b, and c`. */
export function joinNodes(parts: readonly ReactNode[]): ReactNode {
  return parts.map((part, i) => (
    <Fragment key={i}>
      {i === 0 ? '' : i === parts.length - 1 ? (parts.length > 2 ? ', and ' : ' and ') : ', '}
      {part}
    </Fragment>
  ));
}

/** Each pattern past the main one, and whether the main one is on. */
function morePatterns(t: TriggerRecord): string {
  return JSON.stringify([
    mainPattern(t).enabled,
    ...t.patterns.slice(1).map((p) => [patternSource(p), p.enabled]),
  ]);
}

export const morePatternsDiffer = (t: TriggerRecord, ship: TriggerRecord) =>
  morePatterns(t) !== morePatterns(ship);

/** The commands of each Also send. */
export const alsoSends = (t: TriggerRecord) =>
  extraEffects(t.actions)
    .filter((e) => e.kind === 'send')
    .map((e) => e.value);

/** How many Also send rows differ from the preset's: the ones you
 *  changed or added, or the ones you took out where those are more. */
export function alsoChanges(t: TriggerRecord, ship: TriggerRecord): number {
  const mine = alsoSends(t);
  const theirs = alsoSends(ship);
  const added = mine.filter((c) => !theirs.includes(c)).length;
  const gone = theirs.filter((c) => !mine.includes(c)).length;
  return Math.max(added, gone);
}

/** The key of the main pattern of `ship`, the trigger as its preset
 *  ships it. */
export const mainKeyOf = (ship: TriggerRecord) => patternKey(patternSource(mainPattern(ship)));

/** `v` with its alert table set by `fn`, or with none while the table
 *  is the default, so a trigger that rings nothing saves no alert. */
export function withAlert(
  v: TriggerRecord,
  fn: (alert: AlertParts | undefined) => AlertParts,
): TriggerRecord {
  const next = { ...v };
  const alert = alertOrNone(fn(v.alert));
  if (alert) next.alert = alert;
  else delete next.alert;
  return next;
}
