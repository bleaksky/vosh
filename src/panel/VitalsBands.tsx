import type { Fight } from '../stores/gmcp/combatStore';
import { HitGhost } from './HitGhost';
import { DrawnOpponent, DrawnVitals, type DrawnVitalsProps } from './VitalsDrawn';
import { hitFill, type HitView } from './vitalsHit';
import { maxOf, meterFill, toneProps, VITAL_LABELS } from './vitalsView';

// Bands (More Vitals Styles, board 1): Stephen Few's bullet graph laid
// flat under each label and value. Two quiet bands mark under a
// quarter, where low lets go, and under two thirds, where Warn before
// you run low starts. A 4 px bar runs through the middle, and in a
// fight a 2 px tick stands where the vital was as the fight began, and
// where your opponent's health first stood. Bands always stacks, so a
// narrow panel only draws a shorter graph. With Show each hit on, the
// part a hit took stays pale on the bar's line.

export function VitalsBands({
  rows,
  waiting,
  combat,
  place,
  inks,
  hits,
  fight,
}: DrawnVitalsProps & { fight: Fight | null }) {
  const start = combat ? (fight?.start ?? null) : null;
  const foe = combat && (
    <DrawnOpponent
      combat={combat}
      rows={rows}
      waiting={waiting}
      place={place}
      under={(health) => (
        <BandGraph pct={health.pct} hit={hits.foe} tick={fight?.healths[0] ?? null} />
      )}
    />
  );
  return (
    <DrawnVitals kind="bands" waiting={waiting} place={place} foe={foe}>
      {rows.map((row) => (
        <div key={row.key} {...toneProps(row.tone, inks[row.key], 'vitals-band')}>
          <div className="vitals-band-text">
            <span className="vitals-mark-label">{VITAL_LABELS[row.key]}</span>
            <span className="vitals-mark-value">{row.value}</span>
          </div>
          <BandGraph
            pct={row.pct}
            hit={hits[row.key]}
            tick={start ? meterFill(start[row.key], start[maxOf(row.key)]) : null}
          />
        </div>
      ))}
    </DrawnVitals>
  );
}

/** The quiet bands, the bar at `pct` and the tick at `tick`. */
function BandGraph({
  pct,
  hit,
  tick,
}: {
  pct: number | null;
  hit: HitView | undefined;
  tick: number | null;
}) {
  const { fill, ghost, draining } = hitFill(pct, hit);
  return (
    <div className="vitals-band-graph" aria-hidden="true">
      <span className="vitals-band-zone is-low" />
      <span className="vitals-band-zone is-mid" />
      {fill !== null && ghost !== null && (
        <HitGhost className="vitals-band-gone" fill={fill} ghost={ghost} draining={draining} />
      )}
      {fill !== null && <span className="vitals-band-bar" style={{ width: `${fill}%` }} />}
      {tick !== null && (
        <span
          className={tick >= 100 ? 'vitals-band-tick is-end' : 'vitals-band-tick'}
          style={{ left: `${tick}%` }}
        />
      )}
    </div>
  );
}
