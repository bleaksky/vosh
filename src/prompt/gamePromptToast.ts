import { onGamePromptSeen, type GamePromptSeenPayload } from '../ipc/prompt';
import { getSelected } from '../stores/session/sessionsStore';
import { pushToast, type ToastInput } from '../stores/toasts';
import { listJoin } from '../lib/text';

// When the game tells Vosh a new prompt setting and your profile's
// capture takes it, Vosh says so once with the codes it now reads. When
// the new setting no longer shows a part your design reads and no
// package sends it either, Vosh says that once too, and the card and
// Settings ring that part. Only the session in front, the selected
// one, says so, since the toast names no session.

/** The toast for one report, or null when the capture took nothing. */
export function gamePromptToast(payload: GamePromptSeenPayload): ToastInput | null {
  if (!payload.applied || payload.kind === 'off') return null;
  return {
    kind: 'info',
    message: 'Vosh reads your new prompt.',
    meta: payload.text,
    metaMono: true,
  };
}

/** How the sentence names a part your prompt fed, by its catalog name. */
const PARTS: Record<string, string> = {
  hp: 'your health',
  maxhp: 'your max health',
  mana: 'your mana',
  maxmana: 'your max mana',
  move: 'your moves',
  maxmove: 'your max moves',
  fight: 'that you are in a fight',
  tank: "your tank's name",
  tank_hp: "your tank's health",
  pos: 'your position',
  lang: 'your language',
  stallion: 'your stallion',
  wizi: 'your wizi level',
  incog: 'your incog level',
  afk: 'that you are away',
  gold: 'your gold',
  exp: 'your experience',
  tnl: 'your experience to the next level',
  cp: 'your cabal points',
  rp: 'your RP points',
  room: 'the room',
  room_num: 'the room number',
  area: 'the area',
  area_num: 'the area number',
  exits: 'your exits',
  region: 'the region',
  temp: 'the temperature',
  weather: 'the weather',
  hour: 'the game hour',
  olc: 'your OLC editor',
  olc_vnum: 'the vnum you edit',
  pacify: 'pacify',
};

function partName(name: string): string {
  const slot = /^slot(\d+)$/.exec(name);
  if (slot) return `affect slot ${slot[1]}`;
  if (/^moon\d$/.test(name)) return 'the moons';
  return PARTS[name] ?? name.replace(/_/g, ' ');
}

/** The toast that names the parts of your design your new prompt no
 *  longer feeds, or null when it still feeds every one. */
export function lostPartsToast(payload: GamePromptSeenPayload): ToastInput | null {
  if (!payload.applied || payload.lost.length === 0) return null;
  const parts = [...new Set(payload.lost.map(partName))];
  const one = payload.lost.length === 1;
  return {
    kind: 'info',
    message: `Your prompt no longer shows ${listJoin(parts)}, so ${
      one ? 'that part of your design stays' : 'those parts of your design stay'
    } blank.`,
  };
}

/** Show the toasts for every setting the capture of the selected
 *  session takes. Returns the function that stops listening. */
export async function startGamePromptToasts(): Promise<() => void> {
  return onGamePromptSeen((payload, session) => {
    if (session !== getSelected()) return;
    for (const toast of [gamePromptToast(payload), lostPartsToast(payload)]) {
      if (toast) pushToast(toast);
    }
  });
}
