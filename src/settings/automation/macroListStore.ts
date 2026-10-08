import { listMacros, subscribeMacrosChanged, type Macro } from '../../ipc/automation';
import { createConfigStore } from '../../stores/config/configStore';
import { getShownProfile, isShownHeld, subscribeShownMoves } from '../shownProfile';

// Every macro the profile Settings shows holds, yours and the ones
// presets add. The card of a macro preset under Presets and the rings on
// Macros read it to mark the keys one of your macros keeps, so they
// speak of the profile you edit, not the one in front while a page
// holds another. It loads on first use, reads again when Settings moves
// to another profile, and takes each list the backend sends after a
// change, which names the profile in front. While Settings holds
// another, that list is not the one it shows, so it waits.

const store = createConfigStore<Macro[]>({
  initial: [],
  read: () => listMacros(getShownProfile()),
  follow: (put) =>
    subscribeMacrosChanged((macros) => {
      if (!isShownHeld()) put(macros);
    }),
  moved: subscribeShownMoves,
});

export const useMacroList = store.use;

/** The store itself, for its test. */
export { store };
