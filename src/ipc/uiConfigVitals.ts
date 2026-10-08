// Your vitals options as the config holds them and as the bus carries
// them, and the events that bring a new pick or a new vitals text.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { VITALS_OPTIONS_CHANGED, VITALS_TEXT_CHANGED } from './events';
import type { UiConfig } from './uiConfig';

/** How the vitals under the panel's panes lay out. `rows` gives each
 *  vital its own row. `line` sets Health, Mana, and Moves side by side
 *  on one row. */
export const VITALS_DENSITIES = ['rows', 'line'] as const;

export type VitalsDensity = (typeof VITALS_DENSITIES)[number];

/** Coerce an unknown vitals density back to rows. */
export function normalizeVitalsDensity(value: unknown): VitalsDensity {
  return value === 'line' ? 'line' : 'rows';
}

/** What each vital's value shows. `current-max` reads `186 / 1020`,
 *  `current` reads `186`, and `percent` reads `18%`. */
export const VITALS_VALUES = ['current-max', 'current', 'percent'] as const;

export type VitalsValues = (typeof VITALS_VALUES)[number];

/** Coerce an unknown value form back to current and max. */
export function normalizeVitalsValues(value: unknown): VitalsValues {
  return value === 'current' || value === 'percent' ? value : 'current-max';
}

/** The meter under each vital. `line` is the 2 px meter, `bar` the
 *  4 px one, and `none` drops the meters and tightens the rows. */
export const VITALS_METERS = ['line', 'bar', 'none'] as const;

export type VitalsMeter = (typeof VITALS_METERS)[number];

/** Coerce an unknown meter back to the line. */
export function normalizeVitalsMeter(value: unknown): VitalsMeter {
  return value === 'bar' || value === 'none' ? value : 'line';
}

/** The styles of the gallery, in its order. Rows and One line are the
 *  two densities, and vitals_style holds the others. */
export const VITALS_STYLES = [
  'rows',
  'line',
  'ledger',
  'gauges',
  'pips',
  'bands',
  'ladders',
  'blocks',
  'traces',
  'dials',
  'rings',
  'vials',
  'orbs',
  'candles',
  'text',
] as const;

export type VitalsStyle = (typeof VITALS_STYLES)[number];

/** The styles vitals_style saves. Rows and One line stay in
 *  vitals_density, so a build without styles still reads your look. A
 *  build reads a name it does not know as none and draws your density.
 *  Mirrors VITALS_STYLES in src-tauri/src/profile/ui.rs. */
const SAVED_VITALS_STYLES = [
  'ledger',
  'gauges',
  'pips',
  'bands',
  'ladders',
  'blocks',
  'traces',
  'dials',
  'rings',
  'vials',
  'orbs',
  'candles',
  'text',
] as const;

export type SavedVitalsStyle = (typeof SAVED_VITALS_STYLES)[number];

/** Coerce an unknown saved style back to null, which draws the
 *  density. */
export function normalizeVitalsStyle(value: unknown): SavedVitalsStyle | null {
  return SAVED_VITALS_STYLES.find((style) => style === value) ?? null;
}

/** The style your vitals draw in, the one you picked or else your
 *  density, so a player who never picks sees today's look. */
export function shownStyle(config: Pick<UiConfig, 'vitals_style' | 'vitals_density'>): VitalsStyle {
  return config.vitals_style ?? config.vitals_density;
}

/** Where your vitals show, under the panel's panes or in the status
 *  line. */
export const VITALS_PLACES = ['panel', 'status'] as const;

export type VitalsPlace = (typeof VITALS_PLACES)[number];

/** Coerce an unknown place back to the panel. */
export function normalizeVitalsPlace(value: unknown): VitalsPlace {
  return value === 'status' ? 'status' : 'panel';
}

/** Your vitals in today's order. */
export const VITALS = ['hp', 'mana', 'move'] as const;

export type Vital = (typeof VITALS)[number];

