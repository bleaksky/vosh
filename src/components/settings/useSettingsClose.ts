import { useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  currentCloseGuard,
  listenForQuitFlush,
  runCloseRequest,
  sendPendingWrites,
} from '../../lib/pendingWrites';

/** The Settings window's close handler, the one for every way it
 *  closes: the close button, Close window in the menu bar (Cmd+W),
 *  and the main window closing. It leaves the focused field, so a
 *  number or color you typed saves, sends every write waiting on a
 *  pause, and then closes, or lets a page with unsaved changes ask
 *  first. It also answers the backend on quit with the same writes. */
export function useSettingsClose(): void {
  useEffect(() => {
    const win = getCurrentWindow();
    let cancelled = false;
    // Tauri's unlisten returns a promise at runtime, and the close has
    // to wait for it, or the backend still sees a listener and holds
    // the close with nobody left to answer.
    let unlisten: (() => void | Promise<void>) | null = null;
    let unlistenQuit: (() => void) | null = null;
    let closing = false;
    // Close for real. The listener goes first, or this close would
    // come back here. With no listener left, Tauri closes the window.
    const close = async () => {
      const stop = unlisten;
      unlisten = null;
      if (stop) await stop();
      await win.close();
    };
    win
      .onCloseRequested(async (event) => {
        // Always hold the close here. This page closes the window itself
        // once the writes are out, or once the guard says so.
        event.preventDefault();
        if (closing) return;
        closing = true;
        try {
          await runCloseRequest({
            send: () => sendPendingWrites({ commitFocus: true }),
            guard: currentCloseGuard,
            close,
          });
        } finally {
          closing = false;
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch((e: unknown) => console.error('[settings] close listener failed', e));
    listenForQuitFlush({ commitFocus: true })
      .then((fn) => {
        if (cancelled) fn();
        else unlistenQuit = fn;
      })
      .catch((e: unknown) => console.error('[settings] quit listener failed', e));
    return () => {
      cancelled = true;
      void unlisten?.();
      unlistenQuit?.();
    };
  }, []);
}
