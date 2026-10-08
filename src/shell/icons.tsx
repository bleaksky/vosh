import type { TickCount } from '../ipc/uiConfig';
import { Glyph } from '../ui/icons';
import {
  SUN_ARC_PATH,
  SUN_DOWN,
  SUN_HORIZON_PATH,
  SUN_TRACK_OPACITY,
  SUN_UP_RADIUS,
  sunDot,
} from './sunPath';
import {
  TICK_RING_CENTER,
  TICK_RING_RADIUS,
  TICK_RING_TRACK_OPACITY,
  tickArc,
  tickArcLeft,
} from './tickRing';

// The glyphs only the main window draws, the panel toggle in the title
// band, Hide sessions and the snoop eye in the sessions sidebar, and the
// tick and the game time in the status line. They draw on Glyph from ui/icons.tsx, where
// the title band finds its other icons.

export function PanelIcon() {
  return (
    <Glyph>
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" />
      <path d="M10 2.75v10.5" />
    </Glyph>
  );
}

/** The panel glyph with its line on the left, for the sidebar there. */
export function SidebarIcon() {
  return (
    <Glyph>
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" />
      <path d="M6 2.75v10.5" />
    </Glyph>
  );
}

/** An eye, for the players a session snoops. At 12 px its strokes keep
 *  their 16 px weight, as the Snoop review draws it. */
export function EyeIcon({ size = 16 }: { size?: 12 | 16 }) {
  return (
    <Glyph size={size}>
      <path
        d="M1.75 8C3.25 5.1 5.35 3.75 8 3.75S12.75 5.1 14.25 8C12.75 10.9 10.65 12.25 8 12.25S3.25 10.9 1.75 8Z"
        vectorEffect="non-scaling-stroke"
      />
      <circle cx="8" cy="8" r="2" vectorEffect="non-scaling-stroke" />
    </Glyph>
  );
}

// Status line glyphs for the tick and the game time, drawn at 12 px.
// Unlike the title band chevron, their strokes scale with the icon, 1.25
// units on the 16 unit grid or about 0.94 px at 12 px, as the approved
// drawing has them. Held at 1.25 px, the open sun under the horizon
// fused with the horizon and its hole shrank to one device pixel.

interface SmallIconProps {
  /** Rendered size in px. The status line draws them at 12. */
  size?: 12 | 16;
}

interface TickRingIconProps extends SmallIconProps {
  /** The count the tick shows: whole seconds since the last tick
   *  counting up, or left until the next counting down. */
  secs: number;
  /** The tick interval in seconds, null while unknown. */
  interval: number | null;
  /** Which way the count runs. Up when left out. */
  count?: TickCount;
}

/** The tick. A faint ring, and on top an arc. Counting up it runs
 *  clockwise from 12 o clock as the seconds pass and closes when the
 *  tick is due. Counting down it is the share left, emptying clockwise
 *  toward 12 o clock, and only the faint ring shows while the tick is
 *  late. */
export function TickRingIcon({ secs, interval, count = 'up', size = 12 }: TickRingIconProps) {
  const arc = count === 'up' ? tickArc(secs, interval) : tickArcLeft(secs, interval);
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
