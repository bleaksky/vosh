// Tab completion on the command line, from typed history, the characters
// in the room and the names seen in the output, all of the selected
// session.

import { useEffect, useRef, type RefObject } from 'react';
import { getRoom } from '../stores/gmcp/roomStore';
import { recentNames } from './recentNames';

/** Complete the word under the caret in `inputRef`, which holds `value`,
 *  and write each completion through `setValue`. `history` holds the
 *  typed commands of `session`, the selected session, oldest first.
 *  Input calls complete on Tab, with -1 for Shift+Tab, and resetCycle on
 *  every other key and edit and on each selection. complete answers
 *  whether it took the key. It leaves it, so the focus moves on (Q22),
 *  only while no cycle runs and the line is blank. */
export function useTabCompletion(
  inputRef: RefObject<HTMLInputElement | HTMLTextAreaElement>,
  value: string,
  setValue: (next: string) => void,
  history: readonly string[],
  session: number,
) {
  // Tab-completion cycling state. When the user presses Tab we
  // resolve the word being typed, build a candidate list, and
  // remember the cycle so consecutive Tab presses walk through the
  // matches. Any input change other than Tab resets this so the next
  // Tab starts a fresh search.
  const tabStateRef = useRef<{
    wordStart: number;
    matches: string[];
    idx: number;
    suffixOffset: number;
  } | null>(null);

  // Build the list of completion candidates ordered by source priority:
  //   1. Unique words pulled from typed-command history, most recent first.
  //   2. Room-character names from the latest Room.Chars GMCP push.
  //   3. Capitalized name-like tokens seen anywhere in MUD output in
  //      the last 30 minutes (who-list names, comm-channel speakers,
  //      consider targets, etc.). Populated by Terminal.tsx via
  //      ingestRecentNames().
  // Filter by case-insensitive prefix and deduplicate so the user does
  // not see the same word twice when a noun also appeared in history.
  const buildTabMatches = (prefix: string): string[] => {
    const lower = prefix.toLowerCase();
    const seen = new Set<string>();
    const matches: string[] = [];
    const consider = (word: string) => {
      if (word.length === 0) return;
      if (word.toLowerCase() === lower) return;
      if (!word.toLowerCase().startsWith(lower)) return;
      const key = word.toLowerCase();
      if (seen.has(key)) return;
      seen.add(key);
      matches.push(word);
    };
    for (let i = history.length - 1; i >= 0; i--) {
      for (const token of history[i].split(/\s+/)) {
        consider(token);
      }
    }
    // The people in the session's room, from its Room.Chars, so you can
    // complete a combat target without typing the whole name.
    for (const person of getRoom().people) {
      consider(person.name);
    }
    for (const name of recentNames(session)) {
      consider(name);
    }
    return matches;
  };

  const complete = (step: number): boolean => {
    const el = inputRef.current;
    if (!el) return false;
    const caret = el.selectionStart ?? value.length;
    const state = tabStateRef.current;
    // Only a blank line lets the key go. A Tab mid-line stays here.
    if (!state && value.trim() === '') return false;
    if (state) {
      // Cycle within the existing match set.
      if (state.matches.length === 0) return true;
      const next = (state.idx + step + state.matches.length) % state.matches.length;
      const match = state.matches[next];
      const before = value.slice(0, state.wordStart);
      const after = value.slice(value.length - state.suffixOffset);
      const nextValue = before + match + after;
      setValue(nextValue);
      tabStateRef.current = { ...state, idx: next };
      // Move the caret to the end of the inserted match on the next
      // tick so React has committed the value update.
      requestAnimationFrame(() => {
        const e2 = inputRef.current;
        if (!e2) return;
        const pos = before.length + match.length;
        e2.setSelectionRange(pos, pos);
      });
      return true;
    }
    // Fresh completion. Walk back from caret to find the start of
    // the current word.
    let start = caret;
    while (start > 0 && /\S/.test(value[start - 1])) start -= 1;
    const prefix = value.slice(start, caret);
    if (prefix.length === 0) return true;
    const matches = buildTabMatches(prefix);
    if (matches.length === 0) return true;
    const idx = step >= 0 ? 0 : matches.length - 1;
    const match = matches[idx];
    const before = value.slice(0, start);
    const after = value.slice(caret);
    const nextValue = before + match + after;
    setValue(nextValue);
    tabStateRef.current = {
      wordStart: start,
      matches,
      idx,
      suffixOffset: after.length,
    };
    requestAnimationFrame(() => {
      const e2 = inputRef.current;
      if (!e2) return;
      const pos = before.length + match.length;
      e2.setSelectionRange(pos, pos);
    });
    return true;
  };

  const resetCycle = () => {
    tabStateRef.current = null;
  };
  // A cycle belongs to the draft it began in, which stays with its
  // session as the selection moves.
  useEffect(() => {
    tabStateRef.current = null;
  }, [session]);

  return { complete, resetCycle };
}
