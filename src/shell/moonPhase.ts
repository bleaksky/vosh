// The lit part of a moon for its phase, on the 16 unit icon grid the
// status line draws the moons with. Aabahran counts eight phases. 0 is
// new, 1 to 3 grow, 4 is full, and 5 to 7 fade, so the angle from new
// is the phase times a quarter of pi.
//
// The disc sits at 8,8 with a 6.25 radius, the circle Vosh's icons
// share. The lit part runs along the limb on the lit side and
// back along the terminator, half an ellipse. A growing moon lights the
// right side and a fading one the left, the way a northern sky shows
// it. Before the half the terminator bows toward the light and leaves a
// crescent. After it, the terminator bows away and leaves a gibbous
// moon.
//
// The crescent and nearly full phases are drawn for size, not to the
// sky. The sky gives the terminator a horizontal radius of the disc
// radius times |cos(angle)|, which leaves a thin crescent 1.83 units
// wide, under two pixels at 14 px and too thin to read as lit. These
// phases draw it at MOON_DRAWN_TERMINATOR of the radius instead. That
// lights a 3.75 unit sliver at 1 and 7 and leaves one as dark at 3 and
// 5, about twice the true width, the way icon sets draw moon phases so
// each one reads at a glance. New, half, and full keep their true
// shape. litFraction still gives the true share of the disc the sky
// lights, and terminatorRx the radius the icon draws.
//
// The lit part is one solid shape, the limb and the terminator closed
// into a single path. A dark theme fills it whole with no stroke along
// its edge. A light theme cuts it out of the ink disc through a mask
// and rings the whole limb (MoonPhaseIcon).

export const MOON_PHASE_COUNT = 8;
export const MOON_CENTER = 8;
export const MOON_RADIUS = 6.25;
/** The terminator's horizontal radius at the crescent and nearly full
 *  phases, as a share of the disc radius. The sky gives 0.71 there. */
export const MOON_DRAWN_TERMINATOR = 0.4;

/** The unlit disc, the moon color at this opacity. */
export const MOON_UNLIT_OPACITY = 0.2;
/** The ring round the whole limb on a light theme. */
export const MOON_OUTLINE_WIDTH = 1.25;

export type MoonLitSide = 'left' | 'right';

export interface MoonPhaseShape {
  phase: number;
  /** Share of the disc the sky lights, 0 at new and 1 at full. The
   *  drawn crescents light more than this and the nearly full phases
   *  less. */
  litFraction: number;
  /** The side the light comes from. Null at new and full. */
  side: MoonLitSide | null;
  /** Horizontal radius of the terminator ellipse the icon draws. 0 at
   *  the half phases and the disc radius at new and full. */
  terminatorRx: number;
  /** SVG path data for the lit part, to fill. Null at new. */
  litPath: string | null;
}

const TOP = MOON_CENTER - MOON_RADIUS;
const BOTTOM = MOON_CENTER + MOON_RADIUS;

function num(n: number): string {
  return String(Math.round(n * 1000) / 1000);
}

/** The shape for a phase from 0 to 7. Null for anything else. */
export function moonPhaseShape(phase: number | null): MoonPhaseShape | null {
  if (phase === null || !Number.isInteger(phase) || phase < 0 || phase >= MOON_PHASE_COUNT) {
    return null;
  }
  const angle = (phase * Math.PI * 2) / MOON_PHASE_COUNT;
  const cos = Math.cos(angle);
  // cos(pi / 2) is not quite zero in floating point.
  const halfLit = Math.abs(cos) < 1e-9;
  const newOrFull = Math.abs(cos) > 1 - 1e-9;
  const terminatorRx = halfLit ? 0 : newOrFull ? MOON_RADIUS : MOON_RADIUS * MOON_DRAWN_TERMINATOR;
  const litFraction = phase === 0 ? 0 : phase === 4 ? 1 : (1 - cos) / 2;
  const r = num(MOON_RADIUS);
  const c = num(MOON_CENTER);
  const top = `${c} ${num(TOP)}`;
  const bottom = `${c} ${num(BOTTOM)}`;

  if (phase === 0) {
    return { phase, litFraction, side: null, terminatorRx, litPath: null };
  }
  if (phase === 4) {
    const disc = `M${top}A${r} ${r} 0 0 1 ${bottom}A${r} ${r} 0 0 1 ${top}Z`;
    return { phase, litFraction, side: null, terminatorRx, litPath: disc };
  }

  const side: MoonLitSide = phase < 4 ? 'right' : 'left';
  // Top to bottom along the limb on the lit side. In SVG, sweep 1 runs
  // clockwise on screen, through the right.
  const limbSweep = side === 'right' ? 1 : 0;
  const limb = `M${top}A${r} ${r} 0 0 ${limbSweep} ${bottom}`;
  // Bottom back to top along the terminator. A crescent's terminator
  // bows toward the light, a gibbous one's away from it.
  const crescent = cos > 0;
  const termSweep = crescent ? 1 - limbSweep : limbSweep;
  const terminator = halfLit ? `L${top}` : `A${num(terminatorRx)} ${r} 0 0 ${termSweep} ${top}`;
  const litPath = `${limb}${terminator}Z`;
  return { phase, litFraction, side, terminatorRx, litPath };
}
