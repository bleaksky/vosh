import { startChatStore } from '../chatStore';
import { startGroupStore } from '../groupStore';
import { startImmStore } from '../immStore';
import { startAffectFullStore } from './affectFullStore';
import { startAffectsDisplayStore } from './affectsDisplayStore';
import { startAffectsStore } from './affectsStore';
import { startChatColorsStore } from './chatColorsStore';
import { startChipStyleStore } from './chipStyleStore';
import { startCombatStore } from './combatStore';
import { startGameTimeStore } from './gameTimeStore';
import { startGamePromptStore } from './gamePromptStore';
import { startHiddenStore } from './hiddenStore';
import { startPinnedPromptStore } from './pinnedPromptStore';
import { startRoomStore } from './roomStore';
import { startTargetStore } from './targetStore';
import { startTickCountStore } from './tickCountStore';
import { startTickStore } from './tickStore';
import { startTrackedAffectsStore } from './trackedAffectsStore';
import { startVitalsDensityStore } from './vitalsDensityStore';
import { startVitalsOptionsStore } from './vitalsOptionsStore';
import { startVitalsStore } from './vitalsStore';
import { startWorldStore } from './worldStore';

// Start every pane and status line store once, at launch, so packages
// that arrive before a pane first renders still land. Several arrive
// only at login (World.Moons, Imm.Queues, Char.Prompt) or when you move
// (Room.*).
// Each start is idempotent, and each store also starts itself on its
// first subscribe as a fallback.
export function startStores(): void {
  // First, so every store that ORs in the hidden state finds it
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
}
