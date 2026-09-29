import { launchNoticesTake } from './session';
import { pushToast } from './toasts';

// What launch has to tell you, such as a profile file Vosh could not
// read and will not save over. Launch runs before any window listens,
// so the backend keeps the sentences until the main window takes them
// once its terminal holds the restored scrollback.

/** The terminal line for one notice, in the yellow the login switch
 *  line uses, on a line of its own. */
export function launchNoticeLine(sentence: string): string {
  return `\r\n\x1b[33m${sentence}\x1b[0m\r\n`;
}

/** Take the launch notices and show each one in the terminal through
 *  `write` and as a toast. The backend hands them over once, so a
 *  second call shows nothing. */
export async function showLaunchNotices(write: (text: string) => void): Promise<void> {
  let notices: string[];
  try {
    notices = await launchNoticesTake();
  } catch (e) {
    console.warn('[launch] notices unavailable', e);
    return;
  }
  for (const sentence of notices) {
    write(launchNoticeLine(sentence));
    pushToast({ kind: 'error', message: sentence });
  }
}
