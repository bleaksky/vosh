import { onPaced } from '../../ipc/session';
import { createSessionStore } from '../sessionStore';

// How many lines the pace holds back in each session, from
// session://paced. The command line shows the selected session's count,
// and a click on it drops them. The session loop sends the count as each
// pass ends with it changed, and a disconnect puts it back to 0.

const store = createSessionStore<number>({
  state: 0,
  events: [(apply) => onPaced((waiting, session) => apply(session, () => waiting))],
});

export const startPacedStore = store.start;
export const usePaced = store.use;
