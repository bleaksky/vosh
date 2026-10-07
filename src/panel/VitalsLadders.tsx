import { DrawnOpponent, DrawnVitals, MarkRows, type DrawnVitalsProps } from './VitalsDrawn';
import { FOE_LADDER, LADDER, litSegments, type RowMarkFit } from './vitalsDrawnFit';

// Ladders (More Vitals Styles, board 1): 24 segments 8 px tall with
// 1 px gaps between each label and value, lit in the vital's tone and
// unlit at a fifth of it, as a level meter lights. Your opponent's
// ladder runs the footer in 48. On a narrow panel each ladder drops
// under its label and value (vitalsDrawnFit.ts).

export function VitalsLadders({
  rows,
  waiting,
  combat,
  place,
  inks,
  fit,
}: DrawnVitalsProps & { fit: RowMarkFit }) {
  const foe = combat && (
    <DrawnOpponent
      combat={combat}
      rows={rows}
      waiting={waiting}
      place={place}
      under={(health) => <Ladder pct={health.pct} count={FOE_LADDER} />}
    />
  );
  return (
    <DrawnVitals kind="ladders" waiting={waiting} place={place} foe={foe}>
      <MarkRows
        kind="ladders"
        under={fit === 'under'}
        rows={rows}
        inks={inks}
        mark={(row) => <Ladder pct={row.pct} count={LADDER} />}
      />
    </DrawnVitals>
  );
}

function Ladder({ pct, count }: { pct: number | null; count: number }) {
  const lit = litSegments(pct, count);
  return (
    <span className="vitals-ladder">
      {Array.from({ length: count }, (_, i) => (
        <i key={i} className={i < lit ? 'vitals-ladder-seg is-lit' : 'vitals-ladder-seg'} />
      ))}
    </span>
  );
}
