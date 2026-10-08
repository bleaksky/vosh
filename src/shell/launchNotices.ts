import { launchNoticesTake, type LaunchNotice } from '../ipc/windows';
import { pushToast } from '../stores/toasts';

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
 *  `write` and as a toast of its kind. The backend hands them over once,
 *  so a second call shows nothing. */
export async function showLaunchNotices(write: (text: string) => void): Promise<void> {
  let notices: LaunchNotice[];
  try {
    notices = await launchNoticesTake();
  } catch (e) {
    console.warn('[launch] notices unavailable', e);
    return;
  }
  for (const { kind, message } of notices) {
    write(launchNoticeLine(message));
    pushToast({ kind, message });
  }
}

/** What the main window says once the shared catalog wizard wrote its
 *  files. Nothing the session changes saves until Vosh opens again, since
 *  the live profile still holds the items the move took out of the files. */
export const MIGRATION_APPLIED_NOTICE =
  'The move to loadouts is done. Vosh does not save the changes you make before you quit, so quit Vosh and open it again now.';

/** Show the notice after the move to loadouts in the terminal through
 *  `write`, and in a toast that stays up until you close it. */
export function showMigrationApplied(write: (text: string) => void): void {
  write(launchNoticeLine(MIGRATION_APPLIED_NOTICE));
  pushToast({ kind: 'info', message: MIGRATION_APPLIED_NOTICE, sticky: true });
}
