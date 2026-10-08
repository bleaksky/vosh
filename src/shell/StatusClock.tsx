import type { CSSProperties, ReactNode } from 'react';
import type { ChipStyle, TickCount } from '../ipc/uiConfig';
import { SunPathIcon, TickRingIcon } from './icons';
import { MoonPhaseIcon } from './MoonPhaseIcon';
import { VisuallyHidden } from '../ui';

// The tick, the game time, and the moons as one status line item,
// drawn from plain values so a test can render it without the stores.
// StatusLine reads the stores and the theme and hands them in.

export interface ClockTick {
  /** The whole seconds the tick shows. Counting up, the seconds since
   *  the last tick. Counting down, the seconds left until the next,
   *  below zero past 0 while the tick is late. */
  secs: number;
  /** Inside the warn window before the next tick, or late. */
  warn: boolean;
  /** The tick interval in seconds the ring fills against, null while
   *  unknown. */
  interval: number | null;
  /** Which way the count runs. Up when left out. */
  count?: TickCount;
  /** The expected tick has come and the game's tick has not, so the
   *  reading pulses in the warn tone. */
  overdue?: boolean;
}

/** The tick as the line shows it, like `14s`, with the true minus sign
 *  below zero so the figures keep their width, and what a screen reader
 *  says for it when that differs, like `minus 5s`. */
function tickText(secs: number): { text: string; spoken: string | undefined } {
  if (secs >= 0) return { text: `${secs}s`, spoken: undefined };
  return { text: `−${-secs}s`, spoken: `minus ${-secs}s` };
}

export interface ClockTime {
  /** The game time as the status line shows it, like 8:42. */
  text: string;
  /** The daylight tint, or null for the plain value color. */
  tint: string | null;
  /** Whether the sun is up, null while unknown. */
  daytime: boolean | null;
  /** The 0..23 game hour that places the sun on its path, null while
   *  unknown. */
  hour: number | null;
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
  const shown = tick ? tickText(tick.secs) : null;
  return (
    <span className="shell-status-clock">
      {tick && shown && (
        <Reading
          style={style}
          caption="Tick"
          icon={
            <TickRingIcon secs={tick.secs} interval={tick.interval} count={tick.count ?? 'up'} />
          }
          warn={tick.warn}
          overdue={tick.overdue === true}
          value={shown.text}
          spoken={shown.spoken}
        />
      )}
      {time && (
        <Reading
          style={style}
          caption="Time"
          icon={<SunPathIcon hour={time.hour} daytime={time.daytime} />}
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
      <span className={style === 'caption_value' ? undefined : 'visually-hidden'}>Moons</span>
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
  /** What a screen reader says for the value when the line's glyphs
   *  would not read right, like `minus 5s` for the true minus sign. */
  spoken?: string | undefined;
  warn?: boolean;
  /** Late, so the warn tone pulses. */
  overdue?: boolean;
  valueStyle?: CSSProperties | undefined;
}

/** One value with its caption or icon, 6 px before it. A screen reader
 *  hears the caption in every style. */
function Reading({
  style,
  caption,
  icon,
  value,
  spoken,
  warn = false,
  overdue = false,
  valueStyle,
}: ReadingProps) {
  const tone = `${warn ? ' is-warn' : ''}${warn && overdue ? ' is-overdue' : ''}`;
  return (
    <span className={`shell-status-part${tone}`}>
      {style === 'icon_value' && icon}
      <span className={style === 'caption_value' ? undefined : 'visually-hidden'}>{caption}</span>
      <span className="shell-status-value" style={valueStyle}>
        {spoken === undefined ? (
          value
        ) : (
          <>
            <span aria-hidden="true">{value}</span>
            <VisuallyHidden>{spoken}</VisuallyHidden>
          </>
        )}
      </span>
    </span>
  );
}
