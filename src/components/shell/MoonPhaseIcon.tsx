import { useId } from 'react';
import {
  MOON_CENTER,
  MOON_OUTLINE_WIDTH,
  MOON_RADIUS,
  MOON_UNLIT_OPACITY,
  moonPhaseShape,
} from '../../lib/moonPhase';

// One moon at its phase, drawn in its own color. On a dark theme the
// moon glows. The whole disc shows faintly, so a new moon still reads
// as a moon, and the lit part fills at full color on top as one solid
// shape. No ring runs round it, so a full moon is a solid disc. On a
// light theme it is ink on paper, the way a printed calendar draws the
// moon. The outline runs the whole limb and the dark part fills in, so
// a new moon is a solid disc and a full moon an open ring. The status
// line draws it at 14 px. The label names the moon and its phase for a
// screen reader and shows on hover.

interface Props {
  /** 0 new, 1 to 3 growing, 4 full, 5 to 7 fading. Null draws the
   *  unlit disc alone. */
  phase: number | null;
  color: string;
  /** Like "Lysenties, half-lit and growing". */
  label: string;
  size?: number;
  /** Draw as ink on a light theme. */
  onLight?: boolean;
}

export function MoonPhaseIcon({ phase, color, label, size = 14, onLight = false }: Props) {
  const shape = moonPhaseShape(phase);
  // React ids carry characters a url() reference would need escaped.
  const maskId = `moon-${useId().replace(/[^a-zA-Z0-9_-]/g, '')}`;
  if (onLight) {
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
        {phase !== null && (
          <>
            <mask id={maskId}>
              <circle cx={MOON_CENTER} cy={MOON_CENTER} r={MOON_RADIUS} fill="white" />
              {shape?.litPath && <path d={shape.litPath} fill="black" />}
            </mask>
            <circle
              cx={MOON_CENTER}
              cy={MOON_CENTER}
              r={MOON_RADIUS}
              fill={color}
              mask={`url(#${maskId})`}
            />
          </>
        )}
        <circle
          cx={MOON_CENTER}
          cy={MOON_CENTER}
          r={MOON_RADIUS}
          fill={phase === null ? color : 'none'}
          fillOpacity={phase === null ? MOON_UNLIT_OPACITY : undefined}
          stroke={color}
          strokeWidth={MOON_OUTLINE_WIDTH}
        />
      </svg>
    );
  }
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
      />
      {shape?.litPath && <path d={shape.litPath} fill={color} />}
    </svg>
  );
}
