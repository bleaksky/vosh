import type { VitalsValues } from '../ipc/uiConfig';
import { HitGhost } from './HitGhost';
import { Columns, DrawnOpponent, DrawnVitals, type DrawnVitalsProps } from './VitalsDrawn';
import { VIAL_HEIGHT, VIAL_WIDTH, type ColumnFit } from './vitalsDrawnFit';
import { Glass } from './VitalsGlass';
import { hitFill, type HitView } from './vitalsHit';
import { ledgerFigure } from './vitalsLedgerFit';
import { VITAL_LABELS } from './vitalsView';

// Vials (More Vitals Styles, board 2): a small vial for each vital, 18
// by 44, after the Path of Exile flasks and the Diablo potion belt. The
// glass is a 1 px rim at 55 percent of the vital's tone over a 6 percent
// wash, and the liquid fills from the round foot to the shoulder in the
// tone, with a surface line a step toward the text color. The pane
// label caps, the figure and the max sit beside each vial, in columns
// as Ledger lays them. Low turns the liquid, the glass and the figure
// danger. Your opponent gets a glass tube 8 tall across the footer. On
// a narrow panel the figure moves under the vial and the max goes, and
// the columns stay (vitalsDrawnFit.ts).

/** The vial's outline, round at the foot, with a neck and shoulders. */
const VIAL =
  'M5.5 0.5 H12.5 V4.5 C12.5 7 17.5 7.5 17.5 11 V35 A8.5 8.5 0 0 1 0.5 35 V11 C0.5 7.5 5.5 7 5.5 4.5 Z';

/** Where the liquid stands at the shoulder and at the foot. */
const SHOULDER = 9;
const FOOT = 43.5;

export function VitalsVials({
  rows,
  waiting,
  combat,
  place,
  inks,
  hits,
  values,
  fit,
}: DrawnVitalsProps & { values: VitalsValues; fit: ColumnFit }) {
  const narrow = fit === 'narrow';
  const foe = combat && (
    <DrawnOpponent
      combat={combat}
      rows={rows}
      waiting={waiting}
      place={place}
      under={(health) => <Tube pct={health.pct} hit={hits.foe} />}
    />
  );
  return (
    <DrawnVitals kind="vials" waiting={waiting} place={place} foe={foe}>
      <Columns
        rows={rows}
        inks={inks}
        cell={(row) => {
          const figure = ledgerFigure(values, row.current, row.max, row.tone === 'hidden');
          const caps = <span className="vitals-caps">{VITAL_LABELS[row.key]}</span>;
          return (
            <div className={`vitals-vial${narrow ? ' is-narrow' : ''}`}>
              {narrow && caps}
              <Vial pct={row.pct} hit={hits[row.key]} />
              <span className="vitals-vial-text">
                {!narrow && caps}
                <span className="vitals-vial-figure">{figure.current}</span>
                {!narrow && figure.max !== null && (
                  <span className="vitals-vial-max">{figure.max}</span>
                )}
              </span>
            </div>
          );
        }}
      />
    </DrawnVitals>
  );
}

/** Where the liquid stands at `pct` percent, in px from the top. */
const levelAt = (pct: number) => FOOT - (pct / 100) * (FOOT - SHOULDER);

function Vial({ pct, hit }: { pct: number | null; hit: HitView | undefined }) {
  const { fill, ghost, draining } = hitFill(pct, hit);
  return (
    <Glass
      className="vitals-vial-glass"
      width={VIAL_WIDTH}
      height={VIAL_HEIGHT}
      shape={(props) => <path {...props} d={VIAL} />}
      level={fill === null ? null : levelAt(fill)}
      gone={ghost === null ? null : levelAt(ghost)}
      draining={draining}
      surface
    />
  );
}

/** Your opponent's glass tube, 8 tall across the footer. */
function Tube({ pct, hit }: { pct: number | null; hit: HitView | undefined }) {
  const { fill, ghost, draining } = hitFill(pct, hit);
  return (
    <span className="vitals-tube">
      {fill !== null && ghost !== null && (
        <HitGhost className="vitals-tube-gone" fill={fill} ghost={ghost} draining={draining} />
      )}
      {fill !== null && <span className="vitals-tube-liquid" style={{ width: `${fill}%` }} />}
    </span>
  );
}
