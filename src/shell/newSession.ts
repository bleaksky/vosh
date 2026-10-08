import { profileBeforeLogin, profilesList } from '../ipc/profiles';
import { closeSession, openSession } from '../ipc/session';
import { requestSessionMenu, type OpenedSession } from '../lib/appMenu';
import { errorText } from '../lib/text';
import { getSelected, getSessions, select } from '../stores/session/sessionsStore';
import { loadTarget } from '../stores/session/useConnection';
import { pushToast } from '../stores/toasts';

// New session…, from the session popover, the sidebar's plus, the macOS
// menu bar and the palette. It adds a row at once, on the profile the
// saved world picks, selects it, and opens the popover on its form. The
// form dials it or closes it again.

/** Open a session on the profile the saved world picks, select it, and
 *  open its New session form. */
export async function openNewSession(): Promise<void> {
  const previous = getSelected();
  try {
    const saved = loadTarget();
    const { active } = await profilesList();
    const front = getSessions().find((row) => row.id === previous)?.profile ?? active;
    const profile = (await profileBeforeLogin(saved.host, saved.port)) ?? front;
    const id = await openSession(profile);
    await select(id);
    const opened: OpenedSession = { id, previous, front, profile };
    requestSessionMenu({ mode: 'new', opened });
  } catch (e) {
    pushToast({ kind: 'error', message: errorText(e) || 'Vosh could not open a new session.' });
  }
}

/** Close a session its form left without dialing, and write nothing.
 *  While it is still selected, the session selected before it comes
 *  back first, so the window returns to where you were. */
export async function cancelNewSession(opened: OpenedSession): Promise<void> {
  try {
    const back = getSessions().some((row) => row.id === opened.previous);
    if (getSelected() === opened.id && back) await select(opened.previous);
    await closeSession(opened.id);
  } catch (e) {
    // The session closed some other way meanwhile.
    console.warn('[new session]', e);
  }
}
