import type { WritingCharacter, WritingState } from '../ipc/writing';
import { connectionOf } from '../stores/session/connectionStore';
import { getSessions } from '../stores/session/sessionsStore';
import { subscribeWritingOf, writingOf } from '../stores/session/writingStore';
import {
  characterOf,
  getWritingFile,
  keepCharacter,
  loadWriting,
  withCheckWaiting,
} from './draftsStore';

// Forgets a check that waits once the game says the immortals decided
// it, in any session, with the card open or not. The name comes from
// the session's connection and the world from its row, as the card
// finds them, and the change goes through keepCharacter, so it shares
// the card's store and its write to writing.toml.

/** `character` without the check `decided` names, when that decision
 *  is new since `seen` and the check still waits. Null when nothing
 *  changes. */
export function clearedBy(
  decided: WritingState['decided'],
  seen: number | undefined,
  character: WritingCharacter,
): WritingCharacter | null {
  if (!decided || decided.id === seen) return null;
  const next = withCheckWaiting(character, decided.kind, false);
  return next === character ? null : next;
}

/** The last decision heard in each session. */
const seen = new Map<number, number>();
let started = false;

export function startCheckWatch(): void {
  if (started) return;
  started = true;
  subscribeWritingOf((session) => {
    const decided = writingOf(session).decided;
    const before = seen.get(session);
    if (!decided || decided.id === before) return;
    seen.set(session, decided.id);
    const name = connectionOf(session).character;
    const row = getSessions().find((r) => r.id === session);
    if (!name || !row?.host || !row.port) return;
    const world = { host: row.host, port: row.port };
    void loadWriting().then(() => {
      const next = clearedBy(decided, before, characterOf(getWritingFile(), world, name));
      if (next) keepCharacter(next);
    });
  });
}
