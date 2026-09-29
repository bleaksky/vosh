import {
  MOON_CENTER,
  MOON_OUTLINE_OPACITY,
  MOON_OUTLINE_WIDTH,
  MOON_RADIUS,
  MOON_UNLIT_OPACITY,
  moonPhaseShape,
} from '../../lib/moonPhase';

// One moon at its phase, drawn in its own color. The whole disc shows
// faintly with a faint outline, so a new moon still reads as a moon.
// The lit part fills at full color on top, and the outline takes the
// full color along the lit limb. The status line draws it at 14 px.
// The label names the moon and its phase for a screen reader and shows
// on hover.

interface Props {
  /** 0 new, 1 to 3 growing, 4 full, 5 to 7 fading. Null draws the
   *  unlit disc alone. */
  phase: number | null;
  color: string;
  /** Like "Lysenties, half-lit and growing". */
  label: string;
  size?: number;
}

export function MoonPhaseIcon({ phase, color, label, size = 14 }: Props) {
  const shape = moonPhaseShape(phase);
  return (
    <svg
      className="shell-moon"
      width={size}
      height={size}
      viewBox="0 0 16 16"
      role="img"
      aria-label={label}
    >
      <title>{label}</title>
      <circle
        cx={MOON_CENTER}
        cy={MOON_CENTER}
        r={MOON_RADIUS}
        fill={color}
        fillOpacity={MOON_UNLIT_OPACITY}
        stroke={color}
        strokeOpacity={MOON_OUTLINE_OPACITY}
        strokeWidth={MOON_OUTLINE_WIDTH}
      />
      {shape?.litPath && <path d={shape.litPath} fill={color} />}
      {shape?.litLimbPath && (
        <path
          d={shape.litLimbPath}
          fill="none"
          stroke={color}
          strokeWidth={MOON_OUTLINE_WIDTH}
          strokeLinecap="butt"
        />
      )}
    </svg>
  );
}
