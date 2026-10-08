import type { MouseEvent } from 'react';
import { end, unfold, useGetStarted } from '../getStarted/getStartedStore';
import { progress, stepsFor } from '../getStarted/steps';

// A press on the notice's buttons leaves the caret on the command line.
const keepCaret = (event: MouseEvent) => event.preventDefault();

// Get started folded to the corner (First Run board 1). Connect, Esc and
// Show me fold the card here, on the update notice recipe, and it counts
// what you finished. Open brings the card back where you left it, and
// Close ends Get started with the toast that Help opens it again. It
// lasts until you quit.
export function GetStartedNotice() {
  const view = useGetStarted();
  if (view.shows !== 'folded') return null;
  return (
    <div className="ov-update" role="status" aria-live="polite">
      <span className="ov-update-dot" aria-hidden="true" />
      <span className="ov-update-msg">Get started</span>
      <span className="ov-update-meta">
        {progress(stepsFor(view.target), view.saved?.done ?? [])}
      </span>
      <span className="ov-update-actions">
        <button type="button" className="ov-button" onMouseDown={keepCaret} onClick={end}>
          Close
        </button>
        <button
          type="button"
          className="ov-button is-primary"
          onMouseDown={keepCaret}
          onClick={unfold}
        >
          Open
        </button>
      </span>
    </div>
  );
}
