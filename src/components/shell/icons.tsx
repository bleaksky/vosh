import type { ReactNode } from 'react';
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
// Like the chevron, the stroke keeps its 1.25 px weight at that size.

interface SmallIconProps {
  /** Rendered size in px. The status line draws them at 12. */
  size?: 12 | 16;
}

const keepStroke = (size: 12 | 16) =>
  size === 12 ? ({ vectorEffect: 'non-scaling-stroke' } as const) : {};

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
      <circle cx={c} cy={c} r={r} strokeOpacity={TICK_RING_TRACK_OPACITY} {...keepStroke(size)} />
      {arc.kind === 'whole' && <circle cx={c} cy={c} r={r} {...keepStroke(size)} />}
      {arc.kind === 'part' && <path d={arc.path} {...keepStroke(size)} />}
    </Glyph>
  );
}

/** Game time while the sun is up. A disc and eight short rays. */
export function SunIcon({ size = 12 }: SmallIconProps) {
  return (
    <Glyph size={size}>
      <circle cx="8" cy="8" r="2.75" {...keepStroke(size)} />
      <path
        d="M8 1.75v1.5M8 12.75v1.5M1.75 8h1.5M12.75 8h1.5M3.6 3.6l1.05 1.05M11.35 11.35l1.05 1.05M3.6 12.4l1.05-1.05M11.35 4.65l1.05-1.05"
        {...keepStroke(size)}
      />
    </Glyph>
  );
}

/** Game time after dark. A crescent on the 6.25 circle. */
export function MoonIcon({ size = 12 }: SmallIconProps) {
  return (
    <Glyph size={size}>
      <path
        d="M14.25 8.55A6.25 6.25 0 1 1 7.45 1.75a4.86 4.86 0 0 0 6.8 6.8z"
        {...keepStroke(size)}
      />
    </Glyph>
  );
}
