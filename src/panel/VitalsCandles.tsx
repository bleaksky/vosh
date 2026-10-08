import { HitGhost } from './HitGhost';
import { DrawnOpponent, DrawnVitals, MarkRows, type DrawnVitalsProps } from './VitalsDrawn';
import type { RowMarkFit } from './vitalsDrawnFit';
import { hitFill, type HitView } from './vitalsHit';

// Candles (More Vitals Styles, board 2): a taper on its side between
// each label and value. The wax is the fill, 6 tall on a hairline shelf
// that runs to your max, so the burned part still shows how long the
// candle was. A 1 px wick and a 6 by 9 flame sit at the burning end, in
// the theme's yellow (vitalsFlame), and never flicker. Low dims the
// flame to a third and turns the wax danger. With Show each hit on, the
// wax a hit melted stays pale with a short drip under it, then drains.
// Your opponent's candle runs the footer. On a narrow panel each candle
// drops under its label and value (vitalsDrawnFit.ts).

/** The flame, a drop 6 wide and 9 tall. */
const FLAME = 'M3 0 C3.4 2.6 6 3.6 6 6 A3 3 0 0 1 0 6 C0 3.6 2.6 2.6 3 0 Z';

export function VitalsCandles({
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
      under={(health) => <Candle pct={health.pct} hit={hits.foe} />}
    />
  );
  return (
    <DrawnVitals kind="candles" waiting={waiting} place={place} foe={foe}>
      <MarkRows
        kind="candles"
        under={fit === 'under'}
        rows={rows}
        inks={inks}
        mark={(row) => <Candle pct={row.pct} hit={hits[row.key]} />}
      />
    </DrawnVitals>
  );
}

function Candle({ pct, hit }: { pct: number | null; hit: HitView | undefined }) {
  const { fill, ghost, draining } = hitFill(pct, hit);
  return (
    <span className="vitals-candle">
      <i className="vitals-candle-shelf" />
      {fill !== null && ghost !== null && (
        <>
          <HitGhost className="vitals-candle-melt" fill={fill} ghost={ghost} draining={draining} />
          {!draining && (
            <i
              className="vitals-candle-drip"
              style={{ left: `calc(${round(fill + (ghost - fill) * 0.35)}% - 1px)` }}
            />
          )}
        </>
      )}
      {fill !== null && (
        <>
          <i className="vitals-candle-wax" style={{ width: `${fill}%` }} />
          <i className="vitals-candle-wick" style={{ left: `${fill}%` }} />
          <svg
            className="vitals-candle-flame"
            style={{ left: `${fill}%` }}
            viewBox="0 0 6 9"
            aria-hidden="true"
          >
            <path d={FLAME} />
          </svg>
        </>
      )}
    </span>
  );
}

const round = (n: number) => Math.round(n * 100) / 100;
