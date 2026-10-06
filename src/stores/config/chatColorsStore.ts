import {
  NO_CHAT_COLORS,
  normalizeChatColors,
  sameChatColors,
  type ChatColors,
} from '../../panel/chat/chatColors';
import { getChatColorsTable, subscribeChatColorsChanged } from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// The active profile's chat channel colors for the chat pane and its
// menu, picked under Channel colors.

const store = createConfigStore<ChatColors>({
  initial: NO_CHAT_COLORS,
  read: () => getChatColorsTable().then(normalizeChatColors),
  follow: (cb) => subscribeChatColorsChanged((table) => cb(normalizeChatColors(table))),
  same: sameChatColors,
});

export const startChatColorsStore = store.start;
export const getChatColors = store.get;
export const useChatColors = store.use;
