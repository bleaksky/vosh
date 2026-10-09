import { asNumber, asText } from '../store';
import { createSessionStore } from '../sessionStore';

// Your race and level, from Char.Status, which the game sends at login
// and as your level changes (gmcp.c:200). The writing card reads them
// for a werebeast's Beast switch, the staff boards an immortal writes,
// and the codes the game keeps in a line from trust 55. A disconnect
// puts them back.

export interface CharStatus {
  race: string | null;
  level: number | null;
}

const NONE: CharStatus = { race: null, level: null };

function take(now: CharStatus, data: unknown): CharStatus {
  const d = data && typeof data === 'object' ? (data as Record<string, unknown>) : {};
  const race = asText(d.race) ?? now.race;
  const level = asNumber(d.level) ?? now.level;
  return race === now.race && level === now.level ? now : { race, level };
}

const store = createSessionStore<CharStatus>({
  state: NONE,
  packages: { 'Char.Status': take },
});

export const startCharStatusStore = store.start;
export const getCharStatus = store.get;
export const useCharStatus = store.use;
