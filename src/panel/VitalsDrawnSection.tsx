import type { CSSProperties, ReactNode, Ref } from 'react';
import type { Fight } from '../stores/gmcp/combatStore';
import { VitalsBands } from './VitalsBands';
import { VitalsBlocks } from './VitalsBlocks';
import type { DrawnVitalsProps } from './VitalsDrawn';
import { bandsHeight } from './vitalsDrawnFit';
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
}) {
  let body: ReactNode;
  let height: number;
  let under = false;
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
    case 'blocks':
      body = <VitalsBlocks {...props} fit={fit.fit} />;
      height = marksHeight(size, mine);
      under = fit.fit === 'under';
      break;
  }
  return (
    <section
      ref={sectionRef}
      className={`panel-vitals panel-vitals-marks${under ? ' is-under' : ''}`}
      style={
        props.waiting ? ({ '--vitals-min-height': `${height}px` } as CSSProperties) : undefined
      }
      aria-label={label}
    >
      {body}
    </section>
  );
}
