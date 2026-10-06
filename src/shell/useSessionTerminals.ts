// The live terminal of each session the main window opened, and what
// the window writes into them. Each session keeps a terminal of its own
// until it closes, and the selected session's is the one the find bar,
// the split, the menus and the prompt card reach.
//
// The window's word on each session's link lives here too. The
// connection store keeps where each session stands, the toasts speak
// for the selected session only, a session behind shows its drop in its
// own terminal and on its row, and each connect tells the game the size
// of the pane it will show in.

import { useLayoutEffect, useRef, type MutableRefObject } from 'react';
import {
  onReconnect,
  onState,
  setWindowSize,
  type ReconnectPayload,
  type StatePayload,
} from '../ipc/session';
import { terminalLocalWrite } from '../ipc/terminal';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { noteConnectionError } from '../stores/session/connectionStore';
import { getSelected, getSessions, othersOnProfile } from '../stores/session/sessionsStore';
import { dismissToast, pushToast } from '../stores/toasts';
import type { TerminalHandle } from '../terminal/terminalHandle';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { showLaunchNotices } from './launchNotices';

export interface SessionTerminals {
  /** The selected session's live terminal, once it is ready. */
  termRef: MutableRefObject<TerminalHandle | null>;
  /** Write text the page draws itself into the terminal of `session`. */
  writeTo: (session: number, text: string) => void;
  /** Write text the page draws itself into the selected session's
   *  terminal. */
  writeLive: (text: string) => void;
  /** An action failed, a connect, a send or a disconnect, in `session`,
   *  the selected one when none is named. */
  handleError: (message: string, session?: number) => void;
  /** What the live terminal of `session` calls once it is ready. */
  onTerminalReady: (session: number) => (handle: TerminalHandle) => void;
  /** What the live terminal of `session` calls once its scrollback
   *  loaded, in the session launch selected, which takes what launch
   *  has to tell you. */
  onScrollbackLoaded: (session: number) => (() => void) | undefined;
}

/** Each step of a session's redial, with the session it is. */
const onRedial = (cb: (payload: ReconnectPayload & { session: number }) => void) =>
  onReconnect((payload, session) => cb({ ...payload, session }));

/** Why Vosh will not redial `session` after a drop, for the toast that
 *  says so. */
function declinedWhy(why: 'quit' | 'banned' | 'taken', session: number): string {
  if (why === 'quit') return 'you quit';
  if (why === 'banned') return 'the game banned this account';
  const character = getSessions().find((row) => row.id === session)?.character;
  return `another session took ${character ?? 'it'}`;
}

/** The live terminals of the sessions in `opened`, with `selected` the
 *  one that shows. Mount it once, in MainWindow. */
export function useSessionTerminals(selected: number, opened: readonly number[]): SessionTerminals {
  const terminals = useRef(new Map<number, TerminalHandle>());
  const termRef = useRef<TerminalHandle | null>(null);
  useLayoutEffect(() => {
    for (const id of terminals.current.keys()) {
      if (!opened.includes(id)) terminals.current.delete(id);
    }
    termRef.current = terminals.current.get(selected) ?? null;
  }, [selected, opened]);
  // The session launch selected, which takes what launch has to tell you.
  const launchSession = useRef<number | null>(null);
  launchSession.current ??= opened[0] ?? null;

  // Write text the page draws itself (your typed echo, error notices) to
  // the xterm of `session`, and through terminal_local_write to its
  // native grid and the session on either renderer. The session closes
  // the open row, since the text now follows it, so it never repaints
  // over your echo. Your line goes out by its own call, so the session
  // can hear of the echo after the reply. It closes only the rows that
  // came before the newest output the renderer that shows took: xterm
  // names it here, and the native grid names its own as it takes the
  // text, as it does for a session with no terminal here yet.
  const writeTo = (session: number, text: string) => {
    const term = terminals.current.get(session);
    term?.write(text);
    const after = nativeSurfaceEnabled() || !term ? null : term.outputTaken();
    void terminalLocalWrite(text, after, session).catch(() => {});
  };
  const writeLive = (text: string) => writeTo(getSelected(), text);
  // The session it was for shows the error in its title band and its
  // terminal.
  const handleError = (message: string, session = getSelected()) => {
    noteConnectionError(session, message);
    writeTo(session, `\r\n\x1b[31m[${message}]\x1b[0m\r\n`);
  };

  // The Connection lost toast of the selected session's last drop, which
  // the reconnect notice takes the place of, or a toast that says Vosh
  // will not redial.
  const lostToast = useRef<number | null>(null);
  const dropLostToast = () => {
    if (lostToast.current !== null) dismissToast(lostToast.current);
    lostToast.current = null;
  };

  useTauriEvent(onState, (payload: StatePayload) => {
    const shown = payload.session === getSelected();
    if (payload.kind === 'disconnected') {
      // A reason means the link dropped out from under us; a clean
      // user-initiated disconnect carries none and stays quiet. The
      // reason goes into the terminal of the session that dropped.
      if (payload.reason) {
        writeTo(payload.session, `\r\n\x1b[31m[${payload.reason}]\x1b[0m\r\n`);
        if (shown) {
          lostToast.current = pushToast({
            kind: 'error',
            message: 'Connection lost',
            meta: payload.reason,
          });
        }
      }
    } else if (payload.kind === 'connected') {
      if (shown) {
        pushToast({
          kind: 'success',
          message: 'Connected',
          meta: `${payload.host}:${payload.port}`,
        });
      }
      // Push the current terminal size on every (re)connect so the
      // negotiator advertises the live cols × rows via NAWS as soon
      // as the server asks. MUDs that honor NAWS wrap at this width
      // server-side, which is the right answer to word wrap. A session
      // behind on the selected session's profile shares the pane, so it
      // takes the same size. One on another profile hears its own as its
      // pane shows.
      const handle = termRef.current;
      if (handle && (shown || othersOnProfile(getSelected()).includes(payload.session))) {
        const { cols, rows } = handle.windowSize();
        void setWindowSize(cols, rows, payload.session).catch(() => {});
      }
    }
  });

  // The first wait of a redial puts the notice where the drop's toast
  // stood. A drop Vosh will not redial says why, unless Reconnect is off,
  // where Connection lost says enough.
  useTauriEvent(onRedial, (payload) => {
    if (payload.session !== getSelected()) return;
    if (payload.kind === 'waiting' && payload.try === 1) dropLostToast();
    if (payload.kind === 'declined' && payload.why !== 'off') {
      dropLostToast();
      pushToast({
        kind: 'error',
        message: 'Vosh will not reconnect',
        meta: declinedWhy(payload.why, payload.session),
      });
    }
  });

  const onTerminalReady = (session: number) => (handle: TerminalHandle) => {
    terminals.current.set(session, handle);
    if (session === getSelected()) termRef.current = handle;
  };
  // After the restored scrollback, so what launch has to tell you lands
  // below it instead of scrolling away above, in the session launch
  // selected.
  const onScrollbackLoaded = (session: number) =>
    session === launchSession.current
      ? () => void showLaunchNotices((text) => writeTo(session, text))
      : undefined;

  return { termRef, writeTo, writeLive, handleError, onTerminalReady, onScrollbackLoaded };
}
