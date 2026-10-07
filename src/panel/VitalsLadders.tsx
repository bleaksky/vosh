import { DrawnOpponent, DrawnVitals, MarkRows, type DrawnVitalsProps } from './VitalsDrawn';
import { FOE_LADDER, LADDER, ladderPeak, litSegments, type RowMarkFit } from './vitalsDrawnFit';

// Ladders (More Vitals Styles, board 1): 24 segments 8 px tall with
// 1 px gaps between each label and value, lit in the vital's tone and
// unlit at a fifth of it, as a level meter lights. Your opponent's
// ladder runs the footer in 48. With Show each hit on, the segment a
// vital stood at before a hit stays lit for 1.5 s, then drops. On a
// narrow panel each ladder drops under its label and value
// (vitalsDrawnFit.ts).

export function VitalsLadders({
  rows,
  waiting,
  combat,
  place,
  inks,
  hits,
  fit,
}: DrawnVitalsProps & { fit: RowMarkFit }) {
  const foe = combat && (
    <DrawnOpponent
      combat={combat}
      rows={rows}
      waiting={waiting}
      place={place}
      under={(health) => (
        <Ladder pct={health.pct} peak={hits.foe?.peak ?? null} count={FOE_LADDER} />
      )}
    />
  );
  return (
    <DrawnVitals kind="ladders" waiting={waiting} place={place} foe={foe}>
      <MarkRows
        kind="ladders"
        under={fit === 'under'}
        rows={rows}
        inks={inks}
        mark={(row) => <Ladder pct={row.pct} peak={hits[row.key]?.peak ?? null} count={LADDER} />}
      />
    </DrawnVitals>
  );
}

function Ladder({ pct, peak, count }: { pct: number | null; peak: number | null; count: number }) {
  const lit = litSegments(pct, count);
  const held = pct === null ? -1 : ladderPeak(peak, count);
  return (
    <span className="vitals-ladder">
      {Array.from({ length: count }, (_, i) => (
        <i
          key={i}
          className={i < lit || i === held ? 'vitals-ladder-seg is-lit' : 'vitals-ladder-seg'}
        />
      ))}
    </span>
  );
}
