import type { VitalsValues } from '../ipc/uiConfig';
import {
  Columns,
  DrawnOpponent,
  DrawnVitals,
  LineMark,
  type DrawnVitalsProps,
} from './VitalsDrawn';
import { DIAL, DIAL_NARROW, type ColumnFit } from './vitalsDrawnFit';
import { hitFill, type HitView } from './vitalsHit';
import { ledgerFigure } from './vitalsLedgerFit';
import { VITAL_LABELS } from './vitalsView';

// Dials: one open 270 degree arc for
// each vital, 4 px wide on the divider track with round ends, running
// clockwise from the lower left, after the Breath of the Wild stamina
// wheel. The pane label caps sit above, the figure inside and the max
// in the opening at the foot. With Show each hit on, the part a hit
// took stays pale along the arc. Your opponent draws its name and
// health over a 2 px line in warn, since a dial cannot stretch across
// the footer. On a narrow panel each dial draws at 44 without the max
// and the columns stay (vitalsDrawnFit.ts).

export function VitalsDials({
  rows,
  waiting,
  combat,
  place,
  inks,
  hits,
  values,
  fit,
}: DrawnVitalsProps & { values: VitalsValues; fit: ColumnFit }) {
  const size = fit === 'narrow' ? DIAL_NARROW : DIAL;
  const foe = combat && (
    <DrawnOpponent
      combat={combat}
      rows={rows}
      waiting={waiting}
      place={place}
      under={(health) => <LineMark pct={health.pct} hit={hits.foe} />}
    />
  );
  return (
    <DrawnVitals kind="dials" waiting={waiting} place={place} foe={foe}>
      <Columns
        rows={rows}
        inks={inks}
        cell={(row) => {
          const hidden = row.tone === 'hidden';
          const figure = ledgerFigure(values, row.current, row.max, hidden);
          return (
            <>
              <span className="vitals-caps">{VITAL_LABELS[row.key]}</span>
              <span className={`vitals-dial${fit === 'narrow' ? ' is-narrow' : ''}`}>
                <Dial size={size} pct={row.pct} hit={hits[row.key]} />
                <span className="vitals-dial-figure">{figure.current}</span>
                {fit !== 'narrow' && figure.max !== null && (
                  <span className="vitals-dial-max">{hidden ? '?' : row.max}</span>
                )}
              </span>
            </>
          );
        }}
      />
    </DrawnVitals>
  );
}

/** An arc of 270 degrees `size` px across, open at the foot. */
function arcPath(size: number): string {
  const c = size / 2;
  const r = c - 4;
  const at = (deg: number) =>
    `${round(c + r * Math.cos((deg * Math.PI) / 180))} ${round(c + r * Math.sin((deg * Math.PI) / 180))}`;
  return `M${at(135)} A${r} ${r} 0 1 1 ${at(45)}`;
}

function Dial({ size, pct, hit }: { size: number; pct: number | null; hit: HitView | undefined }) {
  const { fill, ghost, draining } = hitFill(pct, hit);
  const d = arcPath(size);
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
      <path className="vitals-dial-track" d={d} pathLength={100} />
      {fill !== null && ghost !== null && (
        <path
          className={`vitals-dial-gone vitals-ghost${draining ? ' is-draining' : ''}`}
          d={d}
          pathLength={100}
          style={{ strokeDasharray: `0 ${round(fill)} ${draining ? 0 : round(ghost - fill)} 200` }}
        />
      )}
      {fill !== null && fill > 0 && (
        <path
          className="vitals-dial-arc"
          d={d}
          pathLength={100}
          style={{ strokeDasharray: `${round(fill)} 200` }}
        />
      )}
    </svg>
  );
}

const round = (n: number) => Math.round(n * 100) / 100;
