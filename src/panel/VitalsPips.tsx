import { VitalsMarks, type MarkedVitalsProps } from './VitalsMarks';
import { hitFill, type HitViews } from './vitalsHit';
import { pipLights, type PipLight, type PipsFit } from './vitalsMarksFit';

// Pips (Vitals Styles, board 1): ten 6 px discs beside each value, one
// for every tenth, lit in halves as the status line's moons are, the
// whole disc at a fifth of its tone and the lit part at full. Five
// discs, each a fifth, where ten do not fit beside the whole label,
// then ten under the label and value (vitalsMarksFit.ts). Pips draws
// its own mark, so Meter leaves it alone. With Show each hit on, the
// discs a hit put out stay pale, and a half lit disc keeps its lit
// half.

export function VitalsPips({
  fit,
  hits,
  ...props
}: MarkedVitalsProps & { fit: PipsFit; hits: HitViews }) {
  const count = fit === 'five' ? 5 : 10;
  return (
    <VitalsMarks
      {...props}
      kind="pips"
      under={fit === 'under'}
      mark={(row) => {
        const { fill, ghost } = hitFill(row.pct, hits[row.key]);
        const pale = ghost === null ? null : pipLights(ghost, count);
        return (
          <span className="vitals-pips" aria-hidden="true">
            {pipLights(fill, count).map((light, i) => (
              <span key={i} className={pipClass(light, pale?.[i] ?? 'off')} />
            ))}
          </span>
        );
      }}
    />
  );
}

/** A disc lit `light`, which the pale part a hit left lit `pale`. */
function pipClass(light: PipLight, pale: PipLight): string {
  if (light === 'full') return 'vitals-pip is-full';
  if (light === 'half') return pale === 'full' ? 'vitals-pip is-half-gone' : 'vitals-pip is-half';
  return pale === 'off' ? 'vitals-pip' : 'vitals-pip is-gone';
}
