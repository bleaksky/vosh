import { useTarget } from '../../lib/stores/targetStore';
import { useTick } from '../../lib/stores/tickStore';
import { useVitals, type VitalKey } from '../../lib/stores/vitalsStore';
import { moonLabel, useWorld } from '../../lib/stores/worldStore';
import { formatGameTime } from './gameTime';

// The quiet line under the input band (SPEC 10 G4): your target, the
// seconds to the next tick, the game time, and the moon, 20 px apart
// in the UI face with tabular numbers. The tick turns the warn tone at
// the threshold you set in the tick config. With the panel hidden, the
// vitals it pins lead the line so you never lose them.

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
  const tick = useTick();
  const world = useWorld();
  const vitals = useVitals();
  const time = formatGameTime(world.time);
  const moon = moonLabel(world.moons);

  return (
    <div className="shell-statusline" aria-label="Status">
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
      {tick.active && tick.secsToTick !== null && (
        <span className={tick.warn ? 'is-warn' : undefined}>Tick {tick.secsToTick}</span>
      )}
      {time && <span>{time}</span>}
      {moon && <span>{moon}</span>}
    </div>
  );
}
