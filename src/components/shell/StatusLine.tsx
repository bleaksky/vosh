import { useMemo } from 'react';
import { useChipStyle } from '../../lib/stores/chipStyleStore';
import { useTarget } from '../../lib/stores/targetStore';
import { useTick } from '../../lib/stores/tickStore';
import { useVitals, type VitalKey } from '../../lib/stores/vitalsStore';
import { useWorld } from '../../lib/stores/worldStore';
import { themeTokens } from '../../lib/themes';
import { useActiveTheme } from '../../lib/useActiveTheme';
import { daylightTint, isDaytime } from './daylight';
import { formatGameTime } from './gameTime';
import { StatusClock } from './StatusClock';
import { statusMoons } from './statusMoons';

// The quiet line under the input band (SPEC 10 G4): your target, then
// the tick, the game time, and the moons together, 20 px apart in the
// UI face with tabular numbers. With the panel hidden, the vitals it
// pins lead the line so you never lose them.
//
// The tick, the game time, and the moons share one item, the way the
// old input row chip kept the tick and the time, 8 px apart inside it.
// The tick counts up from the last tick and turns the warn tone on a
// soft warn ground in the last seconds you set in the tick config. The
// time takes a daylight tint from your theme. The moons in the sky show
// as phase icons in their own colors, with a word for an eclipse, the
// triad, or a near alignment. The chip style in Settings shows each
// value alone, after a caption, or after an icon.

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
      <ClockItem connected={connected} />
    </div>
  );
}

/** Reads the tick, the game time, the moons, and the theme for
 *  StatusClock. The moons show only while connected. */
function ClockItem({ connected }: { connected: boolean }) {
  const style = useChipStyle();
  const tick = useTick();
  const world = useWorld();
  const theme = useActiveTheme();
  const text = formatGameTime(world.time);
  const hour = world.time?.hour ?? null;
  const tokens = useMemo(() => themeTokens(theme), [theme]);
  const tint = useMemo(() => daylightTint(hour, theme.xterm, tokens), [hour, theme, tokens]);
  const moons = useMemo(
    () => (connected ? statusMoons(world.moons, theme.xterm, tokens) : null),
    [connected, world.moons, theme, tokens],
  );
  return (
    <StatusClock
      style={style}
      tick={
        tick.active && tick.secsSinceTick !== null
          ? { secs: tick.secsSinceTick, warn: tick.warn }
          : null
      }
      time={text ? { text, tint, daytime: isDaytime(world.time) } : null}
      moons={moons}
    />
  );
}
