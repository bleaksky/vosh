import { useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { useCombat } from '../../lib/stores/combatStore';
import { useVitals, type Vitals, type VitalKey } from '../../lib/stores/vitalsStore';
import { useVitalsDensity } from '../../lib/stores/vitalsDensityStore';
import { vitalValue } from './paneText';
import { panelWidthOf, usePanelLayout } from './panelLayoutStore';
import { vitalsLineFit } from './vitalsLine';

// Vitals pinned under the panes (SPEC 5, G3). Each vital is a label,
// the value, and a 2 px meter that stays tertiary at rest and turns
// danger when the vital runs low. In a fight the opponent gets a row on
// top with its health in warn. Nothing pulses.
//
// The density comes from Settings, Layout. Rows gives each vital its
// own row. One line sets Health, Mana, and Moves side by side, the
// Focus board's status line form with a meter under each value, and
// drops the labels only when they no longer fit beside the values,
// under about 360 pt for four digit health. A panel too narrow for even
// the values stacks them in rows (vitalsLine.ts).

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
  const sectionRef = useRef<HTMLElement | null>(null);
  const box = useFooterBox(sectionRef, density === 'line');
  const rows = vitals === null ? [] : shownRows(vitals);
  const values = vitals === null ? [] : rows.map((r) => vitalValue(vitals[r.key], vitals[r.max]));
  // Fit by each vital at its max, the widest its value reads, so the
  // line does not jump between forms as a value loses a digit in a
  // fight. Only a new max or a new panel width moves it.
  const fit =
    density === 'line' && vitals !== null
      ? vitalsLineFit(
          box.width,
          rows.map((r) => ({
            label: textWidth(r.label, `400 12px ${box.family}`),
            value: textWidth(vitalValue(vitals[r.max], vitals[r.max]), `500 12px ${box.family}`),
          })),
        )
      : 'rows';
  const line = fit !== 'rows';

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
          {rows.map((r, i) => (
            <VitalItem
              key={r.key}
              low={vitals.low[r.key]}
              label={r.label}
              showLabel={fit === 'labels'}
              value={values[i]}
              pct={meterPercent(vitals[r.key], vitals[r.max])}
            />
          ))}
        </div>
      ) : (
        rows.map((r, i) => (
          <VitalRow
            key={r.key}
            className={vitals.low[r.key] ? 'panel-vitals-row-low' : undefined}
            label={r.label}
            value={values[i]}
            pct={meterPercent(vitals[r.key], vitals[r.max])}
          />
        ))
      )}
    </section>
  );
}

/** The footer's drawn width and UI font while One line is chosen. The
 *  width sits under the saved panel width when the window is too
 *  narrow for it, and the saved width stands in until the first
 *  measure. */
function useFooterBox(
  el: RefObject<HTMLElement | null>,
  active: boolean,
): { width: number; family: string } {
  const saved = panelWidthOf(usePanelLayout());
  const [box, setBox] = useState<{ width: number; family: string } | null>(null);
  useLayoutEffect(() => {
    const node = el.current;
    if (!active || !node) return;
    const measure = () => {
      const width = node.getBoundingClientRect().width;
      const family = getComputedStyle(node).fontFamily;
      setBox((prev) =>
        prev && prev.width === width && prev.family === family ? prev : { width, family },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [el, active]);
  return box ?? { width: saved, family: 'system-ui' };
}

let measureCanvas: HTMLCanvasElement | null = null;
// Widths by font and text. The labels and maxes rarely change, so a
// vitals update reads these instead of measuring again.
const widths = new Map<string, number>();

/** How wide `text` draws in `font`. Values use tabular numbers, where
 *  every digit is as wide as a zero, so digits measure as zeros. */
function textWidth(text: string, font: string): number {
  const shape = text.replace(/[0-9]/g, '0');
  const key = `${font}|${shape}`;
  const known = widths.get(key);
  if (known !== undefined) return known;
  measureCanvas ??= document.createElement('canvas');
  const ctx = measureCanvas.getContext('2d');
  if (!ctx) return shape.length * 7;
  ctx.font = font;
  const width = Math.ceil(ctx.measureText(shape).width);
  if (widths.size > 64) widths.clear();
  widths.set(key, width);
  return width;
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
