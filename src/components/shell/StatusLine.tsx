import { useMemo } from 'react';
import { usePlayPalette } from '../../lib/fitGameColors';
import type { VitalsOptions } from '../../ipc/uiConfig';
import { useChipStyle } from '../../lib/stores/chipStyleStore';
import { useCombat } from '../../lib/stores/combatStore';
import { useGameTime } from '../../lib/stores/gameTimeStore';
import { useTarget } from '../../lib/stores/targetStore';
import { useTickCount } from '../../lib/stores/tickCountStore';
import { shownTick, useTick } from '../../lib/stores/tickStore';
import { useVitalsOptions } from '../../lib/stores/vitalsOptionsStore';
import { useVitals, type Vitals, type VitalKey } from '../../lib/stores/vitalsStore';
import { useWorld } from '../../lib/stores/worldStore';
import { themeTokens } from '../../lib/themes';
import { useActiveTheme } from '../../lib/useActiveTheme';
import {
  formatVital,
  hiddenVital,
  targetHealthPercent,
  vitalTone,
  type CombatHealth,
  type VitalTone,
} from '../../lib/vitalsView';
import { daylightTint, isDaytime } from './daylight';
import { formatGameTime } from './gameTime';
import { StatusClock } from './StatusClock';
import { statusMoons } from './statusMoons';

// The quiet line under the input band (SPEC 10 G4): your target, then
// the tick, the game time, and the moons together, 20 px apart in the
// panel face, the Panel font, with tabular numbers. With the panel
// hidden, the vitals it pins lead the line so you never lose them. They
// follow Values and Warn before you run low from Settings, Layout,
// Vitals, and never draw a meter (VitalsOptions.dc.html). While you
// fight the target you set, its health follows its name in the warn
// tone.
//
// While the game hides your vitals (lamented tears) each one reads `?`
// in its Values form in tertiary and never warns. The target health
// leaves the line while Char.Combat withholds it.
//
// The tick, the game time, and the moons share one item, the way the
// old input row chip kept the tick and the time, 8 px apart inside it.
// The tick counts up from the last tick and turns the warn tone on a
// soft warn ground in the last seconds you set in the tick config. The
// time reads on the 24 or 12 hour clock you pick in Settings and takes
// a daylight tint from your theme. The moons in the sky show
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
  const combat = useCombat();
  const options = useVitalsOptions();

  return (
    <div className="shell-statusline" role="group" aria-label="Status">
      {!connected && <span>Not connected</span>}
      <StatusVitals
        showVitals={showVitals}
        vitals={vitals}
        target={target.name}
        combat={combat}
        options={options}
      />
      <ClockItem connected={connected} />
    </div>
  );
}

export interface StatusVitalsProps {
  /** The panel is hidden, so the line carries your vitals. */
  showVitals: boolean;
  vitals: Vitals | null;
  /** The target you set, or null. */
  target: string | null;
  /** The Char.Combat opponent, or null out of a fight. */
  combat: CombatHealth | null;
  options: VitalsOptions;
}

/** Your vitals and your target, drawn from plain values so a test can
 *  render every case. The target's health shows only with the panel
 *  hidden, since the panel's combat row carries it otherwise. */
export function StatusVitals({ showVitals, vitals, target, combat, options }: StatusVitalsProps) {
  const targetPct = showVitals ? targetHealthPercent(target, combat) : null;
  return (
    <>
      {showVitals &&
        vitals &&
        VITAL_ROWS.map(({ key, label, max }) => (
          <span key={key}>
            {label}
            {vitals.hidden ? (
              <span className={toneClass('hidden')}>{hiddenVital(options.values)}</span>
            ) : (
              <span
                className={toneClass(
                  vitalTone(vitals[key], vitals[max], vitals.low[key], options.warn_thirds),
                )}
              >
                {formatVital(options.values, vitals[key], vitals[max])}
              </span>
            )}
          </span>
        ))}
      {target && (
        <span className="shell-status-target">
          Target<span className="shell-status-value">{target}</span>
          {targetPct !== null && (
            <span className="shell-status-value is-warn">{`${targetPct}%`}</span>
          )}
        </span>
      )}
    </>
  );
}

function toneClass(tone: VitalTone): string {
  if (tone === 'danger') return 'shell-status-value is-low';
  if (tone === 'warn') return 'shell-status-value is-warn';
  if (tone === 'hidden') return 'shell-status-value is-hidden';
  return 'shell-status-value';
}

/** Reads the tick, the way it counts, the game time on its clock, the
 *  moons, and the theme for StatusClock. The daylight tint and the moons
 *  take the play palette, fitted while Fit game colors is on. The moons
 *  show only while connected. */
function ClockItem({ connected }: { connected: boolean }) {
  const style = useChipStyle();
  const tick = useTick();
  const shown = shownTick(tick, useTickCount());
  const world = useWorld();
  const theme = useActiveTheme();
  const palette = usePlayPalette();
  const text = formatGameTime(world.time, useGameTime());
  const hour = world.time?.hour ?? null;
  const tokens = useMemo(() => themeTokens(theme), [theme]);
  const tint = useMemo(() => daylightTint(hour, palette, tokens), [hour, palette, tokens]);
  const moons = useMemo(
    () => (connected ? statusMoons(world.moons, palette, tokens) : null),
    [connected, world.moons, palette, tokens],
  );
  return (
    <StatusClock
      style={style}
      tick={
        shown && {
          secs: shown.secs,
          count: shown.count,
          warn: tick.warn,
          overdue: tick.overdue,
          interval: tick.intervalSecs,
        }
      }
      time={text ? { text, tint, daytime: isDaytime(world.time), hour } : null}
      moons={moons}
    />
  );
}
