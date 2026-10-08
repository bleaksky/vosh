import { useEffect, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { closeSession as closeSessionCall, type SessionRow } from '../ipc/session';
import { appQuit } from '../ipc/windows';
import { errorText } from '../lib/text';
import { sessionLive } from '../stores/session/connectionStore';
import { getSelected, getSessions, select } from '../stores/session/sessionsStore';
import { pushToast } from '../stores/toasts';
import {
  closeSessionQuestion,
  closeWindowQuestion,
  quitQuestion,
  type CloseQuestion,
} from './closeQuestions';

// Closing a session, the main window and the app. Close session asks
// while its session is connected and closes at once otherwise, Close
// window asks while any session is connected, and Quit while two or
// more are. On macOS the menu bar's Quit hands the question here only
// then, and quits at once otherwise. Each question names the sessions
// as closeQuestions words them, and the main window draws it. Closing
// the last session closes the window.
//
// The red light on macOS and the close button on Windows and Linux reach
// the window's close request, which the window always holds and answers
// itself, so they ask as the Close window command does. A close for
// real lets go of the request first, as Settings does, or it would come
// back here.

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
  /** Quit Vosh, asking first while two or more sessions are
   *  connected. */
  quit: () => void;
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

/** The main window's closing. Mount it once, in MainWindow. */
export function useClosing(): Closing {
  const [asking, setAsking] = useState<Asking | null>(null);
  // Lets go of the close request, so the close that follows goes
  // through. Null until the window holds it.
  const letGo = useRef<(() => void | Promise<void>) | null>(null);

  const closeMainWindow = () => {
    const stop = letGo.current;
    letGo.current = null;
    void Promise.resolve(stop?.())
      .then(() => getCurrentWindow().close())
      .catch((e: unknown) => console.error('[main] closing the window failed', e));
  };

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

  // The window's close request, as the red light or the close button
  // sends it. The latest closeWindow answers it.
  const closeWindowRef = useRef(closeWindow);
  useEffect(() => {
    closeWindowRef.current = closeWindow;
  });
  useEffect(() => {
    let cancelled = false;
    getCurrentWindow()
      .onCloseRequested((event) => {
        event.preventDefault();
        closeWindowRef.current();
      })
      .then((fn) => {
        if (cancelled) fn();
        else letGo.current = fn;
      })
      .catch((e: unknown) => console.error('[main] close listener failed', e));
    return () => {
      cancelled = true;
      void letGo.current?.();
      letGo.current = null;
    };
  }, []);

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

  const quit = () =>
    ask(quitQuestion(closeRows()), () => {
      appQuit().catch((e: unknown) => console.error('[main] quitting failed', e));
    });

  return { asking, cancel: () => setAsking(null), closeSession, closeWindow, quit };
}
