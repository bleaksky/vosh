import { useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { useCombat } from '../../lib/stores/combatStore';
import { useVitals, type Vitals, type VitalKey } from '../../lib/stores/vitalsStore';
import { useVitalsDensity } from '../../lib/stores/vitalsDensityStore';
import { vitalValue } from './paneText';
import { panelWidthOf, usePanelLayout } from './panelLayoutStore';
import { vitalsLineShowsLabels } from './vitalsLine';

// Vitals pinned under the panes (SPEC 5, G3). Each vital is a label,
// the value, and a 2 px meter that stays tertiary at rest and turns
// danger when the vital runs low. In a fight the opponent gets a row on
// top with its health in warn. Nothing pulses.
//
// The density comes from Settings, Layout. Rows gives each vital its
// own row. One line sets Health, Mana, and Moves side by side, the
// Focus board's status line form with a meter under each value, and
// drops the labels on a panel narrower than about 360 pt.

const ROWS: { key: VitalKey; label: string; max: 'maxhp' | 'maxmana' | 'maxmove' }[] = [
  { key: 'hp', label: 'Health', max: 'maxhp' },
  { key: 'mana', label: 'Mana', max: 'maxmana' },
  { key: 'move', label: 'Moves', max: 'maxmove' },
];

/** The vitals the MUD sends. Health always shows. */
function shownRows(vitals: Vitals) {
  return ROWS.filter((r) => r.key === 'hp' || vitals[r.max] > 0);
}

export function VitalsFooter() {
  const vitals = useVitals();
  const combat = useCombat();
  const density = useVitalsDensity();
  const line = density === 'line';
  const sectionRef = useRef<HTMLElement | null>(null);
  const width = useFooterWidth(sectionRef, line);

  return (
    <section
      ref={sectionRef}
      className={`panel-vitals${line ? ' is-one-line' : ''}`}
      aria-label="Vitals"
    >
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
      ) : line ? (
        <div className="panel-vitals-row panel-vitals-oneline">
          {shownRows(vitals).map((r) => (
            <VitalItem
              key={r.key}
              low={vitals.low[r.key]}
              label={r.label}
              showLabel={vitalsLineShowsLabels(width)}
              value={vitalValue(vitals[r.key], vitals[r.max])}
              pct={meterPercent(vitals[r.key], vitals[r.max])}
            />
          ))}
        </div>
      ) : (
        shownRows(vitals).map((r) => (
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

/** The footer's drawn width while One line shows, which sits under
 *  the saved panel width when the window is too narrow for it. The
 *  saved width stands in until the first measure. */
function useFooterWidth(el: RefObject<HTMLElement | null>, active: boolean): number {
  const saved = panelWidthOf(usePanelLayout());
  const [measured, setMeasured] = useState<number | null>(null);
  useLayoutEffect(() => {
    const node = el.current;
    if (!active || !node) return;
    const measure = () => setMeasured(node.getBoundingClientRect().width);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [el, active]);
  return measured ?? saved;
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
      <Meter pct={pct} />
    </div>
  );
}

/** One vital on the One line row. Without its visible label, the label
 *  still names the value for a screen reader. */
function VitalItem({
  label,
  showLabel,
  value,
  pct,
  low,
}: {
  label: string;
  showLabel: boolean;
  value: string;
  pct: number;
  low: boolean;
}) {
  return (
    <div className={`panel-vitals-item${low ? ' panel-vitals-row-low' : ''}`}>
      <div className="panel-vitals-line">
        <span className={showLabel ? 'panel-vitals-label' : 'panel-vitals-label-hidden'}>
          {label}
        </span>
        <span className="panel-vitals-value">{value}</span>
      </div>
      <Meter pct={pct} />
    </div>
  );
}

function Meter({ pct }: { pct: number | null }) {
  return (
    <div className="panel-vitals-meter" aria-hidden="true">
      {pct !== null && <div className="panel-vitals-fill" style={{ width: `${pct}%` }} />}
    </div>
  );
}

/** Meter fill in percent, unrounded so the 2 px line moves smoothly. */
function meterPercent(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(100, (current / max) * 100));
}
