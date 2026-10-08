import type { Fight } from '../stores/gmcp/combatStore';
import type { VitalSample } from '../stores/gmcp/vitalsStore';
import { DrawnOpponent, DrawnVitals, MarkRows, type DrawnVitalsProps } from './VitalsDrawn';
import { traceSeries, type RowMarkFit } from './vitalsDrawnFit';

// Traces (More Vitals Styles, board 3): each vital over its last
// minute, a line over the area it fills, from empty at the foot to
// your max at the top, after Edward Tufte's sparklines. The height at
// the right edge reads like a gauge, a dot marks now, and the value
// sits beside it. Your opponent's trace spans the whole fight. A trace
// already draws each hit, so Show each hit leaves it alone. On a narrow
// panel each trace drops under its label and value (vitalsDrawnFit.ts).

export function VitalsTraces({
  rows,
  waiting,
  combat,
  place,
  inks,
  fit,
  history,
  fight,
}: DrawnVitalsProps & {
  fit: RowMarkFit;
  history: readonly VitalSample[];
  fight: Fight | null;
}) {
  const foe = combat && (
    <DrawnOpponent
      combat={combat}
      rows={rows}
      waiting={waiting}
      place={place}
      under={(health) =>
        health.pct === null ? (
          <Trace series={null} />
        ) : (
          <Trace
            series={(fight?.healths.length ? fight.healths : [health.pct]).map((p) => p / 100)}
          />
        )
      }
    />
  );
  return (
    <DrawnVitals kind="traces" waiting={waiting} place={place} foe={foe}>
      <MarkRows
        kind="traces"
        under={fit === 'under'}
        rows={rows}
        inks={inks}
        mark={(row) => (
          <Trace series={row.pct === null ? null : traceSeries(history, row.key, row.pct)} />
        )}
      />
    </DrawnVitals>
  );
}

/** The trace of `series`, shares of the max oldest first, in a box 100
 *  wide and 16 high that stretches to the mark, or only its baseline
 *  for a value the game hides. */
function Trace({ series }: { series: readonly number[] | null }) {
  const points = series === null ? [] : series.length === 1 ? [series[0], series[0]] : series;
  const y = (share: number) => round(16 - share * 15);
  const xy = points.map((share, i) => `${round((i / (points.length - 1)) * 100)} ${y(share)}`);
  const last = points[points.length - 1];
  return (
    <span className="vitals-trace">
      <svg viewBox="0 0 100 16" preserveAspectRatio="none" aria-hidden="true">
        {last !== undefined && (
          <path className="vitals-trace-area" d={`M0 16 L${xy.join(' L')} L100 16 Z`} />
        )}
        <line
          className="vitals-trace-base"
          x1="0"
          y1="15.5"
          x2="100"
          y2="15.5"
          vectorEffect="non-scaling-stroke"
        />
        {last !== undefined && (
          <path
            className="vitals-trace-line"
            d={`M${xy.join(' L')}`}
            vectorEffect="non-scaling-stroke"
          />
        )}
      </svg>
      {last !== undefined && (
        <i className="vitals-trace-now" style={{ top: `${round((y(last) / 16) * 100)}%` }} />
      )}
    </span>
  );
}

const round = (n: number) => Math.round(n * 100) / 100;
