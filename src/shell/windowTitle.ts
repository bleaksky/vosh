import { useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { ConnectionStatus } from '../stores/session/connectionStore';

/** The window title for a session. The window hides its title, but the
 *  macOS Window menu and Mission Control still list it, as do taskbars
 *  elsewhere. While a session is up it names the session, by the name
 *  you gave it or its character, and where it plays, the world with its
 *  port when the port is not the world's own. It names the place alone
 *  before you log in, and Vosh otherwise. */
export function windowTitle(status: ConnectionStatus, who: string | null, place: string): string {
  if (status.kind !== 'connected') return 'Vosh';
  return who ? `${who} on ${place}` : place;
}

/** Keep the window title on the session. Outside Tauri there is no
 *  window, and a failed call leaves the last title in place. */
export function useWindowTitle(title: string): void {
  useEffect(() => {
    try {
      getCurrentWindow()
        .setTitle(title)
        .catch(() => {});
    } catch {
      // No Tauri window here.
    }
  }, [title]);
}
