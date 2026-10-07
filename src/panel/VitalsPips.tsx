import { VitalsMarks, type MarkedVitalsProps } from './VitalsMarks';
import { pipLights, type PipsFit } from './vitalsMarksFit';

// Pips (Vitals Styles, board 1): ten 6 px discs beside each value, one
// for every tenth, lit in halves as the status line's moons are, the
// whole disc at a fifth of its tone and the lit part at full. Five
// discs, each a fifth, where ten do not fit beside the whole label,
// then ten under the label and value (vitalsMarksFit.ts). Pips draws
// its own mark, so Meter leaves it alone.

export function VitalsPips({ fit, ...props }: MarkedVitalsProps & { fit: PipsFit }) {
  const count = fit === 'five' ? 5 : 10;
  return (
    <VitalsMarks
      {...props}
      kind="pips"
      under={fit === 'under'}
      mark={(row) => (
        <span className="vitals-pips" aria-hidden="true">
          {pipLights(row.pct, count).map((light, i) => (
            <span key={i} className={light === 'off' ? 'vitals-pip' : `vitals-pip is-${light}`} />
          ))}
        </span>
      )}
    />
  );
}
