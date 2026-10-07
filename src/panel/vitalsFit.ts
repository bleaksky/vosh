import type { VitalsValues } from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import { vitalsLineFit, type VitalsLineFit } from './vitalsLine';
import { ledgerFigure, ledgerFit, type LedgerFit, type MeasureText } from './vitalsLedgerFit';
import { gaugesFit, pipsFit, type GaugesFit, type PipsFit } from './vitalsMarksFit';
import { opponentHealth, widestOpponentHealth, VITAL_LABELS, type ShownVital } from './vitalsView';

// How each drawn style fits a footer's width, for the footer under the
// panes and the tiles of the Style gallery in Settings.

/** The style the footer draws, with how it fits the panel's width. */
export type VitalsFit =
  | { style: 'rows' }
  | { style: 'line'; fit: VitalsLineFit }
  | { style: 'ledger'; fit: LedgerFit }
  | { style: 'gauges'; fit: GaugesFit }
  | { style: 'pips'; fit: PipsFit };

/** How `style` fits a footer `width` px wide at panel size `size`.
 *  The gallery in Settings fits its tiles with it too. */
export function vitalsFitOf(
  style: VitalsFit['style'],
  width: number,
  size: number,
  rows: readonly ShownVital[],
  combat: CombatOpponent | null,
  values: VitalsValues,
  measure: MeasureText,
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
  return { style };
}
