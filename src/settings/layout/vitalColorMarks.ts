import { ANSI_SLOT_LABELS } from '../../theme/appearanceSettings';
import { ANSI_SLOTS, type AnsiSlot } from '../../theme/baseAnsi';
import type { ChromeTokens } from '../../theme/chrome';
import { parseHex } from '../../theme/color';
import { PAIR_DE, seenApart, type ColorVision } from '../../theme/gameFit';
import type { XtermPalette } from '../../theme/themes';

// The marks in a vital's color list (Vitals Styles Q5, boards 2 and 6).
// A vital turns the low tone under one third, and with Warn before you
// run low on the warn tone under two thirds, so a color near either
// hides that. A color your vision sees within the fit's pair floor of
// the window's low tone says Like low, and while the warning is on, one
// as near its warn tone says Like warn. Both measure the play palette
// and the status colors as your Color vision sees them. Every color
// stays allowed, since the value still turns.

/** What a color in the list is near. */
export type ColorMark = 'low' | 'warn';

export const COLOR_MARK_WORDS: Readonly<Record<ColorMark, string>> = {
  low: 'Like low',
  warn: 'Like warn',
};

/** The slots of `palette` your `vision` sees near the low tone, and
 *  while `warn` is on, near the warn tone. Low wins a slot near both. */
export function colorMarks(
  palette: XtermPalette,
  tokens: Pick<ChromeTokens, 'danger' | 'warn'>,
  vision: ColorVision,
  warn: boolean,
): Partial<Record<AnsiSlot, ColorMark>> {
  const low = parseHex(tokens.danger);
  const warnTone = parseHex(tokens.warn);
  const marks: Partial<Record<AnsiSlot, ColorMark>> = {};
  for (const slot of ANSI_SLOTS) {
    const color = parseHex(palette[slot]);
    if (!color) continue;
    if (low && seenApart(color, low, vision) < PAIR_DE) marks[slot] = 'low';
    else if (warn && warnTone && seenApart(color, warnTone, vision) < PAIR_DE) marks[slot] = 'warn';
  }
  return marks;
}

/** One row of a vital's color list. */
export interface ColorChoice {
  /** The slot, or null for Default. */
  slot: number | null;
  label: string;
  /** The color its chip draws in, or null for Default's ring. */
  swatch: string | null;
  mark: ColorMark | undefined;
  checked: boolean;
}

/** Default, then the theme's sixteen in slot order on `palette`, each
 *  with its mark and a check on `picked`, the slot you picked or
 *  undefined for Default. */
export function colorChoices(
  palette: XtermPalette,
  marks: Partial<Record<AnsiSlot, ColorMark>>,
  picked: number | undefined,
): ColorChoice[] {
  return [
    { slot: null, label: 'Default', swatch: null, mark: undefined, checked: picked === undefined },
    ...ANSI_SLOTS.map((name, slot) => ({
      slot,
      label: ANSI_SLOT_LABELS[name],
      swatch: palette[name],
      mark: marks[name],
      checked: picked === slot,
    })),
  ];
}
