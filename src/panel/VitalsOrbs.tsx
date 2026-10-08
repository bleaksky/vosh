import type { VitalsValues } from '../ipc/uiConfig';
import { Columns, DrawnOpponent, DrawnVitals, type DrawnVitalsProps } from './VitalsDrawn';
import { ORB, ORB_FOE, ORB_NARROW, type ColumnFit } from './vitalsDrawnFit';
import { Glass } from './VitalsGlass';
import { hitFill, type HitView } from './vitalsHit';
import { ledgerFigure } from './vitalsLedgerFit';
import { VITAL_LABELS } from './vitalsView';

// Orbs (More Vitals Styles, board 2): a 44 pt circle for each vital,
// after the Diablo life and mana globes and the Hollow Knight soul
// vessel. A flat fill rises from the foot and stops at a 1 pt surface
// line, inside a hairline rim, with no gloss, highlight or glow. The
// pane label caps sit over it and the value under it, with the max in
// tertiary as Ledger draws it. Your opponent gets a 14 pt orb before
// its name, since a circle cannot stretch across the footer. On a
// narrow panel the max goes first, then the orbs draw at 40, and the
// columns stay (vitalsDrawnFit.ts).

export function VitalsOrbs({
  rows,
  waiting,
  combat,
  place,
  inks,
  hits,
  values,
  fit,
}: DrawnVitalsProps & { values: VitalsValues; fit: ColumnFit }) {
  const size = fit === 'narrow' ? ORB_NARROW : ORB;
  const foe = combat && (
    <DrawnOpponent
      combat={combat}
      rows={rows}
      waiting={waiting}
      place={place}
      before={(health) => <Orb size={ORB_FOE} pct={health.pct} hit={hits.foe} />}
    />
  );
  return (
    <DrawnVitals kind="orbs" waiting={waiting} place={place} foe={foe}>
      <Columns
        rows={rows}
        inks={inks}
        cell={(row) => {
          const figure = ledgerFigure(values, row.current, row.max, row.tone === 'hidden');
          return (
            <>
              <span className="vitals-caps">{VITAL_LABELS[row.key]}</span>
              <Orb size={size} pct={row.pct} hit={hits[row.key]} />
              <span className="vitals-orb-value">
                {figure.current}
                {fit === 'full' && figure.max !== null && (
                  <span className="vitals-orb-max"> {figure.max}</span>
                )}
              </span>
            </>
          );
        }}
      />
    </DrawnVitals>
  );
}

function Orb({ size, pct, hit }: { size: number; pct: number | null; hit: HitView | undefined }) {
  const { fill, ghost, draining } = hitFill(pct, hit);
  const c = size / 2;
  const inner = c - 1;
  const levelAt = (p: number) => c + inner - (p / 100) * 2 * inner;
  return (
    <Glass
      className="vitals-orb"
      width={size}
      height={size}
      shape={(props) =>
        props.className ? (
          <circle {...props} cx={c} cy={c} r={c - 0.5} />
        ) : (
          <circle cx={c} cy={c} r={inner} />
        )
      }
      level={fill === null ? null : levelAt(fill)}
      gone={ghost === null ? null : levelAt(ghost)}
      draining={draining}
      surface={fill !== null && fill > 1 && fill < 99}
    />
  );
}
