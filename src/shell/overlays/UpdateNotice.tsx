import { useEffect, useState, type MouseEvent } from 'react';
import { getUiConfig } from '../../ipc/uiConfig';
import {
  checkForUpdate,
  installUpdateAndRelaunch,
  type UpdateCheckResult,
} from '../../ipc/updater';
import { Button } from '../../ui';

// A press on the notice's buttons leaves the caret on the command line,
// so a click never strands focus on a button that is about to unmount.
// Keyboard users still reach the buttons with Tab.
const keepCaret = (event: MouseEvent) => event.preventDefault();

// Update notice at the bottom right of the terminal column, where the
// toasts sit. It checks once on mount when auto update is on and shows
// a floating card (radius 16, the floating card recipe) while a new
// version is out: a dot in the accent, Update available, the version in
// the tertiary tone, then Later and Install and restart. Install
// downloads, installs, and relaunches from the backend. A failed
// install turns the dot to danger, says so, and offers Try again.
// Toasts that arrive meanwhile stack above the card (overlays.css).
export function UpdateNotice() {
  const [update, setUpdate] = useState<UpdateCheckResult | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const cfg = await getUiConfig();
        if (!cfg.auto_update) return;
        // Brief delay so the app finishes paint + connect first.
        await new Promise((r) => setTimeout(r, 2000));
        if (cancelled) return;
        const result = await checkForUpdate();
        if (cancelled) return;
        if (result.available) setUpdate(result);
      } catch (e) {
        // Silent fail on launch — no need to badger the user if
        // GitHub is unreachable. They can still update manually.
        console.warn('[updater] check failed', e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (!update || !update.available || dismissed) return null;

  const handleInstall = async () => {
    setInstalling(true);
    setError(null);
    try {
      await installUpdateAndRelaunch();
      // App restarts; this line probably never runs.
    } catch (e) {
      setError(String(e));
      setInstalling(false);
    }
  };

  let message = 'Update available';
  if (error) message = 'Update failed';
  else if (installing) message = 'Installing update';
  const meta = error ?? update.version;

  return (
    <div className={`ov-update${error ? ' is-error' : ''}`} role="status" aria-live="polite">
      <span
        className={`ov-update-dot dot ${error ? 'is-danger' : 'is-accent'}`}
        aria-hidden="true"
      />
      <span className="ov-update-msg">{message}</span>
      {meta && (
        <span className="ov-update-meta" title={error ?? undefined}>
          {meta}
        </span>
      )}
      <span className="ov-update-actions">
        <Button onMouseDown={keepCaret} onClick={() => setDismissed(true)} disabled={installing}>
          Later
        </Button>
        <Button
          variant="primary"
          onMouseDown={keepCaret}
          onClick={() => void handleInstall()}
          disabled={installing}
        >
          {installing ? 'Installing…' : error ? 'Try again' : 'Install and restart'}
        </Button>
      </span>
    </div>
  );
}
