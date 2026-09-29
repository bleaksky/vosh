import { useCombat } from '../../lib/stores/combatStore';
import { useVitals, type VitalKey } from '../../lib/stores/vitalsStore';
import { vitalValue } from './paneText';

// Vitals pinned under the panes (SPEC 5, G3). Each row is a label, the
// value, and a 2 px meter that stays tertiary at rest and turns danger
// when the vital runs low. In a fight the opponent gets a row on top
// with its health in warn. Nothing pulses.

const ROWS: { key: VitalKey; label: string; max: 'maxhp' | 'maxmana' | 'maxmove' }[] = [
  { key: 'hp', label: 'Health', max: 'maxhp' },
  { key: 'mana', label: 'Mana', max: 'maxmana' },
  { key: 'move', label: 'Moves', max: 'maxmove' },
];

export function VitalsFooter() {
  const vitals = useVitals();
  const combat = useCombat();

  return (
    <section className="panel-vitals" aria-label="Vitals">
      {combat && (
        <VitalRow
          className="panel-vitals-row-combat"
          label={combat.name}
          value={combat.hp_pct !== null ? `${combat.hp_pct}%` : (combat.condition ?? '')}
          pct={combat.hp_pct}
        />
      )}
      {vitals === null ? (
        <div className="panel-vitals-row">
          <p className="panel-vitals-empty">Vitals appear when you log in.</p>
        </div>
      ) : (
        ROWS.filter((r) => r.key === 'hp' || vitals[r.max] > 0).map((r) => (
          <VitalRow
            key={r.key}
            className={vitals.low[r.key] ? 'panel-vitals-row-low' : undefined}
            label={r.label}
            value={vitalValue(vitals[r.key], vitals[r.max])}
            pct={meterPercent(vitals[r.key], vitals[r.max])}
          />
        ))
      )}
    </section>
  );
}

function VitalRow({
  label,
  value,
  pct,
  className,
}: {
  label: string;
  value: string;
  pct: number | null;
  className?: string | undefined;
}) {
  return (
    <div className={`panel-vitals-row${className ? ` ${className}` : ''}`}>
      <div className="panel-vitals-line">
        <span className="panel-vitals-label">{label}</span>
        <span className="panel-vitals-value">{value}</span>
      </div>
      <div className="panel-vitals-meter" aria-hidden="true">
        {pct !== null && <div className="panel-vitals-fill" style={{ width: `${pct}%` }} />}
      </div>
    </div>
  );
}

/** Meter fill in percent, unrounded so the 2 px line moves smoothly. */
function meterPercent(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(100, (current / max) * 100));
}
