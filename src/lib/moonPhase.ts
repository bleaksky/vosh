// The lit part of a moon for its phase, on the 16 unit icon grid the
// status line draws the moons with. Aabahran counts eight phases. 0 is
// new, 1 to 3 grow, 4 is full, and 5 to 7 fade, so the angle from new
// is the phase times a quarter of pi.
//
// The disc sits at 8,8 with a 6.25 radius, the circle the One Window
// icons share. The lit part runs along the limb on the lit side and
// back along the terminator, half an ellipse whose horizontal radius is
// the disc radius times |cos(angle)|. A growing moon lights the right
// side and a fading one the left, the way a northern sky shows it.
// Before the half the terminator bows toward the light and leaves a
// crescent. After it, the terminator bows away and leaves a gibbous
// moon.
//
// The outline around the disc is faint on the dark limb and takes the
// full color on the lit limb. A thin crescent is under two units wide,
// about one pixel at 14 px, and the lit limb beside it is what lets it
// read as lit rather than as the outline.

export const MOON_PHASE_COUNT = 8;
export const MOON_CENTER = 8;
export const MOON_RADIUS = 6.25;

/** The unlit disc, the moon color at this opacity. */
export const MOON_UNLIT_OPACITY = 0.22;
/** The outline around the dark limb, at this opacity. */
export const MOON_OUTLINE_OPACITY = 0.55;
export const MOON_OUTLINE_WIDTH = 1.25;

export type MoonLitSide = 'left' | 'right';

export interface MoonPhaseShape {
  phase: number;
  /** Share of the disc that is lit, 0 at new and 1 at full. */
  litFraction: number;
  /** The side the light comes from. Null at new and full. */
  side: MoonLitSide | null;
  /** Horizontal radius of the terminator ellipse. */
  terminatorRx: number;
  /** SVG path data for the lit part, to fill. Null at new. */
  litPath: string | null;
  /** SVG path data for the limb on the lit side, to stroke. The whole
   *  circle at full. Null at new. */
  litLimbPath: string | null;
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
  const terminatorRx = halfLit ? 0 : MOON_RADIUS * Math.abs(cos);
  const litFraction = phase === 0 ? 0 : phase === 4 ? 1 : (1 - cos) / 2;
  const r = num(MOON_RADIUS);
  const c = num(MOON_CENTER);
  const top = `${c} ${num(TOP)}`;
  const bottom = `${c} ${num(BOTTOM)}`;

  if (phase === 0) {
    return { phase, litFraction, side: null, terminatorRx, litPath: null, litLimbPath: null };
  }
  if (phase === 4) {
    const disc = `M${top}A${r} ${r} 0 0 1 ${bottom}A${r} ${r} 0 0 1 ${top}Z`;
    return { phase, litFraction, side: null, terminatorRx, litPath: disc, litLimbPath: disc };
  }

  const side: MoonLitSide = phase < 4 ? 'right' : 'left';
  // Top to bottom along the limb on the lit side. In SVG, sweep 1 runs
  // clockwise on screen, through the right.
  const limbSweep = side === 'right' ? 1 : 0;
  const litLimbPath = `M${top}A${r} ${r} 0 0 ${limbSweep} ${bottom}`;
  // Bottom back to top along the terminator. A crescent's terminator
  // bows toward the light, a gibbous one's away from it.
  const crescent = cos > 0;
  const termSweep = crescent ? 1 - limbSweep : limbSweep;
  const terminator = halfLit ? `L${top}` : `A${num(terminatorRx)} ${r} 0 0 ${termSweep} ${top}`;
  const litPath = `${litLimbPath}${terminator}Z`;
  return { phase, litFraction, side, terminatorRx, litPath, litLimbPath };
}
