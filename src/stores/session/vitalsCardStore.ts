import { useSyncExternalStore } from 'react';
import type { PromptPreviewName } from '../../ipc/promptDesign';
import { createStore } from '../store';

// What the vitals text card shows on the footer it edits: the part you
// picked, ringed, and the preview. A click on a part in the footer
// turns the card to it. The card sets it while it is open and the
// footer reads it. Null while the card is closed.

export interface VitalsCardMarks {
  /** The text the card edits, which the footer draws for a preview. */
  template: string;
  /** The part you picked, or null. */
  picked: number | null;
  preview: PromptPreviewName;
  /** Turn the card to a part you clicked in the footer. */
  pick: (piece: number) => void;
}

const store = createStore<VitalsCardMarks | null>(null);

export function setVitalsCardMarks(marks: VitalsCardMarks | null): void {
  store.set(marks);
}

export function useVitalsCardMarks(): VitalsCardMarks | null {
  return useSyncExternalStore(store.subscribe, store.get, store.get);
}
