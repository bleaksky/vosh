import { useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { ConnectionStatus } from '../stores/session/connectionStore';

/** The window title for a session. The window hides its title, but the
 *  macOS Window menu and Mission Control still list it, as do taskbars
 *  elsewhere. It names your character and world while a session is up,
 *  the world alone before you log in, and Vosh otherwise. */
export function windowTitle(
  status: ConnectionStatus,
  character: string | null,
  world: string,
): string {
  if (status.kind !== 'connected') return 'Vosh';
  return character ? `${character} on ${world}` : world;
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
