// The affects you track, the affects the game sends, and how the Affects
// pane draws them.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { CRITICAL_TICKS, EXPIRING_TICKS } from '../lib/affectsView';
import type { UiConfig } from './uiConfig';

/// Cross-window broadcast for tracked-affect changes. The settings
/// window is a separate Tauri webview, so `window.dispatchEvent`
/// only reaches its own DOM; the main window's BottomHUD listens
/// via this Tauri channel and via the legacy window event (still
/// emitted for in-window consumers like AuxDrawer).
export const TRACKED_AFFECTS_EVENT = 'vosh://tracked-affects-changed';

/** One tracked-affect entry. `name` is what the server pushes in the
 *  Char.Affects feed (matched case-insensitively, whitespace
 *  collapsed). `label` is the optional display string shown in the
 *  affects bar — leave empty to show the name itself. Pair lets the
 *  user track "Field of Discord" but see it as "Shroud" alongside
 *  long names like "Comprehend Languages". */
export interface TrackedAffect {
  name: string;
  label: string | null;
}

export async function subscribeTrackedAffectsChanged(
  cb: (list: TrackedAffect[]) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(TRACKED_AFFECTS_EVENT, (event) => {
    if (Array.isArray(event.payload)) cb(normalizeTrackedAffects(event.payload));
  });
}

/** Coerce a raw array (over-the-wire) into TrackedAffect rows.
 *  Accepts:
 *    - bare strings (legacy): "sanc" -> { name: "sanc", label: null }
 *    - full rows:             { name, label? }
 *  Empty / non-object entries are skipped. */
export function normalizeTrackedAffects(raw: unknown[]): TrackedAffect[] {
  const out: TrackedAffect[] = [];
  for (const row of raw) {
    if (typeof row === 'string') {
      const trimmed = row.trim();
      if (trimmed.length > 0) out.push({ name: trimmed, label: null });
    } else if (row && typeof row === 'object') {
      const r = row as { name?: unknown; label?: unknown };
      const name = typeof r.name === 'string' ? r.name.trim() : '';
      if (name.length === 0) continue;
      const labelRaw = typeof r.label === 'string' ? r.label.trim() : '';
      out.push({ name, label: labelRaw.length > 0 ? labelRaw : null });
    }
  }
  return out;
}

/** The last Char.Affects payload of this connection, raw as the MUD
 *  sent it, or null. A window that opens between ticks reads it so it
 *  shows the affects on you without waiting for the next list. */
export async function affectsSnapshotGet(): Promise<unknown> {
  return invoke('affects_snapshot_get');
}

/** The affect fulls the backend keeps for the logged in character. */
export async function affectFullGet(): Promise<unknown> {
  return invoke('affect_full_get');
}

/** Hear the affect fulls change: a list that starts, recasts, or ends
 *  an affect, the saved fulls at login, or a disconnect that empties
 *  them. The payload is the whole map. */
export async function subscribeAffectFullChanged(
  cb: (value: unknown) => void,
): Promise<UnlistenFn> {
  return listen<unknown>('vosh://affect-full-changed', (event) => cb(event.payload));
}

/** The layouts the Affects pane draws. `timers` is Timers first, the
 *  default, with your tracked affects in their slots. `countdown` lists
 *  every affect by the hours it has left. `chips` groups them as chips,
 *  what to recast first. `chips_drain` groups them the same way, and a
 *  chip running out colors only the share of it that matches the hours
 *  it has left. */
export const AFFECTS_STYLES = ['timers', 'countdown', 'chips', 'chips_drain'] as const;

export type AffectsStyle = (typeof AFFECTS_STYLES)[number];

/** Grouped chips and Draining chips, the styles that pack chips and
 *  show the state on each, with no marker or tint of their own. */
export function isChipsStyle(style: AffectsStyle): boolean {
  return style === 'chips' || style === 'chips_drain';
}

/** Coerce an unknown affects layout back to Timers first. */
export function normalizeAffectsStyle(value: unknown): AffectsStyle {
  return AFFECTS_STYLES.find((style) => style === value) ?? 'timers';
}

/** The mark beside each tracked affect in the timers and countdown
 *  layouts. `dot` is the default. `none` draws no mark. */
export const AFFECTS_MARKERS = ['dot', 'square', 'plus_minus', 'none'] as const;

export type AffectsMarker = (typeof AFFECTS_MARKERS)[number];

/** Coerce an unknown affects marker back to the dot. */
export function normalizeAffectsMarker(value: unknown): AffectsMarker {
  return AFFECTS_MARKERS.find((marker) => marker === value) ?? 'dot';
}

/** The most hours either affects threshold takes. */
export const AFFECTS_HOURS_MAX = 99;

/** Whole hours from 0 to AFFECTS_HOURS_MAX, or null when `value` is no
 *  number. */
