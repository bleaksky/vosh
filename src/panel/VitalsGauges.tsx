import { VitalsMarks, type MarkedVitalsProps } from './VitalsMarks';
import type { GaugesFit } from './vitalsMarksFit';

// Gauges (Vitals Styles, board 1): the Group pane's member row made a
// little bolder, a 6 px pill on the divider tone between each label and
// value that fills in the vital's tone. A pill keeps 40 at least, and
// on a narrower panel each drops under its label and value
// (vitalsMarksFit.ts). Gauges draws its own mark, so Meter leaves it
// alone.

export function VitalsGauges({ fit, ...props }: MarkedVitalsProps & { fit: GaugesFit }) {
  return (
    <VitalsMarks
      {...props}
      kind="gauges"
      under={fit === 'under'}
      mark={(row) => (
        <span className="vitals-gauge" aria-hidden="true">
          {row.pct !== null && (
            <span className="vitals-gauge-fill" style={{ width: `${row.pct}%` }} />
          )}
        </span>
      )}
    />
  );
}
