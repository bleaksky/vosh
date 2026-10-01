import { useSyncExternalStore } from 'react';
import {
  NO_CHAT_COLORS,
  normalizeChatColors,
  sameChatColors,
  type ChatColors,
} from '../chatColors';
import {
  getChatColorsTable,
  subscribeChatColorsChanged,
  subscribeProfileSwitched,
} from '../session';
import { createStore } from './store';

// The active profile's chat channel colors for the chat pane and its
// menu, picked under Channel colors. Seeded from ui_get_chat_colors,
// kept live by vosh://chat-colors-changed (a pick or a reset from the
// menu, the broadcast after a profile switch), and read again on
// vosh://profile-switched in case the switch lands without one.

const store = createStore<ChatColors>(NO_CHAT_COLORS);
let started = false;
// Bumped by every event. A read applies only when no event arrived
// after it started, so a slow read for the old profile cannot
// overwrite a pick made since.
let generation = 0;

/** Keep the current snapshot when nothing in it moved, so the pane does
 *  not render again. */
function put(next: ChatColors): void {
  if (sameChatColors(store.get(), next)) return;
  store.set(next);
}

function refetch(): void {
  const mine = ++generation;
  getChatColorsTable()
    .then((table) => {
      if (mine === generation) put(normalizeChatColors(table));
    })
    .catch(() => undefined);
}

export function startChatColorsStore(): void {
  if (started) return;
  started = true;
  refetch();
  void subscribeChatColorsChanged((table) => {
    generation += 1;
    put(normalizeChatColors(table));
  });
  void subscribeProfileSwitched(() => refetch());
}

export function getChatColors(): ChatColors {
  return store.get();
}

export function subscribeChatColors(cb: () => void): () => void {
  startChatColorsStore();
  return store.subscribe(cb);
}

export function useChatColors(): ChatColors {
  return useSyncExternalStore(subscribeChatColors, getChatColors);
}
