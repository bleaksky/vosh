import { useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { closeSession as closeSessionCall, type SessionRow } from '../ipc/session';
import { errorText } from '../lib/text';
import { sessionLive } from '../stores/session/connectionStore';
import { getSelected, getSessions, select } from '../stores/session/sessionsStore';
import { pushToast } from '../stores/toasts';
import { closeSessionQuestion, closeWindowQuestion, type CloseQuestion } from './closeQuestions';

// Closing a session and the main window, by Q13 of the Sessions review.
// Close session asks while its session is connected and closes at once
// otherwise, and Close window asks while any session is connected. Each
// question names the sessions as closeQuestions words them, and the main
// window draws it. Closing the last session closes the window.

/** The question on screen, with what its danger button does. */
export interface Asking extends CloseQuestion {
  onConfirm: () => void;
}

export interface Closing {
  /** The question waiting on your answer, or null. */
  asking: Asking | null;
  /** Cancel, Escape or a press outside the question. */
  cancel: () => void;
  /** Close a session, the selected one when none is named. */
  closeSession: (session?: number) => void;
  closeWindow: () => void;
}

/** Every open session, connected while the app says so or while this
 *  window heard it dial or connect, since the app marks a session in
 *  only once its connection runs. */
function closeRows(): SessionRow[] {
  return getSessions().map((row) => ({ ...row, connected: row.connected || sessionLive(row.id) }));
}

/** Close the session `session` names. While it is selected, the session
 *  after it comes to the front first, or the one before it when it is
 *  last, as the app picks, so the window opens that session before the
 *  row goes. */
async function endSession(session: number): Promise<void> {
  const rows = getSessions();
  const at = rows.findIndex((row) => row.id === session);
  if (session === getSelected() && at >= 0) {
    const next = rows[at + 1] ?? rows[at - 1];
    if (next) await select(next.id);
  }
  await closeSessionCall(session);
}

function closeMainWindow(): void {
  getCurrentWindow()
    .close()
    .catch((e: unknown) => console.error('[main] closing the window failed', e));
}

/** The main window's closing. Mount it once, in MainWindow. */
export function useClosing(): Closing {
  const [asking, setAsking] = useState<Asking | null>(null);

  /** Ask `question` and run `close` on its danger button, or run it at
   *  once with no question. */
  const ask = (question: CloseQuestion | null, close: () => void) => {
    if (!question) {
      close();
      return;
    }
    setAsking({
      ...question,
      onConfirm: () => {
        setAsking(null);
        close();
      },
    });
  };

  const closeWindow = () => ask(closeWindowQuestion(closeRows()), closeMainWindow);

  const closeSession = (session = getSelected()) => {
    const rows = closeRows();
    if (rows.length <= 1) {
      closeWindow();
      return;
    }
    ask(closeSessionQuestion(session, rows), () => {
      endSession(session).catch((e: unknown) =>
        pushToast({ kind: 'error', message: errorText(e) || 'Vosh could not close the session.' }),
      );
    });
  };

  return { asking, cancel: () => setAsking(null), closeSession, closeWindow };
}
