import { useCallback, useEffect, useRef, useState } from 'react';
import type { SnoopFindResults, SnoopTerminalHandle } from '../terminal/SnoopTerminal';
import type { FindOptions } from '../terminal/terminalHandle';

// The snoop terminals of the split or the snoop window, and Find on the
// tab in front (Snoop SN2 and SN3). Each terminal hands its handle over
// as it sets up, and the find bar searches the one in front with the
// search your terminal uses. The bar closes when another tab comes to
// the front.

/** The find bar before a search runs. */
const NO_RESULTS: SnoopFindResults = { index: -1, count: 0 };

export function useSnoopFind(session: number, selected: string | null) {
  const handles = useRef(new Map<string, SnoopTerminalHandle>());
  const [finding, setFinding] = useState(false);
  const [results, setResults] = useState<SnoopFindResults>(NO_RESULTS);

  useEffect(() => {
    setFinding(false);
    setResults(NO_RESULTS);
  }, [selected, session]);

  const open = useCallback(() => setFinding(true), []);

  /** The handle of the tab in front. */
  const front = () => (selected ? handles.current.get(selected) : undefined);

  return {
    front,
    finding,
    results,
    open,
    close: () => {
      front()?.clearSearch();
      setFinding(false);
      setResults(NO_RESULTS);
    },
    find: (query: string, direction: 'next' | 'previous', options: FindOptions) => {
      const handle = front();
      if (!handle) return false;
      return direction === 'next'
        ? handle.findNext(query, options)
        : handle.findPrevious(query, options);
    },
    /** What the terminal of `name` hands back: its handle and the
     *  matches of a find, which count while it is in front. */
    terminal: (name: string) => ({
      onReady: (handle: SnoopTerminalHandle | null) => {
        if (handle) handles.current.set(name, handle);
        else handles.current.delete(name);
      },
      onFindResults: (found: SnoopFindResults) => {
        if (name === selected) setResults(found);
      },
    }),
  };
}
