// The tone of each alert a session rings. Every session sends
// session://alert, the ones you are not looking at too, with the tone
// to play in `sound`. Rust sends none when a system sound played in the
// tone's place, which happens only while the main window is minimized
// or hidden: an NSSound on macOS, or the toast's own sound on Windows
// while Windows lets Vosh show toasts. On Linux the page plays the tone
// as well, since a notification server may play no sound. Only the main
// window hears the event, so Settings and Help never ring.

import { onAlert } from '../ipc/alerts';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { playAlertTone } from '../stores/session/alertTones';

/** Play the tone of each alert a session rings, from mount to unmount. */
export function useAlertTones(): void {
  useTauriEvent(onAlert, (alert) => {
    if (alert.sound) playAlertTone(alert.sound);
  });
}
