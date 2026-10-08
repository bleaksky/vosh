import { DrawnOpponent, DrawnVitals, LineMark, type DrawnVitalsProps } from './VitalsDrawn';
import { RING_RADII, RINGS, type RingsFit } from './vitalsDrawnFit';
import { hitFill } from './vitalsHit';
import { MarkValue } from './VitalsMarks';
import { toneProps, VITAL_LABELS } from './vitalsView';
import { VisuallyHidden } from '../ui';

// Rings (More Vitals Styles, board 3): your vitals as arcs nested in
// one 56 pt glyph, outer to inner in your order, with a legend of
// labels and values beside it, after Apple's Activity rings. Each arc
// starts at the top and runs clockwise, and with Show each hit on the
// part a hit took stays pale along it. Your opponent draws its name and
// health over a 2 px line in warn. Where a label would not fit whole
// beside its value, the legend keeps only the keys (vitalsDrawnFit.ts).

export function VitalsRings({
  rows,
  waiting,
  combat,
  place,
  inks,
  hits,
  fit,
}: DrawnVitalsProps & { fit: RingsFit }) {
  const foe = combat && (
    <DrawnOpponent
      combat={combat}
      rows={rows}
      waiting={waiting}
      place={place}
      under={(health) => <LineMark pct={health.pct} hit={hits.foe} />}
    />
  );
  const c = RINGS / 2;
  return (
    <DrawnVitals kind="rings" waiting={waiting} place={place} foe={foe}>
      {rows.length > 0 && (
        <div className="vitals-rings">
          <svg width={RINGS} height={RINGS} viewBox={`0 0 ${RINGS} ${RINGS}`} aria-hidden="true">
            {rows.slice(0, RING_RADII.length).map((row, i) => {
              const r = RING_RADII[i];
              const { fill, ghost, draining } = hitFill(row.pct, hits[row.key]);
              const turn = `rotate(-90 ${c} ${c})`;
              return (
                <g key={row.key} {...toneProps(row.tone, inks[row.key], 'vitals-ring')}>
                  <circle className="vitals-ring-track" cx={c} cy={c} r={r} />
                  {fill !== null && ghost !== null && (
                    <circle
                      className={`vitals-ring-gone vitals-ghost${draining ? ' is-draining' : ''}`}
                      cx={c}
                      cy={c}
                      r={r}
                      pathLength={100}
                      transform={turn}
                      style={{
                        strokeDasharray: `0 ${round(fill)} ${draining ? 0 : round(ghost - fill)} 200`,
                      }}
                    />
                  )}
                  {fill !== null && fill > 0 && (
                    <circle
                      className="vitals-ring-arc"
                      cx={c}
                      cy={c}
                      r={r}
                      pathLength={100}
                      transform={turn}
                      style={{ strokeDasharray: `${round(fill)} 200` }}
                    />
                  )}
                </g>
              );
            })}
          </svg>
          <div className="vitals-rings-legend">
            {rows.map((row) => (
              <div key={row.key} {...toneProps(row.tone, inks[row.key], 'vitals-ring-row')}>
                <span className="vitals-mark-label">
                  <i className="vitals-ring-key" />
                  {fit === 'labels' ? (
                    VITAL_LABELS[row.key]
                  ) : (
                    <VisuallyHidden>{VITAL_LABELS[row.key]}</VisuallyHidden>
                  )}
                </span>
                <MarkValue value={row.value} widest={row.widest} />
              </div>
            ))}
          </div>
        </div>
      )}
    </DrawnVitals>
  );
}

const round = (n: number) => Math.round(n * 100) / 100;
