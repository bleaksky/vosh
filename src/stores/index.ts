import { startCharStatusStore } from './gmcp/charStatusStore';
import { startChatStore } from './gmcp/chatStore';
import { startGroupStore } from './gmcp/groupStore';
import { startImmStore } from './gmcp/immStore';
import { startAffectFullStore } from './gmcp/affectFullStore';
import { startAffectsDisplayStore } from './config/affectsDisplayStore';
import { startAlertNoticeStore } from './session/alertNoticeStore';
import { startAffectsStore } from './gmcp/affectsStore';
import { startChatColorsStore } from './config/chatColorsStore';
import { startChipStyleStore } from './config/chipStyleStore';
import { startCombatStore } from './gmcp/combatStore';
import { startEchoMarkStore } from './config/echoMarkStore';
import { startLineLookStore } from './config/lineLookStore';
import { startLineMarkStore } from './config/lineMarkStore';
import { startTypeColorsStore } from './config/typeColorsStore';
import { startKnownWordsStore } from './session/knownWordsStore';
import { startConnectionStore } from './session/connectionStore';
import { startGameTimeStore } from './config/gameTimeStore';
import { startGamePromptStore } from './gmcp/gamePromptStore';
import { startHiddenStore } from './gmcp/hiddenStore';
import { startInputModeStore } from './session/inputModeStore';
import { startLuaPanesStore } from './session/luaPanesStore';
import { startPinnedPromptStore } from './session/pinnedPromptStore';
import { startPluginRowsStore } from './session/pluginRowsStore';
import { startReconnectStore } from './session/reconnectStore';
import { startRoomStore } from './gmcp/roomStore';
import { startReaderStore } from './session/readerStore';
import { startRoundTripStore } from './session/roundTripStore';
import { startScreenReaderStore } from './config/screenReaderStore';
import { startSessionRowStore } from './session/sessionRowStore';
import { startSessionsStore } from './session/sessionsStore';
import { startSnoopStore } from './session/snoopStore';
import { startTargetStore } from './session/targetStore';
import { startTickCountStore } from './config/tickCountStore';
import { startTickStore } from './session/tickStore';
import { startTrackedAffectsStore } from './config/trackedAffectsStore';
import { startVitalsOptionsStore } from './config/vitalsOptionsStore';
import { startVitalsStore } from './gmcp/vitalsStore';
import { startVitalsTextStore } from './session/vitalsTextStore';
import { startWalkStore } from './session/walkStore';
import { startWritingStore } from './session/writingStore';
import { startWorldStore } from './gmcp/worldStore';
import { startCheckWatch } from '../writing/checkWatch';

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
  startReconnectStore();
  startAlertNoticeStore();
  // Then the hidden state, so every store that ORs it in finds it
  // listening.
  startHiddenStore();
  startChatStore();
  startChatColorsStore();
  startGroupStore();
  startImmStore();
  startVitalsStore();
  startVitalsOptionsStore();
  startVitalsTextStore();
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
  startRoundTripStore();
  startTickCountStore();
  startGameTimeStore();
  startChipStyleStore();
  startEchoMarkStore();
  startLineMarkStore();
  startLineLookStore();
  startTypeColorsStore();
  startKnownWordsStore();
  startPinnedPromptStore();
  startInputModeStore();
  startLuaPanesStore();
  startPluginRowsStore();
  startSnoopStore();
  startWalkStore();
  startCharStatusStore();
  startWritingStore();
  startCheckWatch();
  startScreenReaderStore();
  startReaderStore();
}
