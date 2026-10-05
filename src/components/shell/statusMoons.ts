import type { ChromeTokens } from '../../theme/chrome';
import { moonAlignment, moonTitle, type Moons } from '../../stores/gmcp/worldStore';
import type { XtermPalette } from '../../theme/themes';
import { moonColor } from './moonColors';
import type { ClockMoons } from './StatusClock';

/** The moons the status line shows. Only the moons in the sky, raised
 *  with moonrise, since a dormant moon has no phase worth showing. Each
 *  keeps the server's order and takes its color from the theme and its
 *  words from the phase. Null when none is in the sky. */
export function statusMoons(
  moons: Moons | null,
  palette: XtermPalette,
  tokens: Pick<ChromeTokens, 'bg' | 'appearance' | 'tertiary'>,
): ClockMoons | null {
  const up = moons?.moons.filter((moon) => moon.active) ?? [];
  if (!moons || up.length === 0) return null;
  return {
    moons: up.map((moon) => ({
      name: moon.name,
      phase: moon.phase,
      color: moonColor(moon.name, palette, tokens),
      label: moonTitle(moon),
    })),
    alignment: moonAlignment(moons),
    onLight: tokens.appearance === 'light',
  };
}
