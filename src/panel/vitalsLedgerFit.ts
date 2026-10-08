import type { VitalsValues } from '../ipc/uiConfig';
import { textPx } from './paneTextSize';
import { formatVital, hiddenVital } from './vitalsView';

// The Ledger vitals style. Your vitals
// stand in columns, each the pane label caps over a figure of
// round(16 px x your size over 12), the max beside it at the caps size,
// and a line under it. Ledger never stacks. A column that runs short
// drops the max, then sets the figure at 14 px and then at the text
// size. Every column measures at its vital's max, digits as zeros, so a
// fight never moves a column. Kept pure for the unit tests.

/** How wide `text` draws at `px` px in the panel face and `weight`,
 *  digits measured as zeros, as VitalsFooter's textWidth does. */
export type MeasureText = (text: string, px: number, weight: number) => number;

/** A footer's side insets, 18 on the left and 12 on the right. */
export const FOOTER_INSETS = 30;
/** Space between two Ledger columns. */
export const LEDGER_GAP = 16;
/** Space between a Ledger figure and its max. */
export const LEDGER_MAX_GAP = 3;

/** A Ledger figure: the number in large type, and the max beside it in
 *  small type, or null for a Values form with no max. */
export interface LedgerFigure {
  current: string;
  max: string | null;
}

/** A vital's figure in the form Values asks for. Current and max sets
 *  `/ 1020` beside the number, and a hidden vital reads `?` and `/ ?`. */
export function ledgerFigure(
  values: VitalsValues,
  current: number,
  max: number,
  hidden: boolean,
): LedgerFigure {
  if (values !== 'current-max') {
    return { current: hidden ? hiddenVital(values) : formatVital(values, current, max), max: null };
  }
  return hidden ? { current: '?', max: '/ ?' } : { current: String(current), max: `/ ${max}` };
}

/** How Ledger fits the panel: in full, without the max, with the
 *  figure at 14 px, or with the figure at the text size. The last two
 *  drop the max as well. */
export type LedgerFit = 'full' | 'bare' | 'smaller' | 'text';

/** The figure's size in px for a fit at panel size `size` px. */
export function ledgerFigurePx(fit: LedgerFit, size: number): number {
  if (fit === 'text') return size;
  return textPx(fit === 'smaller' ? 14 : 16, size);
}

/** The first fit where every column holds its widest figure, each at
 *  its vital's max. The text size is the last step, so Ledger keeps its
 *  columns even where a figure overruns. */
export function ledgerFit(
  width: number,
  size: number,
  widest: readonly LedgerFigure[],
  measure: MeasureText,
): LedgerFit {
  if (widest.length === 0) return 'full';
  const gaps = LEDGER_GAP * (widest.length - 1);
  const column = (width - FOOTER_INSETS - gaps) / widest.length;
  const fits = (fit: LedgerFit) =>
    widest.every((figure) => {
      const current = measure(figure.current, ledgerFigurePx(fit, size), 500);
      const max =
        fit === 'full' && figure.max !== null
          ? LEDGER_MAX_GAP + measure(figure.max, textPx(10, size), 500)
          : 0;
      return current + max <= column;
    });
  const steps: LedgerFit[] = ['full', 'bare', 'smaller'];
  return steps.find(fits) ?? 'text';
}

/** The footer's height for Ledger's columns alone, the 1 px line on top
 *  included, as panel.css draws them: the caps, the figure and the
 *  line under it with a `meter` px meter, between the footer's pads. It
 *  holds this while it waits for your vitals. */
export function ledgerHeight(size: number, meter: number): number {
  const line = meter > 0 ? textPx(4, size) + meter : 0;
  return (
    1 +
    textPx(10, size) +
    textPx(12, size) +
    textPx(3, size) +
    textPx(20, size) +
    line +
    textPx(12, size)
  );
}
