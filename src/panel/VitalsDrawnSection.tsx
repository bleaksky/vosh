import type { CSSProperties, ReactNode, Ref } from 'react';
import type { Fight } from '../stores/gmcp/combatStore';
import { VitalsBands } from './VitalsBands';
import { VitalsBlocks } from './VitalsBlocks';
import type { DrawnVitalsProps } from './VitalsDrawn';
import { bandsHeight, dialsHeight, tracesHeight } from './vitalsDrawnFit';
import { VitalsDials } from './VitalsDials';
import type { VitalsValues } from '../ipc/uiConfig';
import { VitalsTraces } from './VitalsTraces';
import type { VitalSample } from '../stores/gmcp/vitalsStore';
import type { DrawnFit } from './vitalsFit';
import { VitalsLadders } from './VitalsLadders';
import { marksHeight } from './vitalsMarksFit';

// The footer of each style of the More Vitals Styles review, the
// section VitalsBlock draws for it: the row styles on the Gauges pads,
// and the height each holds while it waits for your vitals.

export function DrawnSection({
  fit,
  sectionRef,
  label,
  size,
  mine,
  fight,
  history,
  values,
  ...props
}: DrawnVitalsProps & {
  fit: DrawnFit;
  sectionRef?: Ref<HTMLElement> | undefined;
  label: string;
  /** Your panel size in px. */
  size: number;
  /** How many of your vitals the footer holds room for. */
  mine: number;
  fight: Fight | null;
  /** Your last Char.Vitals, oldest first, which Traces draws. */
  history: readonly VitalSample[];
  /** The Values form, which the column styles write their figures in. */
  values: VitalsValues;
}) {
  let body: ReactNode;
  let height: number;
  let under = false;
  let cols = false;
  switch (fit.style) {
    case 'bands':
      body = <VitalsBands {...props} fight={fight} />;
      height = bandsHeight(size, mine);
      break;
    case 'ladders':
      body = <VitalsLadders {...props} fit={fit.fit} />;
      height = marksHeight(size, mine);
      under = fit.fit === 'under';
      break;
    case 'traces':
      body = <VitalsTraces {...props} history={history} fight={fight} fit={fit.fit} />;
      height = tracesHeight(size, mine);
      under = fit.fit === 'under';
      break;
    case 'blocks':
      body = <VitalsBlocks {...props} fit={fit.fit} />;
      height = marksHeight(size, mine);
      under = fit.fit === 'under';
      break;
    case 'dials':
      body = <VitalsDials {...props} values={values} fit={fit.fit} />;
      height = dialsHeight(size, fit.fit);
      cols = true;
      break;
  }
  return (
    <section
      ref={sectionRef}
      className={`panel-vitals panel-vitals-marks${under ? ' is-under' : ''}${cols ? ' is-cols' : ''}`}
      style={
        props.waiting ? ({ '--vitals-min-height': `${height}px` } as CSSProperties) : undefined
      }
      aria-label={label}
    >
      {body}
    </section>
  );
}
