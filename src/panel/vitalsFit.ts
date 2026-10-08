import type { VitalsValues } from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import { vitalsLineFit, type VitalsLineFit } from './vitalsLine';
import { ledgerFigure, ledgerFit, type LedgerFit, type MeasureText } from './vitalsLedgerFit';
import { gaugesFit, pipsFit, type GaugesFit, type PipsFit } from './vitalsMarksFit';
import {
  dialsFit,
  orbsFit,
  ringsFit,
  vialsFit,
  rowMarkFit,
  type ColumnFit,
  type RingsFit,
  type RowMarkFit,
} from './vitalsDrawnFit';
import { opponentHealth, widestOpponentHealth, VITAL_LABELS, type ShownVital } from './vitalsView';

// How each drawn style fits a footer's width, for the footer under the
// panes and the tiles of the Style gallery in Settings.

/** The style the footer draws, with how it fits the panel's width. */
export type VitalsFit =
  | { style: 'rows' }
  | { style: 'line'; fit: VitalsLineFit }
  | { style: 'ledger'; fit: LedgerFit }
  | { style: 'gauges'; fit: GaugesFit }
  | { style: 'pips'; fit: PipsFit }
  | { style: 'bands' }
  | { style: RowStyle; fit: RowMarkFit }
  | { style: 'dials' | 'vials' | 'orbs'; fit: ColumnFit }
  | { style: 'rings'; fit: RingsFit };

/** The drawn styles that keep a row for each vital, its mark
 *  between its label and its value, or under both. */
const ROW_STYLES = ['ladders', 'blocks', 'traces', 'candles'] as const;
type RowStyle = (typeof ROW_STYLES)[number];

/** The drawn styles, which DrawnSection draws. */
const DRAWN = ['bands', ...ROW_STYLES, 'dials', 'rings', 'vials', 'orbs'] as const;

export type DrawnFit = Extract<VitalsFit, { style: (typeof DRAWN)[number] }>;

function isRowStyle(style: string): style is RowStyle {
  return (ROW_STYLES as readonly string[]).includes(style);
}

export function isDrawnFit(fit: VitalsFit): fit is DrawnFit {
  return (DRAWN as readonly string[]).includes(fit.style);
}

/** How `style` fits a footer `width` px wide at panel size `size`.
 *  `measureGame` measures in the game face, which Blocks draws its
 *  labels and values in. The gallery in Settings fits its tiles with it
 *  too. */
export function vitalsFitOf(
  style: VitalsFit['style'],
  width: number,
  size: number,
  rows: readonly ShownVital[],
  combat: CombatOpponent | null,
  values: VitalsValues,
  measure: MeasureText,
  measureGame: MeasureText,
): VitalsFit {
  if (style === 'line') {
    const items = rows.map((row) => ({
      label: measure(VITAL_LABELS[row.key], size, 400),
      value: measure(row.widest, size, 500),
    }));
    return { style, fit: rows.length === 0 ? 'rows' : vitalsLineFit(width, items) };
  }
  if (style === 'ledger') {
    const widest = rows.map((row) => ledgerFigure(values, row.max, row.max, row.tone === 'hidden'));
    return { style, fit: ledgerFit(width, size, widest, measure) };
  }
  if (style === 'gauges' || style === 'pips') {
    const labels = rows.map((row) => VITAL_LABELS[row.key]);
    const values = rows.map((row) => row.widest);
    if (combat) values.push(widestOpponentHealth(opponentHealth(combat)));
    return style === 'gauges'
      ? { style, fit: gaugesFit(width, size, labels, values, measure) }
      : { style, fit: pipsFit(width, size, labels, values, measure) };
  }
  if (isRowStyle(style)) {
    const labels = rows.map((row) => VITAL_LABELS[row.key]);
    return {
      style,
      fit: rowMarkFit(
        width,
        size,
        labels,
        rows.map((row) => row.widest),
        style === 'blocks' ? measureGame : measure,
      ),
    };
  }
  if (style === 'dials') return { style, fit: dialsFit(width, rows.length) };
  if (style === 'vials' || style === 'orbs') {
    const widest = rows.map((row) => ({
      label: VITAL_LABELS[row.key],
      figure: ledgerFigure(values, row.max, row.max, row.tone === 'hidden'),
    }));
    return style === 'vials'
      ? { style, fit: vialsFit(width, size, widest, measure) }
      : { style, fit: orbsFit(width, size, widest, measure) };
  }
  if (style === 'rings') {
    const labels = rows.map((row) => VITAL_LABELS[row.key]);
    const widest = rows.map((row) => row.widest);
    return { style, fit: ringsFit(width, size, labels, widest, measure) };
  }
  return { style };
}
