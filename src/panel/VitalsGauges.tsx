import { HitGhost } from './HitGhost';
import { VitalsMarks, type MarkedVitalsProps } from './VitalsMarks';
import { hitFill, type HitViews } from './vitalsHit';
import type { GaugesFit } from './vitalsMarksFit';

// Gauges: the Group pane's member row made a
// little bolder, a 6 px pill on the divider tone between each label and
// value that fills in the vital's tone. A pill keeps 40 at least, and
// on a narrower panel each drops under its label and value
// (vitalsMarksFit.ts). Gauges draws its own mark, so Meter leaves it
// alone. With Show each hit on, the part a hit took stays pale beside
// the fill, which then ends square.

export function VitalsGauges({
  fit,
  hits,
  ...props
}: MarkedVitalsProps & { fit: GaugesFit; hits: HitViews }) {
  return (
    <VitalsMarks
      {...props}
      kind="gauges"
      under={fit === 'under'}
      mark={(row) => {
        const { fill, ghost, draining } = hitFill(row.pct, hits[row.key]);
        return (
          <span className="vitals-gauge" aria-hidden="true">
            {fill !== null && ghost !== null && (
              <HitGhost
                className="vitals-gauge-gone"
                fill={fill}
                ghost={ghost}
                draining={draining}
              />
            )}
            {fill !== null && (
              <span
                className={ghost === null ? 'vitals-gauge-fill' : 'vitals-gauge-fill is-hit'}
                style={{ width: `${fill}%` }}
              />
            )}
          </span>
        );
      }}
    />
  );
}
