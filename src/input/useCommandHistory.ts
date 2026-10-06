// The typed commands Up and Down recall into the command line, with the
// prefix search over them, and the line you were composing, for each
// session.

import { useLayoutEffect, useRef, useState } from 'react';

/** No history yet. */
const NONE: readonly string[] = [];

/** The typed history of the command line, which holds `value`, and the
 *  prefix search over it, for the selected session `session`. Recalled
 *  lines go through `setValue`. Input calls remember for each submitted
 *  line, with the session it went to, recallOlder on Up and recallNewer
 *  on Down when the caret has no line left to move to, and resetSearch
 *  when an edit, a submit or a mask change ends the search.
 *
 *  Each session keeps its own history and its own draft. As the
 *  selection moves, the line you were composing stays with the session
 *  you leave, and the command line takes the one the next session holds,
 *  before the page paints. */
export function useCommandHistory(
  value: string,
  setValue: (next: string) => void,
  session: number,
) {
  const [histories, setHistories] = useState<ReadonlyMap<number, readonly string[]>>(
    () => new Map(),
  );
  const history = histories.get(session) ?? NONE;
  // When the user starts arrow-key navigation with non-empty input, we
  // remember that prefix so Up and Down cycle only matching history entries.
  // Null means no active prefix search; cycle the full history.
  const [searchPrefix, setSearchPrefix] = useState<string | null>(null);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);

  // The drafts of the sessions behind, and the session whose draft the
  // command line holds.
  const drafts = useRef(new Map<number, string>());
  const shown = useRef(session);
  const valueRef = useRef(value);
  valueRef.current = value;

  const matchingIndices = (prefix: string | null): number[] => {
    if (prefix === null || prefix === '') {
      return history.map((_, i) => i);
    }
    return history.flatMap((line, i) => (line.startsWith(prefix) ? [i] : []));
  };

  const startSearchIfNeeded = (): number[] => {
    if (searchPrefix === null) {
      const prefix = value;
      setSearchPrefix(prefix);
      return matchingIndices(prefix);
    }
    return matchingIndices(searchPrefix);
  };

  /** Add a line submitted to `to`, unless it repeats its newest one. */
  const remember = (line: string, to: number) => {
    setHistories((now) => {
      const lines = now.get(to) ?? NONE;
      if (lines[lines.length - 1] === line) return now;
      return new Map(now).set(to, [...lines, line]);
    });
  };

  const resetSearch = () => {
    setSearchPrefix(null);
    setHistoryIndex(null);
  };

  useLayoutEffect(() => {
    const left = shown.current;
    if (left === session) return;
    drafts.current.set(left, valueRef.current);
    shown.current = session;
    setValue(drafts.current.get(session) ?? '');
    drafts.current.delete(session);
    setSearchPrefix(null);
    setHistoryIndex(null);
  }, [session, setValue]);

  const recallOlder = () => {
    const matches = startSearchIfNeeded();
    if (matches.length === 0) return;
    const currentMatchPos = historyIndex === null ? matches.length : matches.indexOf(historyIndex);
    const nextPos = Math.max(0, currentMatchPos - 1);
    const next = matches[nextPos];
    if (next === undefined) return;
    setHistoryIndex(next);
    setValue(history[next] ?? '');
  };

  const recallNewer = () => {
    if (historyIndex === null) return;
    const matches = matchingIndices(searchPrefix);
    const currentMatchPos = matches.indexOf(historyIndex);
    const nextPos = currentMatchPos + 1;
    if (nextPos >= matches.length) {
      setHistoryIndex(null);
      setValue(searchPrefix ?? '');
    } else {
      const next = matches[nextPos];
      if (next === undefined) return;
      setHistoryIndex(next);
      setValue(history[next] ?? '');
    }
  };

  return { history, searchPrefix, remember, resetSearch, recallOlder, recallNewer };
}
