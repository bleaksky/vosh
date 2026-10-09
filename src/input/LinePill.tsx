import type { ModePill as Pill } from './modePill';
import { VisuallyHidden } from '../ui';

// The pill at the start of the command line: the mode's name, then its
// count after a dot, the count in warn past the editor's limit. A screen
// reader hears the label, which reads the count as words.

export function LinePill({ pill }: { pill: Pill }) {
  return (
    <span className="input-pill" role="status">
      <VisuallyHidden>{pill.label}</VisuallyHidden>
      <PillFace pill={pill} />
    </span>
  );
}

function PillFace({ pill }: { pill: Pill }) {
  return (
    <>
      <span className="input-pill-name" aria-hidden="true">
        {pill.name}
      </span>
      {pill.count !== null && (
        <>
          <span className="input-pill-dot" aria-hidden="true">
            ·
          </span>
          <span className={`input-pill-count${pill.warn ? ' is-warn' : ''}`} aria-hidden="true">
            {pill.count}
          </span>
        </>
      )}
    </>
  );
}
