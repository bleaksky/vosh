// Shared bits of the four Ember vitals layouts (ledger, gauges, pips,
// strip). The live card in components/VitalsBar.tsx and the Settings
// preview in components/VitalsSettings.tsx draw the same anatomy, so
// the label words, the low-state thresholds, the override resolution,
// and the pip cell math live here rather than in either file.

import type { VitalsConfig, VitalsLayout } from './session';
import { hexToRgb } from './vitalsColor';

/** Spelled-out labels for the ledger columns and the gauge rows,
 *  where the caps label sits on its own rather than beside a bar. */
export const LEDGER_LABELS: Record<string, string> = {
  hp: 'hp',
  mn: 'mana',
  mv: 'moves',
};

/** Two-letter labels for the pip rows and the strip segments. */
export const SHORT_LABELS: Record<string, string> = {
  hp: 'hp',
  mn: 'mn',
  mv: 'mv',
};

/** Layouts whose anatomy is fixed. The columns, bar style, glyph and
 *  width settings do not apply to them; color overrides and the low
 *  hp vignette still do. */
export const FIXED_LAYOUTS: readonly VitalsLayout[] = ['ember', 'gauges', 'pips', 'strip'];

export function isFixedLayout(layout: VitalsLayout): boolean {
  return FIXED_LAYOUTS.includes(layout);
}

/** Below this percent a vital enters the low state. */
export const LEDGER_LOW_ENTER = 20;
/** A low vital leaves the state only once it climbs back to this
 *  percent, so regen straddling the line does not flicker. */
export const LEDGER_LOW_EXIT = 25;

/** The user's override color for a vital, or empty when unset or not
 *  a hex color the picker would accept. The layouts draw the theme
 *  token otherwise. */
export function ledgerOverride(label: string, config?: VitalsConfig): string {
  if (!config) return '';
  const hex =
    label === 'hp'
      ? config.hp_color
      : label === 'mn'
        ? config.mn_color
        : label === 'mv'
          ? config.mv_color
          : '';
  return hex && hexToRgb(hex) ? hex.trim() : '';
}

/** Signed per-tick change. Empty at zero so a resting vital shows no
 *  delta at all. Uses the real minus sign so losses read as a glyph,
 *  not a hyphen. */
export function formatLedgerDelta(delta: number): string {
  if (delta > 0) return `+${delta}`;
  if (delta < 0) return `\u2212${Math.abs(delta)}`;
  return '';
}

export interface PipCell {
  kind: 'full' | 'part' | 'empty';
  /** Fill width of a partial cell, 0 to 100. */
  part: number;
}

/** Ten cells for a percent. Whole tens fill, the next cell carries
 *  the remainder as a partial fill, the rest stay hollow. */
export function pipCells(percent: number): PipCell[] {
  const p = Math.max(0, Math.min(100, Math.round(percent)));
  const full = Math.floor(p / 10);
  const rem = p - full * 10;
  const cells: PipCell[] = [];
  for (let i = 0; i < 10; i++) {
    if (i < full) cells.push({ kind: 'full', part: 100 });
    else if (i === full && rem > 0) cells.push({ kind: 'part', part: rem * 10 });
    else cells.push({ kind: 'empty', part: 0 });
  }
  return cells;
}
