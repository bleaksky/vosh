import { onInputMode } from '../../ipc/session';
import { createSessionStore } from '../sessionStore';

// Whether the game turned its echo off in a session, as it does while
// you type a password, so the command line shows the masked field. Each
// session keeps its own, and the command line shows the selected
// session's. The session sends false as it disconnects, and the factory
// puts back false on the disconnect too.

const store = createSessionStore<boolean>({
  state: false,
  events: [(apply) => onInputMode(({ password }, session) => apply(session, () => password))],
});

export const startInputModeStore = store.start;
export const getPasswordMode = store.get;
export const subscribePasswordMode = store.subscribe;