function affectsHoursOf(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) return null;
  return Math.min(AFFECTS_HOURS_MAX, Math.max(0, Math.round(value)));
}

/** The hours at which an affect runs out and is almost gone, read the
 *  way the backend saves them: whole hours from 0 to 99, almost gone
 *  never over running out, and the defaults, 2 and 1, for anything
 *  that is no number. */
export function normalizeAffectsThresholds(
  runningOut: unknown,
  almostGone: unknown,
): { running_out: number; almost_gone: number } {
  const running_out = affectsHoursOf(runningOut) ?? EXPIRING_TICKS;
  const almost_gone = Math.min(affectsHoursOf(almostGone) ?? CRITICAL_TICKS, running_out);
  return { running_out, almost_gone };
}

/** How the Affects pane draws, as one event payload. Style and Marker
 *  from Settings, Layout, Affects or the pane's own menu, and Tint what
 *  to recast and the two thresholds from Settings. */
export interface AffectsDisplay {
  style: AffectsStyle;
  marker: AffectsMarker;
  /** Tint the missing and running out rows in the timers and countdown
   *  layouts. Grouped chips always do. */
  tint: boolean;
  /** At or under this many hours an affect you track turns yellow and
   *  counts as running out. `affects_running_out_hours`. */
  running_out: number;
  /** At or under this many hours an affect's hours turn bold red.
   *  `affects_almost_gone_hours`. */
  almost_gone: number;
}

export const DEFAULT_AFFECTS_DISPLAY: AffectsDisplay = {
  style: 'timers',
  marker: 'dot',
  tint: false,
  running_out: EXPIRING_TICKS,
  almost_gone: CRITICAL_TICKS,
};

/** The fields of a config that hold the affects display. */
export type AffectsDisplayFields = Pick<
  UiConfig,
  | 'affects_style'
  | 'affects_marker'
  | 'affects_tint'
  | 'affects_running_out_hours'
  | 'affects_almost_gone_hours'
>;

/** The affects display a config holds. */
export function affectsDisplayOf(config: AffectsDisplayFields): AffectsDisplay {
  return {
    style: config.affects_style,
    marker: config.affects_marker,
    tint: config.affects_tint,
    running_out: config.affects_running_out_hours,
    almost_gone: config.affects_almost_gone_hours,
  };
}

/** The config fields that hold `display`, to lay over a config copy. */
export function affectsDisplayFields(display: AffectsDisplay): AffectsDisplayFields {
  return {
    affects_style: display.style,
    affects_marker: display.marker,
    affects_tint: display.tint,
    affects_running_out_hours: display.running_out,
    affects_almost_gone_hours: display.almost_gone,
  };
}

/** Whether two affects displays draw the pane alike. */
export function sameAffectsDisplay(a: AffectsDisplay, b: AffectsDisplay): boolean {
  return (
    a.style === b.style &&
    a.marker === b.marker &&
    a.tint === b.tint &&
    a.running_out === b.running_out &&
    a.almost_gone === b.almost_gone
  );
}

/** Read an affects display off the bus, filling anything missing or
 *  unknown with the defaults. */
export function normalizeAffectsDisplay(raw: unknown): AffectsDisplay {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    style: normalizeAffectsStyle(o.style),
    marker: normalizeAffectsMarker(o.marker),
    tint: o.tint === true,
    ...normalizeAffectsThresholds(o.running_out, o.almost_gone),
  };
}

export const AFFECTS_DISPLAY_EVENT = 'vosh://affects-display-changed';

/** Hear a new affects display, saved from Settings or picked in the
 *  pane menu, or the one a profile switch brings. */
export async function subscribeAffectsDisplayChanged(
  cb: (value: AffectsDisplay) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(AFFECTS_DISPLAY_EVENT, (event) => {
    cb(normalizeAffectsDisplay(event.payload));
  });
}

/** Save an affects display pick alone, for the pane menu. A full
 *  setUiConfig from the main window would write its stale copy of every
 *  other field. The backend tells every window. */
export async function setAffectsDisplay(patch: Partial<AffectsDisplay>): Promise<void> {
  await invoke('ui_set_affects_display', {
    style: patch.style ?? null,
    marker: patch.marker ?? null,
    tint: patch.tint ?? null,
    runningOut: patch.running_out ?? null,
    almostGone: patch.almost_gone ?? null,
  });
}

/** Replace a profile's tracked affects and get back the list as saved,
 *  trimmed and with names repeated in another case dropped. */
export async function trackedAffectsSet(
  list: TrackedAffect[],
  profile?: string | null,
): Promise<TrackedAffect[]> {
  const saved = await invoke<unknown>('tracked_affects_set', { list, profile: profile ?? null });
  return normalizeTrackedAffects(Array.isArray(saved) ? saved : []);
}
