// The typed commands Up and Down recall into the command line, with the
// prefix search over them.

import { useState } from 'react';

/** The typed history of the command line, which holds `value`, and the
 *  prefix search over it. Recalled lines go through `setValue`. Input
 *  calls remember for each submitted line, recallOlder on Up and
 *  recallNewer on Down when the caret has no line left to move to, and
 *  resetSearch when an edit, a submit or a mask change ends the search. */
export function useCommandHistory(value: string, setValue: (next: string) => void) {
  const [history, setHistory] = useState<string[]>([]);
  // When the user starts arrow-key navigation with non-empty input, we
  // remember that prefix so Up and Down cycle only matching history entries.
  // Null means no active prefix search; cycle the full history.
  const [searchPrefix, setSearchPrefix] = useState<string | null>(null);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);

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

  /** Add a submitted line, unless it repeats the newest one. */
  const remember = (line: string) => {
    setHistory((prev) => {
      if (prev[prev.length - 1] === line) return prev;
      return [...prev, line];
    });
  };

  const resetSearch = () => {
    setSearchPrefix(null);
    setHistoryIndex(null);
  };

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
