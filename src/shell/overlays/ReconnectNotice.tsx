import { useEffect, useState, type MouseEvent } from 'react';
import { reconnectCancel, reconnectNow } from '../../ipc/session';
import { useReconnect } from '../../stores/session/reconnectStore';
import { Button } from '../../ui';

// A press on the notice's buttons leaves the caret on the command line,
// as on the update notice.
const keepCaret = (event: MouseEvent) => event.preventDefault();

/** Whole seconds left until `until`, never under 1, as the count holds
 *  there until the try's dialing step lands. */
const secondsLeft = (until: number, now: number): number =>
  Math.max(1, Math.ceil((until - now) / 1000));

interface Props {
  /** The selected session, whose redial the notice shows. */
  session: number;
  /** Try again, which dials the session as Connect does. */
  onTryAgain: () => void;
  /** Cancel or Reconnect now failed in `session`. */
  onError: (message: string, session: number) => void;
}

// The reconnect notice, in the update notice's card at the toasts'
// corner. While a try waits it counts down with Cancel and Reconnect
// now, while a try dials it rings in the success tone with Cancel, and
// once the tries run out it offers Try again, which dials as Connect
// does.
export function ReconnectNotice({ session, onTryAgain, onError }: Props) {
  const redial = useReconnect();
  const [now, setNow] = useState(Date.now);
  const waiting = redial.kind === 'waiting';

  useEffect(() => {
    if (!waiting) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [waiting, redial]);

  if (redial.kind === 'none') return null;

  const act = (run: (session: number) => Promise<void>) => () => {
    run(session).catch((e: unknown) => onError(String(e), session));
  };

  let message: string;
  let meta: string | null = null;
  if (redial.kind === 'waiting') {
    message = `Reconnecting in ${secondsLeft(redial.until, now)}s`;
    meta = `Try ${redial.try} of ${redial.tries}`;
  } else if (redial.kind === 'dialing') {
    message = 'Connecting';
    meta = `Try ${redial.try} of ${redial.tries}`;
  } else {
    message = `Vosh stopped after ${redial.tries} tries`;
  }

  const dialing = redial.kind === 'dialing';
  return (
    <div
      className={`ov-update ${dialing ? 'is-wait' : 'is-error'}`}
      role="status"
      aria-live="polite"
    >
      <span
        className={`ov-update-dot dot ${dialing ? 'is-off is-success' : 'is-danger'}`}
        aria-hidden="true"
      />
      <span className="ov-update-msg">{message}</span>
      {meta && <span className="ov-update-meta">{meta}</span>}
      <span className="ov-update-actions">
        {redial.kind === 'stopped' ? (
          <Button variant="primary" onMouseDown={keepCaret} onClick={onTryAgain}>
            Try again
          </Button>
        ) : (
          <Button onMouseDown={keepCaret} onClick={act(reconnectCancel)}>
            Cancel
          </Button>
        )}
        {redial.kind === 'waiting' && (
          <Button variant="primary" onMouseDown={keepCaret} onClick={act(reconnectNow)}>
            Reconnect now
          </Button>
        )}
      </span>
    </div>
  );
}
