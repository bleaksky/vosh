import { startChatStore } from '../chatStore';
import { startGroupStore } from '../groupStore';
import { startImmStore } from '../immStore';
import { startAffectsStore } from './affectsStore';
import { startChipStyleStore } from './chipStyleStore';
import { startCombatStore } from './combatStore';
import { startRoomStore } from './roomStore';
import { startTargetStore } from './targetStore';
import { startTickStore } from './tickStore';
import { startTrackedAffectsStore } from './trackedAffectsStore';
import { startVitalsDensityStore } from './vitalsDensityStore';
import { startVitalsStore } from './vitalsStore';
import { startWorldStore } from './worldStore';

// Start every pane and status line store once, at launch, so packages
// that arrive before a pane first renders still land. Several arrive
// only at login (World.Moons, Imm.Queues) or when you move (Room.*).
// Each start is idempotent, and each store also starts itself on its
// first subscribe as a fallback.
export function startStores(): void {
  startChatStore();
  startGroupStore();
  startImmStore();
  startVitalsStore();
  startVitalsDensityStore();
  startAffectsStore();
  startTrackedAffectsStore();
  startCombatStore();
  startWorldStore();
  startRoomStore();
  startTargetStore();
  startTickStore();
  startChipStyleStore();
}
