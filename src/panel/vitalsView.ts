import type { CSSProperties } from 'react';
import type { PromptShowState } from '../ipc/prompt';
import type {
  UiFields,
  Vital,
  VitalOff,
  VitalsColors,
  VitalsMeter,
  VitalsOptions,
  VitalsStyle,
  VitalsValues,
} from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import { vitalPercent, type Vitals } from '../stores/gmcp/vitalsStore';
import { ANSI_SLOTS } from '../theme/baseAnsi';
import type { ChromeTokens } from '../theme/chrome';
import { liftAtHue, parseHex, toHex } from '../theme/color';
import type { XtermPalette } from '../theme/themes';

// How your vitals read in the panel footer and in the status line, from
// the rows under Layout, Vitals (VitalsOptions.dc.html). Values picks
// the form of each number, Meter the line under it, and Warn before you
// run low the thresholds that color it. Kept pure for the unit tests.

/** A vital's color. Quiet at rest, warn in the middle third while Warn
 *  before you run low is on, danger when it runs low. Hidden while the
 *  game hides the value, a tertiary `?` that never warns. */
export type VitalTone = 'quiet' | 'warn' | 'danger' | 'hidden';

/** The Group pane's thirds, by whole percent: quiet from 67, warn from
 *  34, danger under 34. */
export function thirdsTone(pct: number): VitalTone {
  if (pct >= 67) return 'quiet';
  if (pct >= 34) return 'warn';
  return 'danger';
}

/** A vital's tone. With Warn before you run low off it turns danger
 *  only while the vitals store's low latch holds (under 20 percent,
 *  until it climbs back to 25). With it on it follows the thirds. A
 *  vital with no max stays quiet either way. */
export function vitalTone(
  current: number,
  max: number,
  low: boolean,
  warnThirds: boolean,
): VitalTone {
  if (!warnThirds) return low ? 'danger' : 'quiet';
  if (max <= 0) return 'quiet';
  return thirdsTone(vitalPercent(current, max));
}

/** A vital's value in the form Values asks for: `186 / 1020`, `186`,
 *  or `18%` in whole percent. A vital with no max has no percent, so
 *  Percent shows the value alone. */
export function formatVital(values: VitalsValues, current: number, max: number): string {
  if (values === 'current') return String(current);
  if (values === 'percent') return max > 0 ? `${vitalPercent(current, max)}%` : String(current);
  return `${current} / ${max}`;
}

/** A vital the game hides, in the form Values asks for, with `?` for
 *  each number: `? / ?`, `?`, or `?%`. */
export function hiddenVital(values: VitalsValues): string {
  if (values === 'current') return '?';
  if (values === 'percent') return '?%';
  return '? / ?';
}

/** The widest a vital with this max reads, the vital at full. One line
 *  fits by it, so the row keeps its form while a value loses a digit in
 *  a fight. A hidden vital reads its hidden form. */
export function widestVital(values: VitalsValues, max: number, hidden = false): string {
  return hidden ? hiddenVital(values) : formatVital(values, max, max);
}

/** Meter fill in percent, unrounded so the meter moves smoothly. */
export function meterFill(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(100, (current / max) * 100));
}

/** How the vitals rows measure for a Meter choice, in CSS pixels. */
export interface VitalsGeometry {
  /** The row pitch. */
  row: number;
  /** Space above the 16 px text line inside a row. */
  rowTop: number;
  /** The meter's height. 0 draws no meter. */
  meter: number;
  /** Space between the text line and the meter. */
  meterGap: number;
  meterRadius: number;
  /** Space inside the footer above the first row and below the last. */
  padTop: number;
  padBottom: number;
}

const GEOMETRY: Record<VitalsMeter, VitalsGeometry> = {
  // The 2 px meter 1 px under the text on a 28 px pitch (SPEC 5, G3).
  line: { row: 28, rowTop: 4, meter: 2, meterGap: 1, meterRadius: 1, padTop: 8, padBottom: 11 },
  // Twice as thick and 2 px under the text, on the same pitch.
  bar: { row: 28, rowTop: 4, meter: 4, meterGap: 2, meterRadius: 2, padTop: 8, padBottom: 11 },
  // The panes' dense 22 px row with the text centered. 9 above and 13
  // below keep the text 12 and 16 from the footer's edges, as in Rows.
  none: { row: 22, rowTop: 3, meter: 0, meterGap: 0, meterRadius: 0, padTop: 9, padBottom: 13 },
};

