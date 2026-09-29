import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { CloseIcon, MaximizeIcon, MinimizeIcon } from './ui';

/** Minimize, maximize, and close for the frameless Settings window on
 *  Windows and Linux, at the right end of the header like the main
 *  window's title band. Maximize reads Restore while the window is
 *  maximized. macOS draws its own traffic lights instead. */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const win = getCurrentWindow();
    const read = () => {
      win
        .isMaximized()
        .then((m) => {
          if (!cancelled) setMaximized(m);
        })
        .catch(() => {});
    };
    read();
    win
      .onResized(read)
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
  return (
    <div className="st-window-controls">
      <button
        type="button"
        className="st-icon-button"
        aria-label="Minimize"
        onClick={() => void getCurrentWindow().minimize()}
      >
        <MinimizeIcon />
      </button>
      <button
        type="button"
        className="st-icon-button"
        aria-label={maximized ? 'Restore' : 'Maximize'}
        onClick={() => void getCurrentWindow().toggleMaximize()}
      >
        <MaximizeIcon />
      </button>
      <button
        type="button"
        className="st-icon-button is-close"
        aria-label="Close"
        onClick={() => void getCurrentWindow().close()}
      >
        <CloseIcon />
      </button>
    </div>
  );
}
