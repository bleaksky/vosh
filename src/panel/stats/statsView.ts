import { STATS_STYLES, type StatsStyle } from '../../ipc/uiConfig';
import type { ArmorKey, SaveKey, StatKey, StatValue } from '../../stores/gmcp/statsStore';
import type { MenuChoice } from '../affects/affectsDisplay';

// What the Stats pane reads off Char.Stats, apart from how it draws.
// Pure, so every style reads the same words and tones and a test pins
// them. The tones follow the score sheet. A stat above what you trained
// reads green and one below it yellow, as `score` colors them. Armor
// that counts under 80% reads yellow and under 50% red, the sheet's own
// limits. A save above zero works against you and reads red.

/** The names Settings and the pane menu give each style. */
export const STATS_STYLE_LABELS: Readonly<Record<StatsStyle, string>> = {
  sheet: 'Score sheet',
  rows: 'Rows',
  ledger: 'Ledger',
};

/** The pane menu's style choices, checked on `style`. */
export function statsStyleChoices(style: StatsStyle): MenuChoice<StatsStyle>[] {
  return STATS_STYLES.map((value) => ({
    value,
    label: STATS_STYLE_LABELS[value],
    checked: value === style,
  }));
}

export const STAT_NAMES: Readonly<Record<StatKey, string>> = {
  str: 'Strength',
  int: 'Intelligence',
  wis: 'Wisdom',
  dex: 'Dexterity',
  con: 'Constitution',
};

/** The three letters the score sheet writes each stat with. */
export const STAT_SHORT: Readonly<Record<StatKey, string>> = {
  str: 'Str',
  int: 'Int',
  wis: 'Wis',
  dex: 'Dex',
  con: 'Con',
};

export const ARMOR_NAMES: Readonly<Record<ArmorKey, string>> = {
  slash: 'Slash',
  pierce: 'Pierce',
  blunt: 'Blunt',
  magic: 'Magic',
};

export const SAVE_NAMES: Readonly<Record<SaveKey, string>> = {
  spell: 'Spell',
  afflictive: 'Afflictive',
  maledictive: 'Maledictive',
  mental: 'Mental',
  breath: 'Breath',
};

/** A tone class's word, or '' for the plain one. */
export type StatsTone = '' | 'up' | 'down' | 'warn' | 'danger' | 'quiet';

/** Green above what you trained, yellow below it. */
export function statTone(s: StatValue): StatsTone {
  if (s.cur > s.perm) return 'up';
  if (s.cur < s.perm) return 'down';
  return '';
}

/** Plain from 80%, yellow from 50%, red under it. */
export function armorTone(pct: number | null): StatsTone {
  if (pct === null || pct >= 80) return '';
  return pct >= 50 ? 'warn' : 'danger';
}

/** Red above zero, dim at zero, plain below it. */
export function saveTone(value: number): StatsTone {
  if (value > 0) return 'danger';
  return value === 0 ? 'quiet' : '';
}

/** ` is-<tone>`, or '' for the plain tone. */
export function toneClass(tone: StatsTone): string {
  return tone ? ` is-${tone}` : '';
}

/** How far `value` sits from 3, the lowest a stat goes, toward `max`,
 *  as a percent from 0 to 100. */
function fromThree(value: number, max: number): number {
  if (max <= 3) return 100;
  return Math.min(100, Math.max(0, (100 * (value - 3)) / (max - 3)));
}

/** A stat meter's fill, the stat in use from 3 toward its max. */
export function statFill(s: StatValue): number {
  return fromThree(s.cur, s.max);
}

/** Where a stat meter marks what you trained. */
export function trainedAt(s: StatValue): number {
  return fromThree(s.perm, s.max);
}

/** A number with its sign, a true minus below zero and a plus above,
 *  as armor and saves read. */
export function signed(value: number): string {
  if (value < 0) return `−${-value}`;
  return value > 0 ? `+${value}` : '0';
}

/** A stat as the pointer and a screen reader read it. */
export function statTitle(key: StatKey, s: StatValue): string {
  const over = s.over > 0 ? `, ${s.over} past the max, which counts for nothing` : '';
  return `${STAT_NAMES[key]} ${s.cur} of ${s.max}, trained to ${s.perm}${over}`;
}

/** An armor class as the pointer reads it. */
export function armorTitle(key: ArmorKey, ac: number, pct: number | null): string {
  const counts = pct === null ? '' : `, ${pct}% of it counts`;
  return `${ARMOR_NAMES[key]} armor ${signed(ac)}${counts}`;
}

/** A save as the pointer reads it. */
export function saveTitle(key: SaveKey, value: number): string {
  const word = value > 0 ? ', against you' : '';
  return `${SAVE_NAMES[key]} save ${signed(value)}${word}`;
}
