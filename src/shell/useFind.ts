// Find in scrollback, from the toolbar that Mod+F, the palette and the
// terminal menu open. On the native surface the grid finds, scrolls to
// and highlights each match itself.
//
// Each session keeps its find bar open or closed, with its query and its
// match count, while another session shows. The host keeps the bar of
// each session that has one open mounted, and shows the selected
// session's.

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
  /** The selected session, whose find bar shows. */
  session: number;
  /** The live pane, which owns iteration. */
  termRef: RefObject<TerminalHandle>;
  /** The history pane, while the split shows. */
  historyTermRef: RefObject<TerminalHandle>;
  /** Puts the caret on the command line. */
  focusInput: () => void;
}

/** The active match and the match count, for the toolbar badge. */
export interface FindResults {
  index: number;
  count: number;
}

export interface ScrollbackFind {
  /** The selected session's find bar is open. */
  findOpen: boolean;
  openFind: () => void;
  /** The selected session's find bar. */
  findToolbarRef: RefObject<FindToolbarHandle>;
  /** Each session whose find bar is open, with its match count. */
  finds: ReadonlyMap<number, FindResults>;
  closeFind: () => void;
  /** Run a find from the toolbar. Returns whether it found a match. */
  submitFind: (query: string, opts: FindOptions, direction: 'next' | 'previous') => boolean;
  /** The onResultsChanged of the live pane of `session`. */
  onFindResults: (session: number, event: ISearchResultChangeEvent) => void;
}

/** The count before a search runs. */
const NO_RESULTS: FindResults = { index: -1, count: 0 };

export function useFind({
  session,
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
  //
  // Each open bar's match count, by session, from its live pane's
  // SearchAddon. Drives the "3 / 12" badge in the find toolbar. `index`
  // of -1 means the active match was lost (e.g. after the toolbar opened
  // but before the first search ran).
  const [finds, setFinds] = useState<ReadonlyMap<number, FindResults>>(() => new Map());
  const findOpen = finds.has(session);
  const findToolbarRef = useRef<FindToolbarHandle | null>(null);

  /** Set the count of the open bar of `id`. A closed bar takes none. */
  const setResults = (id: number, results: FindResults) =>
    setFinds((now) => (now.has(id) ? new Map(now).set(id, results) : now));

  const closeFind = () => {
    termRef.current?.clearSearch();
    historyTermRef.current?.clearSearch();
    if (nativeSurfaceEnabled()) {
      void nativeSurfaceFindClear(session).catch(() => {});
    }
    clearQueuedSearch();
    setFinds((now) => {
      const next = new Map(now);
      next.delete(session);
      return next;
    });
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
      const id = session;
      void nativeSurfaceFind(
        {
          query,
          regex: opts.regex ?? false,
          caseSensitive: opts.caseSensitive ?? false,
          wholeWord: opts.wholeWord ?? false,
          forward: direction === 'next',
        },
        id,
      )
        .then(([current, total]) => {
          setResults(id, { index: total > 0 ? current - 1 : -1, count: total });
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

  const onFindResults = (id: number, event: ISearchResultChangeEvent) =>
    setResults(id, { index: event.resultIndex, count: event.resultCount });

  const openFind = () =>
    setFinds((now) => (now.has(session) ? now : new Map(now).set(session, NO_RESULTS)));

  return {
    findOpen,
    openFind,
    findToolbarRef,
    finds,
    closeFind,
    submitFind,
    onFindResults,
  };
}