export function vitalsGeometry(meter: VitalsMeter): VitalsGeometry {
  return GEOMETRY[meter];
}

/** The footer's height for `rows` rows, the 1 px line on top included.
 *  The footer holds this while it waits for your vitals, so logging in
 *  moves nothing. */
export function vitalsFooterHeight(geometry: VitalsGeometry, rows: number): number {
  return 1 + geometry.padTop + rows * geometry.row + geometry.padBottom;
}

/** The vitals you left on under Customize vitals, in your order. */
export function vitalsOn(order: readonly Vital[], off: readonly VitalOff[]): Vital[] {
  return order.filter((vital) => !off.includes(vital));
}

/** The vitals the footer draws of the ones you left on (vitalsOn).
 *  Health always shows. Mana and Moves drop while the game sends no
 *  max for them, except while it hides your vitals, when it sends every
 *  max as 0. */
export function shownVitals(vitals: Vitals, on: readonly Vital[]): Vital[] {
  return on.filter((vital) => vitals.hidden || vital === 'hp' || vitals[maxOf(vital)] > 0);
}

/** Each vital's name in the footer. */
export const VITAL_LABELS: Record<Vital, string> = { hp: 'Health', mana: 'Mana', move: 'Moves' };

/** Each style's name, as the gallery and the vitals menu write it. */
export const VITALS_STYLE_LABELS: Readonly<Record<VitalsStyle, string>> = {
  rows: 'Rows',
  line: 'One line',
  ledger: 'Ledger',
  gauges: 'Gauges',
  pips: 'Pips',
  bands: 'Bands',
  ladders: 'Ladders',
  blocks: 'Blocks',
  traces: 'Traces',
  dials: 'Dials',
  text: 'Text',
};

/** Each Values form's name, as Customize vitals and the vitals menu
 *  write it. */
export const VITALS_VALUES_LABELS: Readonly<Record<VitalsValues, string>> = {
  'current-max': 'Current and max',
  current: 'Current',
  percent: 'Percent',
};

/** What a pick of `style` saves. Rows and One line live in
 *  vitals_density, so an older build still reads them, and clear any
 *  style you picked before (Q15). */
export function vitalsStylePick(style: VitalsStyle): UiFields {
  return style === 'rows' || style === 'line'
    ? { vitals_density: style, vitals_style: null }
    : { vitals_style: style };
}

/** One vital as every footer style draws it. */
export interface ShownVital {
  key: Vital;
  current: number;
  max: number;
  /** The value in its Values form. */
  value: string;
  /** The widest the value reads, the vital at its max (widestVital). */
  widest: string;
  /** The mark's fill, null for an empty mark. */
  pct: number | null;
  tone: VitalTone;
}

/** The vitals the footer draws (shownVitals), each in the Values form
 *  and the tone `options` ask for (vitalRows). */
export function shownRows(
  vitals: Vitals,
  on: readonly Vital[],
  options: Pick<VitalsOptions, 'values' | 'warn_thirds'>,
): ShownVital[] {
  return vitalRows(vitals, shownVitals(vitals, on), options);
}

/** Each of `keys` in the Values form and the tone `options` ask for. A
 *  hidden vital reads `?` over an empty mark and never warns. The
 *  status line draws every vital you left on this way, with or without
 *  a max. */
export function vitalRows(
  vitals: Vitals,
  keys: readonly Vital[],
  options: Pick<VitalsOptions, 'values' | 'warn_thirds'>,
): ShownVital[] {
  return keys.map((key) => {
    const current = vitals[key];
    const max = vitals[maxOf(key)];
    const widest = widestVital(options.values, max, vitals.hidden);
    return vitals.hidden
      ? { key, current, max, value: hiddenVital(options.values), widest, pct: null, tone: 'hidden' }
      : {
          key,
          current,
          max,
          value: formatVital(options.values, current, max),
          widest,
          pct: meterFill(current, max),
          tone: vitalTone(current, max, vitals.low[key], options.warn_thirds),
        };
  });
}

/** The key of a vital's max in Char.Vitals. */
export function maxOf(vital: Vital): 'maxhp' | 'maxmana' | 'maxmove' {
  if (vital === 'hp') return 'maxhp';
  if (vital === 'mana') return 'maxmana';
  return 'maxmove';
}

