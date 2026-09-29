import type { ReactNode } from 'react';
import {
  SUN_ARC_PATH,
  SUN_DOWN,
  SUN_HORIZON_PATH,
  SUN_TRACK_OPACITY,
  SUN_UP_RADIUS,
  sunDot,
} from './sunPath';
import { TICK_RING_CENTER, TICK_RING_RADIUS, TICK_RING_TRACK_OPACITY, tickArc } from './tickRing';

// The One Window icon set (SPEC 6): 16 unit strokes at 1.25, round caps
// and joins, drawn in currentColor so each button sets the tone.

function Glyph({ size = 16, children }: { size?: number; children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.25"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function PlusIcon() {
  return (
    <Glyph>
      <path d="M8 3.5v9M3.5 8h9" />
    </Glyph>
  );
}

export function SearchIcon() {
  return (
    <Glyph>
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5l3.5 3.5" />
    </Glyph>
  );
}

export function PanelIcon() {
  return (
    <Glyph>
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" />
      <path d="M10 2.75v10.5" />
    </Glyph>
  );
}

/** The 12 px chevron after the session title. The stroke keeps its
 *  1.25 px weight at the smaller size. */
export function ChevronDownIcon() {
  return (
    <Glyph size={12}>
      <path d="M4.5 6.25L8 9.75l3.5-3.5" vectorEffect="non-scaling-stroke" />
    </Glyph>
  );
}

// Window controls for the frameless window on Windows and Linux.

export function MinimizeIcon() {
  return (
    <Glyph>
      <path d="M4 8h8" />
    </Glyph>
  );
}

export function MaximizeIcon() {
  return (
    <Glyph>
      <rect x="4" y="4" width="8" height="8" rx="1" />
    </Glyph>
  );
}

export function CloseIcon() {
  return (
    <Glyph>
      <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />
    </Glyph>
  );
}

// Status line glyphs for the tick and the game time, drawn at 12 px.
// Unlike the chevron, their strokes scale with the icon, 1.25 units on
// the 16 unit grid or about 0.94 px at 12 px, as the approved drawing
// has them. Held at 1.25 px, the open sun under the horizon fused with
// the horizon and its hole shrank to one device pixel.

interface SmallIconProps {
  /** Rendered size in px. The status line draws them at 12. */
  size?: 12 | 16;
}

interface TickRingIconProps extends SmallIconProps {
  /** Whole seconds since the last tick. */
  secs: number;
  /** The tick interval in seconds, null while unknown. */
  interval: number | null;
}

/** The tick. A faint ring, and on top an arc from 12 o clock that runs
 *  clockwise as the seconds pass and closes when the tick lands. */
export function TickRingIcon({ secs, interval, size = 12 }: TickRingIconProps) {
  const arc = tickArc(secs, interval);
  const c = TICK_RING_CENTER;
  const r = TICK_RING_RADIUS;
  return (
    <Glyph size={size}>
      <circle cx={c} cy={c} r={r} strokeOpacity={TICK_RING_TRACK_OPACITY} />
      {arc.kind === 'whole' && <circle cx={c} cy={c} r={r} />}
      {arc.kind === 'part' && <path d={arc.path} />}
    </Glyph>
  );
}

interface SunPathIconProps extends SmallIconProps {
  /** The 0..23 game hour, null while unknown. */
  hour: number | null;
  /** Whether the sun is up, null while unknown. */
  daytime: boolean | null;
}

/** Two decimals, enough for a 16 unit grid drawn at 12 px. */
const round2 = (n: number) => Math.round(n * 100) / 100;

/** The game time. A horizon with the sun's path over it, and the sun
 *  on the path for the hour, or under the horizon after dark. */
export function SunPathIcon({ hour, daytime, size = 12 }: SunPathIconProps) {
  const dot = sunDot(hour, daytime);
  return (
    <Glyph size={size}>
      <path d={SUN_ARC_PATH} strokeOpacity={SUN_TRACK_OPACITY} />
      <path d={SUN_HORIZON_PATH} />
      {dot.kind === 'up' && (
        <circle
          cx={round2(dot.x)}
          cy={round2(dot.y)}
          r={SUN_UP_RADIUS}
          fill="currentColor"
          stroke="none"
        />
      )}
      {dot.kind === 'down' && <circle cx={SUN_DOWN.x} cy={SUN_DOWN.y} r={SUN_DOWN.r} />}
    </Glyph>
  );
}
