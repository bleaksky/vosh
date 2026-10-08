import type { MouseEvent } from 'react';
import { stopWalk, type WalkProgress } from '../../ipc/session';
import { getSelected } from '../../stores/session/sessionsStore';

// Where a walk stands, at the bottom of the map (Scripts and Panels
// review, board 9). While you walk it is the Walking chip on the update
// notice's shape, since it holds Stop, with the steps left as a #walk
// string. A stopped walk leaves a toast with how far it got, or just
// Stopped when Vosh lost track of it, as the terminal line says.

// A press on Stop leaves the caret on the command line.
const keepCaret = (event: MouseEvent) => event.preventDefault();

const steps = (n: number) => (n === 1 ? 'step' : 'steps');

export function WalkStatus({ progress }: { progress: WalkProgress }) {
  if (progress.kind === 'walking') {
    const left = progress.total - progress.done;
    return (
      <div className="walk-chip ov-update" role="status">
        <span className="ov-update-msg">
          {left} {steps(left)} left
        </span>
        <span className="ov-update-meta is-mono">{progress.left}</span>
        <span className="ov-update-actions">
          <button
            type="button"
            className="ov-button"
            onMouseDown={keepCaret}
            onClick={() => void stopWalk(getSelected()).catch(() => {})}
          >
            Stop
          </button>
        </span>
      </div>
    );
  }
  if (progress.kind === 'stopped') {
    return (
      <div className="walk-chip ov-toast" role="status">
        <span className="ov-toast-dot" aria-hidden="true" />
        <span className="ov-toast-msg">
          {progress.why === 'lost_track'
            ? 'Stopped'
            : `Stopped after ${progress.done} of ${progress.total} ${steps(progress.total)}`}
        </span>
      </div>
    );
  }
  return null;
}
