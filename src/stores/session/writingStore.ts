import { onWriting, type WritingState } from '../../ipc/writing';
import { createSessionStore } from '../sessionStore';

// Where each session's writer stands, from session://writing: where the
// game takes your input, the text its editor holds while Vosh can name
// it, the card's offer, the job under way, the sends that wait for it,
// and how the last job ended. The writing card, the offer notice and the
// command line read the selected session's. The writer sends its state
// as each pass of the session loop ends with it changed, and a
// disconnect puts it back.

export const WRITING_IDLE: WritingState = {
  game: 'unknown',
  editor: null,
  offer: null,
  job: null,
  held: 0,
  done: null,
};

const store = createSessionStore<WritingState>({
  state: WRITING_IDLE,
  events: [(apply) => onWriting((state, session) => apply(session, () => state))],
});

export const startWritingStore = store.start;
export const getWriting = store.get;
export const subscribeWriting = store.subscribe;
export const useWriting = store.use;
export const writingOf = store.stateOf;
