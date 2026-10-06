import { useMemo } from 'react';
import type { SessionRow } from '../ipc/session';
import { sessionLabel } from '../lib/sessionLabel';
import { rowLook, waitingWords } from '../stores/session/sessionRowStore';
import { howLong, useSessionView, type SessionView } from './sessionLine';

// What the card beside a session's row says, S6 of the Sessions Sidebar
// review, which SessionCard draws and the row reads to a screen reader
// as its description. Each fact drops out while the session has nothing
// for it.

/** What the foot of every card says. */
export const CARD_FOOT = 'Double click the name to rename';

export interface CardFact {
  label: string;
  value: string;
  /** The value takes the danger tone, as low health does. */
  low: boolean;
}

/** What a session's card says. */
export interface CardFacts {
  /** The head, the row's name with its port in quiet meta. */
  name: string;
  port: string | null;
  /** The world with the profile, like `The Forsaken Lands 1825, profile
   *  Build`, null with neither. */
  where: string | null;
  facts: CardFact[];
}

const fact = (label: string, value: string | null | undefined, low = false): CardFact[] =>
  value ? [{ label, value, low }] : [];

/** `now` over `max` as the card shows a vital, none without a max. */
const vital = (now: number, max: number) => (max > 0 ? `${now} / ${max}` : null);

/** What the card of `row` says, among the open sessions in `rows`. */
export function cardFacts(
  row: SessionRow,
  rows: readonly SessionRow[],
  { state, room, combat, vitals, now }: SessionView,
): CardFacts {
  const label = sessionLabel(row, rows);
  const character = row.character?.trim() || null;
  // A session you named keeps its character on the card.
  const who = character !== label.name ? character : null;
  const world = who && label.place ? `${who} on ${label.place}` : (label.place ?? who);
  const profile = row.profile && (world ? `profile ${row.profile}` : `Profile ${row.profile}`);
  const where = [world, profile].filter(Boolean).join(', ') || null;

  const mark = rowLook(state, row, false).mark;
  const facts: CardFact[] = [];
  if (mark === 'live' || mark === 'hand') {
    const shows = vitals !== null && !vitals.hidden;
    facts.push(
      ...fact('Room', room?.name),
      ...fact('Area', room?.area),
      ...fact('Fighting', combat?.name),
      ...(shows ? fact('Health', vital(vitals.hp, vitals.maxhp), vitals.low.hp) : []),
      ...(shows ? fact('Mana', vital(vitals.mana, vitals.maxmana)) : []),
      ...(shows ? fact('Moves', vital(vitals.move, vitals.maxmove)) : []),
      ...fact('Online', row.since === null ? null : (howLong(now - row.since) ?? 'Under a minute')),
    );
  } else if (state.downAt !== null) {
    const ago = howLong(now - state.downAt);
    facts.push(...fact('Played', ago ? `${ago} ago` : 'Just now'));
  }
  facts.push(...fact('Waiting', waitingWords(state.waiting)));
  return {
    name: label.split?.world ?? label.name,
    port: label.split?.port ?? label.meta,
    where,
    facts,
  };
}

/** The card's text in sentences, the row's description for a screen
 *  reader, which already reads the name. */
export function cardWords({ where, facts }: CardFacts): string {
  return [where, ...facts.map((f) => `${f.label} ${f.value}`), CARD_FOOT]
    .filter(Boolean)
    .join('. ')
    .concat('.');
}

/** What the card of `row` says, which follows its session as it plays. */
export function useCardFacts(row: SessionRow, rows: readonly SessionRow[]): CardFacts {
  const view = useSessionView(row.id);
  return useMemo(() => cardFacts(row, rows, view), [row, rows, view]);
}
