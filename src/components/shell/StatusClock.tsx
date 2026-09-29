import type { CSSProperties, ReactNode } from 'react';
import type { ChipStyle } from '../../lib/session';
import { MoonIcon, SunIcon, TickRingIcon } from './icons';
import { MoonPhaseIcon } from './MoonPhaseIcon';

// The tick, the game time, and the moons as one status line item,
// drawn from plain values so a test can render it without the stores.
// StatusLine reads the stores and the theme and hands them in.

export interface ClockTick {
  /** Whole seconds since the last tick. */
  secs: number;
  /** Inside the warn window before the next tick. */
  warn: boolean;
  /** The tick interval in seconds the ring fills against, null while
   *  unknown. */
  interval: number | null;
}

export interface ClockTime {
  /** The game time as the status line shows it, like 8:42. */
  text: string;
  /** The daylight tint, or null for the plain value color. */
  tint: string | null;
  /** Whether the sun is up, null while unknown. */
  daytime: boolean | null;
}

export interface ClockMoon {
  name: string;
  phase: number | null;
  /** The moon's color from the theme. */
  color: string;
  /** Like "Lysenties, half-lit and growing". */
  label: string;
}

export interface ClockMoons {
  /** The moons in the sky, in the order the server lists them. */
  moons: ClockMoon[];
  /** Eclipse, Triad, or Near alignment, or null for a quiet sky. */
  alignment: string | null;
  /** Draw the moons as ink on a light theme. */
  onLight?: boolean;
}

interface Props {
  style: ChipStyle;
  tick: ClockTick | null;
  time: ClockTime | null;
  moons?: ClockMoons | null;
}

/** The tick, the game time, and the moons as one item, in that order,
 *  8 px apart. Renders nothing while none of them is known. */
export function StatusClock({ style, tick, time, moons = null }: Props) {
  const sky = moons && moons.moons.length > 0 ? moons : null;
  if (!tick && !time && !sky) return null;
  return (
    <span className="shell-status-clock">
      {tick && (
        <Reading
          style={style}
          caption="Tick"
          icon={<TickRingIcon secs={tick.secs} interval={tick.interval} />}
          warn={tick.warn}
          value={`${tick.secs}s`}
        />
      )}
      {time && (
        <Reading
          style={style}
          caption="Time"
          icon={time.daytime === false ? <MoonIcon /> : <SunIcon />}
          value={time.text}
          valueStyle={time.tint ? { color: time.tint } : undefined}
        />
      )}
      {sky && <Moons style={style} moons={sky} />}
    </span>
  );
}

/** The moons at 14 px, 4 px apart, after their caption in the Caption
 *  style, then the sky's one word in the warn tone. The icons stand in
 *  for values, so Value and Icon look the same. */
function Moons({ style, moons }: { style: ChipStyle; moons: ClockMoons }) {
  return (
    <span className="shell-status-part">
      <span className={style === 'caption_value' ? undefined : 'shell-sr'}>Moons</span>
      <span className="shell-status-moons">
        {moons.moons.map((moon) => (
          <MoonPhaseIcon
            key={moon.name}
            phase={moon.phase}
            color={moon.color}
            label={moon.label}
            onLight={moons.onLight === true}
          />
        ))}
      </span>
      {moons.alignment && <span className="shell-status-alignment">{moons.alignment}</span>}
    </span>
  );
}

interface ReadingProps {
  style: ChipStyle;
  caption: string;
  icon: ReactNode;
  value: string;
  warn?: boolean;
  valueStyle?: CSSProperties | undefined;
}

/** One value with its caption or icon, 6 px before it. A screen reader
 *  hears the caption in every style. */
function Reading({ style, caption, icon, value, warn = false, valueStyle }: ReadingProps) {
  return (
    <span className={`shell-status-part${warn ? ' is-warn' : ''}`}>
      {style === 'icon_value' && icon}
      <span className={style === 'caption_value' ? undefined : 'shell-sr'}>{caption}</span>
      <span className="shell-status-value" style={valueStyle}>
        {value}
      </span>
    </span>
  );
}
