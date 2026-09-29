import { useEffect, useRef } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

/** Ask before the window closes while `active` is true. `ask` gets a
 *  `proceed` callback that closes the window after all. The guard
 *  listens only while there is something to lose, since a close
 *  listener keeps Tauri from closing the window on its own. */
export function useCloseGuard(active: boolean, ask: (proceed: () => void) => void): void {
  const askRef = useRef(ask);
  useEffect(() => {
    askRef.current = ask;
  }, [ask]);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    let unlisten: (() => void | Promise<void>) | null = null;
    const win = getCurrentWindow();
    win
      .onCloseRequested((event) => {
        event.preventDefault();
        askRef.current(() => {
          void (async () => {
            // Stop listening first, or this close would ask again.
            const stop = unlisten;
            unlisten = null;
            if (stop) await stop();
            await win.close();
          })();
        });
      })
      .then((fn) => {
        if (cancelled) void fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      const stop = unlisten;
      unlisten = null;
      if (stop) void stop();
    };
  }, [active]);
}
