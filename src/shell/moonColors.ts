import { liftToContrast, STATUS_CONTRAST, type ChromeTokens } from '../theme/chrome';
import { parseHex, toHex } from '../theme/color';
import type { XtermPalette } from '../theme/themes';

// Each moon in the status line takes the color the game gives its name.
// Lysenties shows in bright white (ANSI 15), Nercuros in bright cyan
// (14), and Dyphrities in red (1), so the status line reads each one
// from that slot of your theme. The slot moves in lightness until the
// icon holds 3:1 on the status line ground, the floor the chrome gives
// status marks. That matters on a light theme, where a bright slot can
// sit close to the paper, and for a deep red on a dark one (Gruvbox,
// Tango, Classic Vivid).

/** The ANSI slots the moons read from. */
export type MoonSlot = keyof Pick<XtermPalette, 'brightWhite' | 'brightCyan' | 'red'>;

/** Each Aabahran moon, lowercase, and its slot. */
export const MOON_SLOTS: ReadonlyMap<string, MoonSlot> = new Map<string, MoonSlot>([
  ['lysenties', 'brightWhite'],
  ['nercuros', 'brightCyan'],
  ['dyphrities', 'red'],
]);

/** The slot for a moon name, whatever its case. Null for a moon the
 *  table does not know. */
export function moonSlot(name: string): MoonSlot | null {
  return MOON_SLOTS.get(name.toLowerCase()) ?? null;
}

/** The color a moon's icon draws in. Its slot lifted to 3:1 on the
 *  ground, or the tertiary tone for a moon the table does not know or
 *  a slot that does not parse. */
export function moonColor(
  name: string,
  palette: XtermPalette,
  tokens: Pick<ChromeTokens, 'bg' | 'appearance' | 'tertiary'>,
): string {
  const slot = moonSlot(name);
  const rgb = slot ? parseHex(palette[slot]) : null;
  if (!rgb) return tokens.tertiary;
  const bg = parseHex(tokens.bg);
  if (!bg) return toHex(rgb);
  const dir = tokens.appearance === 'dark' ? 1 : -1;
  return toHex(liftToContrast(rgb, bg, STATUS_CONTRAST, dir));
}
