import type { PromptShowState } from '../ipc/prompt';
import type { Vital, VitalOff, VitalsColors, VitalsMeter, VitalsValues } from '../ipc/uiConfig';
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

/** The opponent Char.Combat names, as far as the status line needs it. */
export interface CombatHealth {
  name: string;
  hp_pct: number | null;
  /** The game withholds the opponent's health. */
  hidden?: boolean;
}

/** The target's health for the status line with the panel hidden. It
 *  shows only when the target you set is the one you are fighting, the
 *  Char.Combat opponent, with the names compared without case. Null
 *  otherwise, while the server sends no percent, and while the game
 *  withholds it. */
export function targetHealthPercent(
  target: string | null,
  opponent: CombatHealth | null,
): number | null {
  if (!target || opponent === null || opponent.hidden === true || opponent.hp_pct === null) {
    return null;
  }
  return sameName(target, opponent.name) ? opponent.hp_pct : null;
}

function sameName(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

/** Whether the panel draws its vitals under the panes. While your
 *  prompt is pinned above the command line and Hide vitals while your
 *  prompt is pinned is on, the footer goes and the panes take its room,
 *  all but the opponent row in a fight.
 *  Turning either off brings it back. So does a pinned band with no
 *  prompt to show, with no capture or with prompts off in the game,
 *  since your vitals would then show nowhere. */
export function panelShowsVitals(
  prompt: Pick<PromptShowState, 'show' | 'capture' | 'promptsOff'> | null,
  hideWhenPinned: boolean,
): boolean {
  const pinnedPrompt = prompt?.show === 'pinned' && prompt.capture && !prompt.promptsOff;
  return !(pinnedPrompt && hideWhenPinned);
}
