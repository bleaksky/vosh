import type { MouseEvent } from 'react';
import {
  closeAlertNotice,
  showAlertNotice,
  useAlertNotice,
} from '../../stores/session/alertNoticeStore';

// A press on Close or Show leaves the caret on the command line.
const keepCaret = (event: MouseEvent) => event.preventDefault();

// The notice of an alert from a session you are not looking at, board 5
// of the Sessions review (Q10). It sits on the update notice recipe with
// the accent dot, names the alert and the session it rang in, and never
// shows the words (Alerts Q3). Show selects that session, and the notice
// goes with it. Close puts the notice away and leaves the dot and the
// count on the session's row, which go once you look at it.
export function AlertNotice() {
  const notice = useAlertNotice();
  if (!notice) return null;
  return (
    <div className="ov-update" role="status" aria-live="polite">
      <span className="ov-update-dot" aria-hidden="true" />
      <span className="ov-update-msg">{notice.title}</span>
      {notice.label !== null && <span className="ov-update-meta">to {notice.label}</span>}
      <span className="ov-update-actions">
        <button
          type="button"
          className="ov-button"
          onMouseDown={keepCaret}
          onClick={closeAlertNotice}
        >
          Close
        </button>
        <button
          type="button"
          className="ov-button"
          onMouseDown={keepCaret}
          onClick={showAlertNotice}
        >
          Show
        </button>
      </span>
    </div>
  );
}
