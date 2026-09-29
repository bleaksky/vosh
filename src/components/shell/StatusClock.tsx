import type { CSSProperties, ReactNode } from 'react';
import type { ChipStyle } from '../../lib/session';
import { MoonIcon, StopwatchIcon, SunIcon } from './icons';

// The tick and the game time as one status line item, drawn from plain
// values so a test can render it without the stores. StatusLine reads
// the stores and the theme and hands them in.

export interface ClockTick {
  /** Whole seconds since the last tick. */
  secs: number;
  /** Inside the warn window before the next tick. */
  warn: boolean;
}

export interface ClockTime {
  /** The game time as the status line shows it, like 8:42. */
  text: string;
  /** The daylight tint, or null for the plain value color. */
  tint: string | null;
  /** Whether the sun is up, null while unknown. */
  daytime: boolean | null;
}

interface Props {
  style: ChipStyle;
  tick: ClockTick | null;
  time: ClockTime | null;
}

/** The tick and the game time as one item, tick first, 8 px apart.
 *  Renders nothing while neither is known. */
export function StatusClock({ style, tick, time }: Props) {
  if (!tick && !time) return null;
  return (
    <span className="shell-status-clock">
      {tick && (
        <Reading
          style={style}
          caption="Tick"
          icon={<StopwatchIcon />}
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
