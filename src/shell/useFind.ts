// Find in scrollback, from the toolbar that Mod+F, the palette and the
// terminal menu open. On the native surface the grid finds, scrolls to
// and highlights each match itself.

import { useRef, useState, type RefObject } from 'react';
import type { ISearchResultChangeEvent } from '@xterm/addon-search';
import { nativeSurfaceFind, nativeSurfaceFindClear } from '../ipc/nativeSurface';
import { useEscape } from '../lib/escapeStack';
import type { FindToolbarHandle } from '../terminal/FindToolbar';
import type { FindOptions, TerminalHandle } from '../terminal/terminalHandle';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import type { ScrollbackSplit } from './useScrollbackSplit';

interface FindPanes extends Pick<
  ScrollbackSplit,
  'showHistoryMatch' | 'hideHistoryMatch' | 'clearQueuedSearch'
> {
  /** The live pane, which owns iteration. */
  termRef: RefObject<TerminalHandle>;
  /** The history pane, while the split shows. */
  historyTermRef: RefObject<TerminalHandle>;
  /** Puts the caret on the command line. */
  focusInput: () => void;
}

interface ScrollbackFind {
  findOpen: boolean;
  openFind: () => void;
  findToolbarRef: RefObject<FindToolbarHandle>;
  /** The active match and the match count, for the toolbar badge. */
  findResults: { index: number; count: number };
  closeFind: () => void;
  /** Run a find from the toolbar. Returns whether it found a match. */
  submitFind: (query: string, opts: FindOptions, direction: 'next' | 'previous') => boolean;
  /** The live pane's onResultsChanged. */
  onFindResults: (event: ISearchResultChangeEvent) => void;
}

export function useFind({
  termRef,
  historyTermRef,
  focusInput,
  showHistoryMatch,
  hideHistoryMatch,
  clearQueuedSearch,
}: FindPanes): ScrollbackFind {
  // Scrollback find toolbar. Opens on Cmd+F (macOS) or Ctrl+F (other
  // platforms). Drives xterm's SearchAddon. The live pane always owns
  // iteration (selection + cached search term live on its addon, so
  // pressing Enter advances one match each time). The history pane
  // mirrors the search in parallel so a scrollback match has a
  // visible highlight up top while the live pane stays anchored to
  // its tail. A search whose match lands inside the live viewport
  // skips the split entirely.
  const [findOpen, setFindOpen] = useState(false);
  const findToolbarRef = useRef<FindToolbarHandle | null>(null);
  // Match count from live's SearchAddon. Drives the "3 / 12" badge in
  // the find toolbar. `index` of -1 means the active match was lost
  // (e.g. after the toolbar opened but before the first search ran).
  const [findResults, setFindResults] = useState<{ index: number; count: number }>({
    index: -1,
    count: 0,
  });

  const closeFind = () => {
    termRef.current?.clearSearch();
    historyTermRef.current?.clearSearch();
    if (nativeSurfaceEnabled()) {
      void nativeSurfaceFindClear().catch(() => {});
    }
    clearQueuedSearch();
    setFindResults({ index: -1, count: 0 });
    setFindOpen(false);
    focusInput();
  };

  // The find bar closes from anywhere, so a click that drifted focus
  // away (or the split auto-closing when history scrolled back to its
  // tail) still leaves Esc working.
  useEscape(findOpen, closeFind);

  // Run a find call from the toolbar. The live pane is the
  // authoritative iterator: each call advances its SearchAddon
  // selection + cachedSearchTerm, so pressing Enter walks through
  // matches in order. The history pane is a passive mirror, used
  // only when the active match falls outside the live viewport.
  //
  // Strategy per call:
  //   1. Advance live.findNext (or findPrevious). If no match, close
  //      any open split and clear history decorations.
  //   2. If after the call live is still anchored to its tail, the
  //      match is in the visible viewport. Close the split if it had
  //      been opened for a prior scrollback match.
  //   3. Otherwise the active match is up in scrollback. Snap live
  //      back to its tail (without clearing live's search state, so
  //      iteration survives), open the split, and run the same search
  //      on the history pane so its decorations + viewport land on
  //      a matching line.
  const submitFind = (
    query: string,
    opts: FindOptions,
    direction: 'next' | 'previous',
  ): boolean => {
    if (query.length === 0) return false;

    // Native surface: the grid owns search, scroll-to-match, and the
    // highlight. Route to the native command and feed the count back to
    // the toolbar; no xterm split is involved.
    if (nativeSurfaceEnabled()) {
      void nativeSurfaceFind({
        query,
        regex: opts.regex ?? false,
        caseSensitive: opts.caseSensitive ?? false,
        wholeWord: opts.wholeWord ?? false,
        forward: direction === 'next',
      })
        .then(([current, total]) => {
          setFindResults({ index: total > 0 ? current - 1 : -1, count: total });
        })
        .catch(() => {});
      return true;
    }

    const live = termRef.current;
    if (!live) return false;

    const hit = direction === 'next' ? live.findNext(query, opts) : live.findPrevious(query, opts);
    if (!hit) {
      hideHistoryMatch();
      return false;
    }

    if (live.isAtBottom()) {
      // Match landed inside the live viewport. Decorations on live
      // are visible; no split needed. Tear down the split if it had
      // been opened for an earlier scrollback match.
      hideHistoryMatch();
      return true;
    }

    // Match is up in scrollback. Pin live back to its tail so it
    // keeps streaming; live's SearchAddon selection + cachedSearchTerm
    // survive the scroll, which is what lets the next call advance.
    // Mirror the search into the history pane so the user can see
    // the highlighted match up there.
    live.scrollToBottom();
    showHistoryMatch(query, opts, direction);
    return true;
  };

  const onFindResults = (event: ISearchResultChangeEvent) =>
    setFindResults({ index: event.resultIndex, count: event.resultCount });

  return {
    findOpen,
    openFind: () => setFindOpen(true),
    findToolbarRef,
    findResults,
    closeFind,
    submitFind,
    onFindResults,
  };
}
