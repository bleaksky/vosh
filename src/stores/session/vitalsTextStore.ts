import { onVitalsText, type VitalsText } from '../../ipc/vitals';
import { createSessionStore } from '../sessionStore';

// The last render of your vitals text for each session, for the Text
// style of the footer. The session sends one while a footer watches it
// (vitalsTextWatch), at once and whenever what it shows moves, and a
// disconnect clears it.

const store = createSessionStore<VitalsText | null>({
  state: null,
  events: [(apply) => onVitalsText((text) => apply(text.session, () => text))],
});

export const startVitalsTextStore = store.start;
export const useVitalsText = store.use;
