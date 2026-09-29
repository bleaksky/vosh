import { useMemo, type CSSProperties, type ReactNode } from 'react';
import type { ChipStyle } from '../../lib/session';
import { useChipStyle } from '../../lib/stores/chipStyleStore';
import { useTarget } from '../../lib/stores/targetStore';
import { useTick } from '../../lib/stores/tickStore';
import { useVitals, type VitalKey } from '../../lib/stores/vitalsStore';
import { moonLabel, useWorld } from '../../lib/stores/worldStore';
import { themeTokens } from '../../lib/themes';
import { useActiveTheme } from '../../lib/useActiveTheme';
import { daylightTint, isDaytime } from './daylight';
import { formatGameTime } from './gameTime';
import { MoonIcon, StopwatchIcon, SunIcon } from './icons';

// The quiet line under the input band (SPEC 10 G4): your target, the
// tick and the game time together, and the moon, 20 px apart in the UI
// face with tabular numbers. With the panel hidden, the vitals it pins
// lead the line so you never lose them.
//
// The tick and the game time share one item, the way the old input row
// chip kept them, 8 px apart inside it. The tick counts up from the last
// tick and turns the warn tone in the last seconds you set in the tick
// config. The time takes a daylight tint from your theme. The chip style
// in Settings shows each value alone, after a caption, or after an icon.

const VITAL_ROWS: { key: VitalKey; label: string; max: 'maxhp' | 'maxmana' | 'maxmove' }[] = [
  { key: 'hp', label: 'Health', max: 'maxhp' },
  { key: 'mana', label: 'Mana', max: 'maxmana' },
  { key: 'move', label: 'Moves', max: 'maxmove' },
];

interface Props {
  connected: boolean;
  /** Lead with your vitals, for when the panel that pins them is
   *  hidden. */
  showVitals: boolean;
}

export function StatusLine({ connected, showVitals }: Props) {
  const target = useTarget();
  const vitals = useVitals();
  const world = useWorld();
  const moon = moonLabel(world.moons);

  return (
    <div className="shell-statusline" role="group" aria-label="Status">
      {!connected && <span>Not connected</span>}
      {showVitals &&
        vitals &&
        VITAL_ROWS.map(({ key, label, max }) => (
          <span key={key}>
            {label}
            <span className={`shell-status-value${vitals.low[key] ? ' is-low' : ''}`}>
              {vitals[key]} / {vitals[max]}
            </span>
          </span>
        ))}
      {target.name && (
        <span>
          Target<span className="shell-status-value">{target.name}</span>
        </span>
      )}
      <TickAndTime />
      {moon && <span>{moon}</span>}
    </div>
  );
}

/** The tick and the game time as one status line item, tick first.
 *  Renders nothing while neither is known. */
function TickAndTime() {
  const style = useChipStyle();
  const tick = useTick();
  const world = useWorld();
  const theme = useActiveTheme();
  const time = formatGameTime(world.time);
  const hour = world.time?.hour ?? null;
  const tint = useMemo(() => daylightTint(hour, theme.xterm, themeTokens(theme)), [hour, theme]);
  const showTick = tick.active && tick.secsSinceTick !== null;
  if (!showTick && !time) return null;

  return (
    <span className="shell-status-clock">
      {showTick && (
        <Reading
          style={style}
          caption="Tick"
          icon={<StopwatchIcon />}
          warn={tick.warn}
          value={`${tick.secsSinceTick}s`}
        />
      )}
      {time && (
        <Reading
          style={style}
          caption="Time"
          icon={isDaytime(world.time) === false ? <MoonIcon /> : <SunIcon />}
          value={time}
          valueStyle={tint ? { color: tint } : undefined}
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
