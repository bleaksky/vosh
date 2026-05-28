import { useEffect, useState } from 'react';
import {
  checkForUpdate,
  getUiConfig,
  installUpdateAndRelaunch,
  type UpdateCheckResult,
} from '../lib/session';

// Update notice that floats at the bottom of the app. Checks once
// on mount when ui.auto_update is enabled; renders a small banner
// when a new version is available. Install triggers backend
// download + install + relaunch.
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

  return (
    <div className="update-notice" role="status" aria-live="polite">
      <span className="update-notice-tag">update</span>
      <span className="update-notice-version">v{update.version}</span>
      <span className="update-notice-sep" aria-hidden="true">
        ·
      </span>
      <span className="update-notice-msg">{error ? error : 'new release available'}</span>
      <span className="update-notice-spacer" />
      <button
        type="button"
        className="settings-btn"
        onClick={() => void handleInstall()}
        disabled={installing}
      >
        {installing ? '[installing...]' : '[install + restart]'}
      </button>
      <button
        type="button"
        className="settings-btn settings-btn-mute"
        onClick={() => setDismissed(true)}
        disabled={installing}
      >
        [later]
      </button>
    </div>
  );
}