/** Keep each known vital once, in the order given, and add any missing
 *  after them in today's order. */
export function normalizeVitalsOrder(value: unknown): Vital[] {
  const given = Array.isArray(value) ? (value as unknown[]) : [];
  const kept: Vital[] = [];
  for (const name of [...given, ...VITALS]) {
    const vital = VITALS.find((v) => v === name);
    if (vital && !kept.includes(vital)) kept.push(vital);
  }
  return kept;
}

/** What vitals_off can hold, each vital and your opponent's row. */
export const VITALS_OFF = [...VITALS, 'opponent'] as const;

export type VitalOff = (typeof VITALS_OFF)[number];

/** Keep each known name once, in the order of VITALS_OFF. */
export function normalizeVitalsOff(value: unknown): VitalOff[] {
  const given = Array.isArray(value) ? (value as unknown[]) : [];
  return VITALS_OFF.filter((name) => given.includes(name));
}

/** Where your opponent's row sits in a fight. */
export const VITALS_OPPONENT_PLACES = ['top', 'bottom'] as const;

export type VitalsOpponent = (typeof VITALS_OPPONENT_PLACES)[number];

/** Coerce an unknown opponent place back to the top. */
export function normalizeVitalsOpponent(value: unknown): VitalsOpponent {
  return value === 'bottom' ? 'bottom' : 'top';
}

/** Each vital's color as an ANSI slot from 0 to 15. A vital left out
 *  takes Default. */
export type VitalsColors = Partial<Record<Vital, number>>;

/** Keep the colors of known vitals that name a slot from 0 to 15. */
export function normalizeVitalsColors(value: unknown): VitalsColors {
  const given = value && typeof value === 'object' ? (value as Record<string, unknown>) : {};
  const colors: VitalsColors = {};
  for (const vital of VITALS) {
    const slot = given[vital];
    if (typeof slot === 'number' && Number.isInteger(slot) && slot >= 0 && slot <= 15) {
      colors[vital] = slot;
    }
  }
  return colors;
}

/** How many earlier vitals texts Vosh keeps. */
const VITALS_TEXT_PREVIOUS = 2;

/** Drop blank and repeated texts and keep the newest two. */
export function normalizeVitalsTextPrevious(value: unknown): string[] {
  const given = Array.isArray(value) ? (value as unknown[]) : [];
  const kept: string[] = [];
  for (const text of given) {
    if (kept.length === VITALS_TEXT_PREVIOUS) break;
    if (typeof text === 'string' && text.length > 0 && !kept.includes(text)) kept.push(text);
  }
  return kept;
}

/** Every vitals choice the footer, the status line and the menu draw
 *  from, as one event payload, so a pick moves them together. The
 *  status line reads the values and the warning, never the meter. Your
 *  vitals text comes on its own event, rendered (src/ipc/vitals.ts). */
export interface VitalsOptions {
  /** The style shown, your pick or else your density. */
  style: VitalsStyle;
  place: VitalsPlace;
  order: Vital[];
  off: VitalOff[];
  opponent: VitalsOpponent;
  colors: VitalsColors;
  values: VitalsValues;
  meter: VitalsMeter;
  /** Warn under two thirds and turn danger under one third, like the
   *  Group pane. Off keeps danger under 20 percent. */
  warn_thirds: boolean;
  /** Hide the panel's vitals while your prompt is pinned. */
  hide_when_pinned: boolean;
  /** Show each hit: the part a hit took stays pale for a moment. */
  hit: boolean;
}

/** What Reset to default under Customize vitals puts back. Every vital
 *  on in today's order with Default colors, your opponent on top,
 *  Current and max, Line, and the warning and Show each hit off. Your style, where your
 *  vitals show and Hide vitals while your prompt is pinned stay as they
 *  are. */