/** How your opponent's health reads in the footer. */
export interface OpponentHealth {
  value: string;
  /** The meter's fill, null for an empty meter. */
  pct: number | null;
  /** The game withholds it, so it reads a quiet `?`. */
  hidden: boolean;
}

/** Your opponent's health: its percent, else the condition the game
 *  sends, else `?`. A health the game hides or sends nothing for reads
 *  `?` either way, so no style draws a blank. */
export function opponentHealth(
  combat: Pick<CombatOpponent, 'hp_pct' | 'condition' | 'hidden'>,
): OpponentHealth {
  if (!combat.hidden && combat.hp_pct !== null) {
    return { value: `${combat.hp_pct}%`, pct: combat.hp_pct, hidden: false };
  }
  if (!combat.hidden && combat.condition) {
    return { value: combat.condition, pct: null, hidden: false };
  }
  return { value: '?', pct: null, hidden: true };
}

/** The widest your opponent's health reads, at 100 percent, so a
 *  fight never moves a column. A condition or a `?` reads as itself. */
export function widestOpponentHealth(health: OpponentHealth): string {
  return health.pct === null ? health.value : '100%';
}

/** The contrast a vital's color holds on the panel, as a chat line's
 *  does (chatColors.ts). */
export const VITAL_COLOR_CONTRAST = 3;

/** The color each vital's label and mark draw in, for the vitals you
 *  gave one. A vital left out keeps the footer's own tones. */
export type VitalInks = Partial<Record<Vital, string>>;

/** The ground the footer draws on, from the theme's chrome tokens. */
export type VitalsGround = Pick<ChromeTokens, 'panel' | 'appearance'>;

/** Each color you picked under Customize vitals, the play palette's
 *  slot lifted to 3:1 on the panel at its own hue, lighter on a dark
 *  theme and darker on a light one. Numbers never take it, so every
 *  value keeps the text color. A slot or panel that does not parse as
 *  hex draws as given. */
export function vitalInks(
  colors: VitalsColors,
  palette: XtermPalette,
  ground: VitalsGround,
): VitalInks {
  const panel = parseHex(ground.panel);
  const dir = ground.appearance === 'dark' ? 1 : -1;
  const inks: VitalInks = {};
  for (const [vital, slot] of Object.entries(colors) as [Vital, number][]) {
    const color = palette[ANSI_SLOTS[slot]];
    const rgb = parseHex(color);
    inks[vital] = rgb && panel ? toHex(liftAtHue(rgb, panel, VITAL_COLOR_CONTRAST, dir)) : color;
  }
  return inks;
}

/** A vital's tone and color as the classes and the custom property
 *  the Ledger, Gauges and Pips rules in panel.css read: `is-low`,
 *  `is-warn` or `is-hidden` on `vitals-tone`, and `--vital-ink` for a
 *  color you picked. */
export function toneProps(
  tone: VitalTone,
  ink: string | undefined,
  className: string,
): { className: string; style?: CSSProperties } {
  const toned = tone === 'quiet' ? '' : ` is-${tone === 'danger' ? 'low' : tone}`;
  const classes = `${className} vitals-tone${toned}`;
  return ink
    ? { className: classes, style: { '--vital-ink': ink } as CSSProperties }
    : { className: classes };
}

/** Whether two names call the same mob, compared without case, as a
 *  target you set and the opponent Char.Combat names. */
export function sameMob(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

/** What the panel draws under its panes: every vital you left on, only
 *  your opponent's row in a fight, or nothing. With Show your vitals in
 *  on Status line it draws nothing and the panes reach the window's
 *  foot. While your prompt is pinned above the command line and Hide
 *  vitals while your prompt is pinned is on, it keeps the opponent row.
 *  Turning either off brings the vitals back. So does a pinned band with
 *  no prompt to show, with no capture or with prompts off in the game,
 *  since your vitals would then show nowhere. */
export function panelVitals(
  prompt: Pick<PromptShowState, 'show' | 'capture' | 'promptsOff'> | null,
  options: Pick<VitalsOptions, 'place' | 'hide_when_pinned'>,
): 'vitals' | 'opponent' | null {
  if (options.place === 'status') return null;
  const pinnedPrompt = prompt?.show === 'pinned' && prompt.capture && !prompt.promptsOff;
  return pinnedPrompt && options.hide_when_pinned ? 'opponent' : 'vitals';
}
