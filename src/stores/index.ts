import { startChatStore } from './gmcp/chatStore';
import { startGroupStore } from './gmcp/groupStore';
import { startImmStore } from './gmcp/immStore';
import { startAffectFullStore } from './gmcp/affectFullStore';
import { startAffectsDisplayStore } from './config/affectsDisplayStore';
import { startAffectsStore } from './gmcp/affectsStore';
import { startChatColorsStore } from './config/chatColorsStore';
import { startChipStyleStore } from './config/chipStyleStore';
import { startCombatStore } from './gmcp/combatStore';
import { startConnectionStore } from './session/connectionStore';
import { startGameTimeStore } from './config/gameTimeStore';
import { startGamePromptStore } from './gmcp/gamePromptStore';
import { startHiddenStore } from './gmcp/hiddenStore';
import { startInputModeStore } from './session/inputModeStore';
import { startLuaPanesStore } from './session/luaPanesStore';
import { startPinnedPromptStore } from './session/pinnedPromptStore';
import { startPluginRowsStore } from './session/pluginRowsStore';
import { startRoomStore } from './gmcp/roomStore';
import { startSessionRowStore } from './session/sessionRowStore';
import { startSessionsStore } from './session/sessionsStore';
import { startTargetStore } from './session/targetStore';
import { startTickCountStore } from './config/tickCountStore';
import { startTickStore } from './session/tickStore';
import { startTrackedAffectsStore } from './config/trackedAffectsStore';
import { startVitalsDensityStore } from './config/vitalsDensityStore';
import { startVitalsOptionsStore } from './config/vitalsOptionsStore';
import { startVitalsStore } from './gmcp/vitalsStore';
import { startWorldStore } from './gmcp/worldStore';

// Start every pane and status line store once, at launch, so packages
// that arrive before a pane first renders still land. Several arrive
// only at login (World.Moons, Imm.Queues, Char.Prompt) or when you move
// (Room.*).
// Each start is idempotent, and each store also starts itself on its
// first subscribe as a fallback.
export function startStores(): void {
  // The sessions first, so every store that shows the selected one
  // finds the list on its way.
  startSessionsStore();
  startConnectionStore();
  startSessionRowStore();
  // Then the hidden state, so every store that ORs it in finds it
  // listening.
  startHiddenStore();
  startChatStore();
  startChatColorsStore();
  startGroupStore();
  startImmStore();
  startVitalsStore();
  startVitalsDensityStore();
  startVitalsOptionsStore();
  startAffectsStore();
  startAffectsDisplayStore();
  startAffectFullStore();
  startTrackedAffectsStore();
  startCombatStore();
  startGamePromptStore();
  startWorldStore();
  startRoomStore();
  startTargetStore();
  startTickStore();
  startTickCountStore();
  startGameTimeStore();
  startChipStyleStore();
  startPinnedPromptStore();
  startInputModeStore();
  startLuaPanesStore();
  startPluginRowsStore();
}
