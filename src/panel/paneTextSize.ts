import { createContext, useContext } from 'react';

// Every pane draws at your panel size (Settings, Appearance, Panel
// text, Size), in the Panel font. The panes were drawn at 12 px, the
// size the panel starts at, and every length that sits with the text,
// a header, a label, a count, a row or a line or the gap between two,
// scales by your size over 12 and rounds to whole px, so the rows stay
// as dense at 16 px as at 12. At 12 px every length is its base,
// exactly as the panes drew before. The marks, the meters, the icons,
// and the space at the sides keep their px at every size.
//
// The main window writes your size on the root as --panel-text-px,
// and panel.css declares the lengths it draws on .panel-host as
// --mud-*, each round(Npx * var(--mud-scale), 1px) over a base below,
// or writes the same product in place. The pane geometry reads the
// same bases here, so a pane, its minimum, and the tests agree on
// every row. The status line under the terminal scales the same way
// (frame.css).

/** The size the panes were drawn at, and the size each base is
 *  measured at. */
export const PANE_TEXT_PX = 12;

/** Every length that sits with the text, in px at 12 px. The ones
 *  panel.css draws are --mud-* there, the key in kebab case. */
export const PANE_TEXT_BASE = {
  /** A line of text. */
  line: 16,
  /** A pane's header, the --pane-header token. */
  header: 28,
  /** A dense row, the --row token. */
  row: 22,
  /** A Timers first row. */
  affectsRow: 22,
  /** Above and below the hairline under your tracked slots. */
  affectsRuleGap: 4,
  /** The Timers first hours column, three cells wide. */
  affectsHours: 22,
  /** A Countdown row, the text and the meter under it. */
  countdownRow: 23,
  /** From a Countdown row's top to its meter. */
  countdownMeterTop: 19,
  /** The Countdown meter. */
  countdownMeter: 2,
  /** A chip, and a line of chips. */
  chip: 20,
  /** Between two lines of chips in a group. */
  chipLineGap: 4,
  /** Between two groups of chips. */
  chipGroupGap: 8,
  /** Above the first line of chips on a page. */
  chipsTop: 4,
  /** The gutter that holds a group's name. */
  chipGutter: 56,
  /** The narrowest Affects pane that draws two columns, and whose
   *  chips set the group names in a gutter. */
  twoColumns: 360,
  /** A chat line. */
  chatLine: 17,
  /** Between two chat messages. */
  chatGap: 3,
  /** Distance from the bottom of chat that still counts as reading the
   *  newest message. */
  chatSticky: 24,
} as const;

export type PaneTextKey = keyof typeof PANE_TEXT_BASE;

/** The lengths at one size, and the size. */
export type PaneText = Record<PaneTextKey, number> & { size: number };

/** A size you can draw at: your panel size, or 12 for anything else. */
export function paneTextSize(size: number | null | undefined): number {
  return typeof size === 'number' && Number.isFinite(size) && size > 0 ? size : PANE_TEXT_PX;
}

/** `px` at 12 px, at panel size `size` px, in whole px. panel.css
 *  rounds the same product: round(Npx * var(--mud-scale), 1px), where
 *  --mud-scale is the size over 12. Both round half up. */
export function textPx(px: number, size: number): number {
  return Math.round(px * (paneTextSize(size) / PANE_TEXT_PX));
}

const cache = new Map<number, PaneText>();

/** Every length at text `size` px. */
export function paneText(size: number = PANE_TEXT_PX): PaneText {
  const at = paneTextSize(size);
  const hit = cache.get(at);
  if (hit) return hit;
  const out = { size: at } as PaneText;
  for (const key of Object.keys(PANE_TEXT_BASE) as PaneTextKey[]) {
    out[key] = textPx(PANE_TEXT_BASE[key], at);
  }
  cache.set(at, out);
  return out;
}

/** Your panel size, which PanelHost hands every pane. 12 outside the
 *  panel, so a pane drawn alone, as in a test, draws as before. */
export const PaneTextSizeContext = createContext<number>(PANE_TEXT_PX);

/** The lengths at your panel size. */
export function usePaneText(): PaneText {
  return paneText(useContext(PaneTextSizeContext));
}