export const DEFAULT_VITALS_CUSTOM: Pick<
  UiConfig,
  | 'vitals_order'
  | 'vitals_off'
  | 'vitals_colors'
  | 'vitals_opponent'
  | 'vitals_values'
  | 'vitals_meter'
  | 'vitals_warn_thirds'
  | 'vitals_hit'
> = {
  vitals_order: [...VITALS],
  vitals_off: [],
  vitals_colors: {},
  vitals_opponent: 'top',
  vitals_values: 'current-max',
  vitals_meter: 'line',
  vitals_warn_thirds: false,
  vitals_hit: false,
};

/** The fields of the config VitalsOptions reads. */
type VitalsFields =
  | 'vitals_style'
  | 'vitals_density'
  | 'vitals_place'
  | 'vitals_order'
  | 'vitals_off'
  | 'vitals_opponent'
  | 'vitals_colors'
  | 'vitals_values'
  | 'vitals_meter'
  | 'vitals_warn_thirds'
  | 'vitals_hide_when_pinned'
  | 'vitals_hit';

/** The vitals options a config holds. */
export function vitalsOptionsOf(config: Pick<UiConfig, VitalsFields>): VitalsOptions {
  return {
    style: shownStyle(config),
    place: config.vitals_place,
    order: config.vitals_order,
    off: config.vitals_off,
    opponent: config.vitals_opponent,
    colors: config.vitals_colors,
    values: config.vitals_values,
    meter: config.vitals_meter,
    warn_thirds: config.vitals_warn_thirds,
    hide_when_pinned: config.vitals_hide_when_pinned,
    hit: config.vitals_hit,
  };
}

export const DEFAULT_VITALS_OPTIONS: VitalsOptions = vitalsOptionsOf({
  ...DEFAULT_VITALS_CUSTOM,
  vitals_style: null,
  vitals_density: 'rows',
  vitals_place: 'panel',
  vitals_hide_when_pinned: true,
});

/** Read vitals options off the bus, filling anything missing or
 *  unknown with the defaults. */
export function normalizeVitalsOptions(raw: unknown): VitalsOptions {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    style: VITALS_STYLES.find((style) => style === o.style) ?? 'rows',
    place: normalizeVitalsPlace(o.place),
    order: normalizeVitalsOrder(o.order),
    off: normalizeVitalsOff(o.off),
    opponent: normalizeVitalsOpponent(o.opponent),
    colors: normalizeVitalsColors(o.colors),
    values: normalizeVitalsValues(o.values),
    meter: normalizeVitalsMeter(o.meter),
    warn_thirds: o.warn_thirds === true,
    hide_when_pinned: o.hide_when_pinned !== false,
    hit: o.hit === true,
  };
}

/** Hear new vitals options, your style and every choice under Layout,
 *  Vitals, saved from Settings or the menu, or the ones a profile
 *  switch brings. */
export async function subscribeVitalsOptionsChanged(
  cb: (value: VitalsOptions) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(VITALS_OPTIONS_CHANGED, (event) => {
    cb(normalizeVitalsOptions(event.payload));
  });
}

/** Your vitals text and the earlier ones, as one event payload. */
export type VitalsTextChange = Pick<UiConfig, 'vitals_text' | 'vitals_text_previous'>;

export function vitalsTextOf(config: VitalsTextChange): VitalsTextChange {
  return { vitals_text: config.vitals_text, vitals_text_previous: config.vitals_text_previous };
}

/** Hear a new vitals text, saved from Settings or the vitals text card,
 *  or the one a profile switch brings. */
export async function subscribeVitalsTextChanged(
  cb: (value: VitalsTextChange) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(VITALS_TEXT_CHANGED, (event) => {
    const raw = event.payload as { vitals_text?: unknown; vitals_text_previous?: unknown } | null;
    cb({
      vitals_text: typeof raw?.vitals_text === 'string' ? raw.vitals_text : '',
      vitals_text_previous: normalizeVitalsTextPrevious(raw?.vitals_text_previous),
    });
  });
}
