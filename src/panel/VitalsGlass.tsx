import { useId, type ReactNode } from 'react';

// The glass Vials and Orbs draw: a flat
// vessel in the vital's tone with a hairline rim and no shine. Inside
// it the liquid rises from the foot to `level`, the y in px its surface
// stands at, with a 1 px surface line, and eases there in 160 ms. With
// Show each hit on, the part a hit took stays pale above the surface,
// from `gone` down to it, and drains into it.

export function Glass({
  className,
  width,
  height,
  shape,
  level,
  gone,
  draining,
  surface,
}: {
  className: string;
  width: number;
  height: number;
  /** The vessel's outline, which clips everything inside. */
  shape: (props: { className?: string }) => ReactNode;
  /** Where the surface stands, or null for an empty vessel. */
  level: number | null;
  /** Where the pale part a hit left tops out, or null for none. */
  gone: number | null;
  draining: boolean;
  /** Draw the surface line, which a full or empty orb leaves out. */
  surface: boolean;
}) {
  const clip = `vitals-glass-${useId().replace(/[^\w-]/g, '')}`;
  return (
    <svg
      className={className}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      aria-hidden="true"
    >
      <defs>
        <clipPath id={clip}>{shape({})}</clipPath>
      </defs>
      <g clipPath={`url(#${clip})`}>
        <rect className="vitals-glass-wash" x="0" y="0" width={width} height={height} />
        {level !== null && gone !== null && gone < level && (
          <rect
            className={`vitals-glass-gone${draining ? ' is-draining' : ''}`}
            x="0"
            y={round(gone)}
            width={width}
            height={round(level - gone)}
          />
        )}
        {level !== null && (
          <g className="vitals-glass-level" style={{ transform: `translateY(${round(level)}px)` }}>
            <rect className="vitals-glass-liquid" x="0" y="0" width={width} height={height} />
            {surface && (
              <line className="vitals-glass-surface" x1="0" y1="0.5" x2={width} y2="0.5" />
            )}
          </g>
        )}
      </g>
      {shape({ className: 'vitals-glass-rim' })}
    </svg>
  );
}

const round = (n: number) => Math.round(n * 100) / 100;
