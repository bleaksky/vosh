// The scrollback split on xterm. A wheel up over the terminal, Page Up,
// Mod+\ or a find match up in scrollback opens a history pane over the
// live one, so you read back while the live pane keeps its tail. The
// native grid splits its own display, so on the native surface each path
// hands off to it or leaves the split closed.
//
// Each session keeps its split open or closed, and a find match that
// waits for its history pane, while another session shows. The split
// shows the selected session's, and its history pane mounts afresh for
// each selection, since the host keys it by session.

import { useCallback, useEffect, useRef, useState, type MouseEvent, type RefObject } from 'react';
import { nativeSurfaceScroll } from '../ipc/nativeSurface';
import { noteReader } from '../terminal/readerBusy';
import { listenSplitDrag, SplitDrag } from '../terminal/splitDrag';
import type { FindOptions, TerminalHandle } from '../terminal/terminalHandle';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';

interface SplitPanes {
  /** The selected session, whose split shows. */
  session: number;
  /** The live pane. */
  termRef: RefObject<TerminalHandle>;
  /** The history pane, while the split shows. */
  historyTermRef: RefObject<TerminalHandle>;
  /** The terminal area, which takes the wheel. */
  terminalAreaRef: RefObject<HTMLDivElement>;
  /** Puts the caret on the command line. */
  focusInput: () => void;
}

export interface ScrollbackSplit {
  splitOpen: boolean;
  /** The history pane has its scrollback and shows. */
  historyReady: boolean;
  /** How far back the history pane shows, for the depth chip. */
  historyScrollPos: { back: number; max: number } | null;
  toggleSplit: () => void;
  /** A middle click over the terminal. */
  middleClick: (event: MouseEvent) => void;
  /** Page Up and Page Down from the command line, as signed pages. */
  pageSplit: (pages: number) => void;
  /** Esc from the command line. */
  exitSplit: () => void;
  /** The history pane's onScrollbackLoaded. */
  onHistoryLoaded: () => void;
  /** The history pane's onScrollPosition. */
  onHistoryScroll: (back: number, max: number) => void;
  /** Show a find match that sits up in scrollback in the history pane,
   *  opening the split for it. */
  showHistoryMatch: (query: string, opts: FindOptions, direction: 'next' | 'previous') => void;
  /** Clear the history pane's match and close the split. */
  hideHistoryMatch: () => void;
  /** Drop a match still waiting for the history pane. */
  clearQueuedSearch: () => void;
}

// Page back once more when a PageUp opened the split, after the history
// pane sits just above the live rows. The load callback and the frame
// poll both position the pane, and the poll comes last and positions it
// from its bottom, so only the poll takes the page for good.
function pageOnOpen(pending: { current: boolean }, history: TerminalHandle, take: boolean) {
  if (!pending.current) return;
  if (take) pending.current = false;
  history.scrollPages(-1);
}

export function useScrollbackSplit({
  session,
  termRef,
  historyTermRef,
  terminalAreaRef,
  focusInput,
}: SplitPanes): ScrollbackSplit {
  // splitOpen state needs to be read inside the wheel handler. The
  // handler is registered once and runs many times, so we mirror the
  // state into a ref to avoid stale closures. So is the session.
  const splitOpenRef = useRef(false);
  const sessionRef = useRef(session);
  sessionRef.current = session;
  // Split-scrollback state, the sessions whose split is open. When the
  // selected session's is, a second xterm appears above the live one
  // and shows the same buffer scrolled back so you can read earlier
  // output while live combat keeps streaming below.
  const [openSplits, setOpenSplits] = useState<ReadonlySet<number>>(() => new Set());
  const splitOpen = openSplits.has(session);
  // Open or close the split of the session that shows now.
  const setSplitOpen = useCallback((open: boolean) => {
    const id = sessionRef.current;
    setOpenSplits((now) => {
      if (now.has(id) === open) return now;
      const next = new Set(now);
      if (open) next.add(id);
      else next.delete(id);
      return next;
    });
  }, []);
  // History pane readiness: flips true once the history Terminal has
  // finished loading scrollback after its mount. We queue any pending
  // mirror search through pendingFindRef until the history is ready,
  // since findNext on an empty buffer would silently return no match.
  const [historyReady, setHistoryReady] = useState(false);
  // Live-pane row count captured at the moment the split opens, before
  // the live pane refits to its post-split smaller height. Used by
  // onScrollbackLoaded to position the history viewport so its bottom
  // row is the line that was immediately above the live pane's top
  // row — i.e. opening the split produces zero apparent motion.
  // Reading termRef.getSize().rows inside onScrollbackLoaded was
  // unreliable: that callback may fire before or after the live pane
  // refits, and the answer is different in each case.
  const preSplitLiveRowsRef = useRef(0);
  // A PageUp that opened the split also pages, once the history pane
  // sits at the live pane's top. The wheel, Mod+\ and find open it with
  // no motion and leave this false.
  const pendingPageRef = useRef(false);
  // A find match waiting for the history pane, by session.
  const pendingFindRef = useRef(
    new Map<number, { query: string; opts: FindOptions; direction: 'next' | 'previous' }>(),
  );
  // History pane scroll depth, driven by the Terminal's onScrollPosition
  // callback. Drives the depth chip at the terminal's top right.
  const [historyScrollPos, setHistoryScrollPos] = useState<{
    back: number;
    max: number;
  } | null>(null);

  // Reset the history pane's scroll depth whenever the split
  // closes or another session's shows. The history Terminal unmounts and
  // the next mount will fire its own onScrollPosition; keeping the prior
  // value here would flash stale numbers for one paint before being
  // overwritten.
  useEffect(() => {
    setHistoryScrollPos(null);
    splitOpenRef.current = splitOpen;
    // Reading back in the split leaves your prompt's clock as it is. A
    // session's split opens and closes only while it shows, so the
    // session that shows is the one to tell.
    noteReader('split', splitOpen, session);
  }, [splitOpen, session]);

  // Reset history readiness whenever the split closes or another
  // session's shows. The next time a split shows, the history Terminal
  // mounts again and the onScrollbackLoaded callback will set this back
  // to true.
  useEffect(() => {
    setHistoryReady(false);
  }, [splitOpen, session]);

  // Reveal the split as soon as its scrollback lands, not on a fixed
  // timer. The history pane's xterm is held at `visibility: hidden` (the
  // priming class) until `historyReady` flips true, which normally
  // happens in onScrollbackLoaded — but that runs off an xterm
  // write-drain callback that intermittently never fires, stranding the
  // pane hidden and blank. So poll each frame: the moment the buffer
  // holds real content, position it, reveal it, and repaint a few frames
  // (the DOM renderer otherwise leaves the freshly shown rows blank).
  // A ~1.5s ceiling reveals it anyway so an empty or never-arriving
  // buffer can't strand it hidden. Idempotent with the onScrollbackLoaded
  // path, which still runs when it does fire.
  useEffect(() => {
    if (!splitOpen) return;
    let raf = 0;
    // The repaint frames, which stop with the poll. A wheel can close the
    // split inside them, and the pane they would repaint is gone then.
    let repaintRaf = 0;
    let done = false;
    let tries = 0;
    const repaintBurst = () => {
      let frames = 0;
      const repaint = () => {
        historyTermRef.current?.refresh();
        if (++frames < 6) repaintRaf = requestAnimationFrame(repaint);
      };
      repaintRaf = requestAnimationFrame(repaint);
    };
    const tick = () => {
      const h = historyTermRef.current;
      if (h && !done) {
        const size = h.contentSize();
        if (size.bufferLength > size.rows + 1) {
          done = true;
          const scrollBack = preSplitLiveRowsRef.current;
          h.scrollToBottom();
          if (scrollBack > 0) h.scrollLines(-scrollBack);
          // This positions the pane from its bottom, after any load
          // callback, so it takes the page for good.
          pageOnOpen(pendingPageRef, h, true);
          setHistoryReady(true);
          repaintBurst();
          return;
        }
      }
      if (++tries < 90) {
        raf = requestAnimationFrame(tick);
      } else {
        setHistoryReady(true);
        repaintBurst();
      }
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      cancelAnimationFrame(repaintRaf);
      // The split closed or another session shows, so a page still
      // waiting has nothing to land in.
      pendingPageRef.current = false;
    };
  }, [splitOpen, session, historyTermRef]);

  // Drain a queued search once the split has opened and the history
  // pane finishes loading scrollback. showHistoryMatch enqueues here
  // when a live-pane search would have scrolled the live pane off its
  // tail — we hand the search off to the history pane and run it as
  // soon as history is ready to receive it. This runs before the
  // selection sync below subscribes, so the match it selects in history
  // does not clear the live selection that find iterates from.
  useEffect(() => {
    if (!splitOpen || !historyReady) return;
    const pending = pendingFindRef.current.get(session);
    if (!pending) return;
    pendingFindRef.current.delete(session);
    const handle = historyTermRef.current;
    if (!handle) return;
    if (pending.direction === 'next') handle.findNext(pending.query, pending.opts);
    else handle.findPrevious(pending.query, pending.opts);
  }, [splitOpen, historyReady, session, historyTermRef]);

  // Sync selections between the live and history panes so only one
  // can be active at a time. Without this, dragging a selection in
  // the history pane while a stale live-pane selection lingers
  // produces two simultaneous selections that compete for the copy
  // shortcut (the live pane's wins) — confusing the user who only
  // sees the history-pane highlight. Reactive cross-clear means
  // the most recent gesture is always the one that "owns" the
  // selection.
  useEffect(() => {
    if (!historyReady) return;
    const live = termRef.current;
    const hist = historyTermRef.current;
    if (!live || !hist) return;
    const unsubLive = live.onSelectionChange(() => {
      if (live.hasSelection()) hist.clearSelection();
    });
    const unsubHist = hist.onSelectionChange(() => {
      if (hist.hasSelection()) live.clearSelection();
    });
    return () => {
      unsubLive();
      unsubHist();
    };
  }, [historyReady, termRef, historyTermRef]);

  // A drag that starts on the history pane's text and goes below it
  // scrolls the history down and hands its selection to the live pane
  // when the split closes at the bottom (src/terminal/splitDrag.ts). One
  // controller for the window's life, since the drag outlives the split.
  // The wheel and Page Down tell it when they take the history to its
  // bottom, so a drag carries on through that close too.
  const historyReadyRef = useRef(false);
  useEffect(() => {
    historyReadyRef.current = historyReady;
  }, [historyReady]);
  const splitDragRef = useRef<SplitDrag | null>(null);
  useEffect(() => {
    const drag = new SplitDrag({
      history: () =>
        splitOpenRef.current && historyReadyRef.current ? historyTermRef.current : null,
      live: () => termRef.current,
      closeSplit: () => setSplitOpen(false),
    });
    splitDragRef.current = drag;
    const stop = listenSplitDrag(window, drag, nativeSurfaceEnabled);
    return () => {
      stop();
      splitDragRef.current = null;
    };
  }, [termRef, historyTermRef, setSplitOpen]);

  // Wheel listener attached in capture phase with passive:false so we
  // fire BEFORE the xterm canvas inside terminal-area sees the event.
  // Without capture phase, xterm's own bubble-phase handler scrolls
  // the live pane first and preventDefault is too late; the live pane
  // would scroll along with the history pane any time the cursor hovered
  // over it during a wheel gesture. stopPropagation guarantees the
  // event never reaches xterm at all when we handle it ourselves.
  useEffect(() => {
    const el = terminalAreaRef.current;
    if (!el) return;
    // Accumulate raw deltaY so high-frequency touchpad events
    // (~60 small deltas/sec on macOS) don't compound into a runaway
    // scroll. Each PX_PER_LINE pixels of accumulated delta = one
    // line scrolled in the history pane; CRITICAL: we
    // preventDefault on every event we're "handling" — even when
    // the accumulator hasn't ticked over a line yet — otherwise
    // small touchpad deltas leak through to xterm and scroll the
    // LIVE pane while the user thinks they're scrolling history.
    const PX_PER_LINE = 12;
    let wheelAccum = 0;
    const onWheel = (e: globalThis.WheelEvent) => {
      // Native surface: the sizer forwards the wheel to the native
      // grid, which scrolls and splits its own display. Opening the
      // DOM split here would lay xterm's history pane over the grid.
      if (nativeSurfaceEnabled()) return;
      if (e.deltaY === 0) return;
      const scrollingUp = e.deltaY < 0;
      const splitOpen = splitOpenRef.current;
      // Decide whether this event belongs to us or to xterm's live
      // pane handler: up-scroll always belongs to us (it opens the
      // split or scrolls history); down-scroll belongs to us only
      // when the split is already open. Anything else falls
      // through to xterm.
      const ours = scrollingUp || splitOpen;
      if (!ours) return;
      e.preventDefault();
      e.stopPropagation();
      // Direction change resets the accumulator so a fresh swipe
      // doesn't inherit leftover delta from the previous direction.
      if (wheelAccum !== 0 && Math.sign(e.deltaY) !== Math.sign(wheelAccum)) {
        wheelAccum = 0;
      }
      // First up-scroll opens the split without consuming the
      // accumulator — gives the user a single "intent" gesture
      // before history starts moving.
      if (scrollingUp && !splitOpen) {
        preSplitLiveRowsRef.current = termRef.current?.getSize().rows ?? 0;
        setSplitOpen(true);
        wheelAccum = 0;
        return;
      }
      wheelAccum += e.deltaY;
      const lines = Math.trunc(wheelAccum / PX_PER_LINE);
      if (lines === 0) return;
      wheelAccum -= lines * PX_PER_LINE;
      historyTermRef.current?.scrollLines(lines);
      if (lines > 0) {
        queueMicrotask(() => {
          if (!historyTermRef.current?.isAtBottom()) return;
          splitDragRef.current?.historyBottomed();
          setSplitOpen(false);
        });
      }
    };
    el.addEventListener('wheel', onWheel, { passive: false, capture: true });
    return () => el.removeEventListener('wheel', onWheel, { capture: true });
  }, [terminalAreaRef, termRef, historyTermRef, setSplitOpen]);

  // Open or close the scrollback split, the keyboard twin of a middle
  // click. The native grid splits itself when it scrolls back, so it
  // pages up into history or snaps back to the tail. xterm mounts the
  // history pane above the live one.
  const toggleSplit = () => {
    if (nativeSurfaceEnabled()) {
      void nativeSurfaceScroll('toggle', session).catch(() => {});
      return;
    }
    if (splitOpenRef.current) {
      setSplitOpen(false);
      termRef.current?.scrollToBottom();
      return;
    }
    // Same pre-split row capture as the wheel and PageUp paths.
    preSplitLiveRowsRef.current = termRef.current?.getSize().rows ?? 0;
    setSplitOpen(true);
  };

  // Middle-click (scroll-wheel click) closes the split-scrollback
  // view and snaps the live pane to the bottom. Standard "remove
  // scrollback break" gesture for users coming from other clients.
  const middleClick = (event: MouseEvent) => {
    event.preventDefault();
    if (splitOpen) setSplitOpen(false);
    termRef.current?.scrollToBottom();
    // Snapping the scrollback back to the bottom is a "get me back to
    // typing" gesture, so return the caret to the command line rather
    // than leaving focus on the terminal surface.
    focusInput();
  };

  const pageSplit = (pages: number) => {
    // Native surface: the grid pages its own display in place
    // (Input invokes native_surface_scroll alongside this), so
    // the DOM split stays closed. Opening it would lay xterm's
    // history pane over the grid.
    if (nativeSurfaceEnabled()) return;
    // Split-scrollback gesture. The live pane (termRef) stays
    // anchored to the tail. PageUp opens the split if closed and
    // pages too, so one press lands a page back. The history
    // Terminal mounts on that state change and is null until then,
    // so the page waits for its scrollback to land.
    if (pages < 0) {
      if (!splitOpen) {
        // Same pre-split row capture as the wheel path: without it
        // onScrollbackLoaded scrolls back zero rows and the history
        // pane opens showing a duplicate of the live tail.
        preSplitLiveRowsRef.current = termRef.current?.getSize().rows ?? 0;
        pendingPageRef.current = true;
        setSplitOpen(true);
        return;
      }
      historyTermRef.current?.scrollPages(pages);
      return;
    }
    if (!splitOpen) return;
    historyTermRef.current?.scrollPages(pages);
    // After the page-down lands, close the split if we paged
    // all the way back to the live tail.
    queueMicrotask(() => {
      if (!historyTermRef.current?.isAtBottom()) return;
      splitDragRef.current?.historyBottomed();
      setSplitOpen(false);
    });
  };

  const exitSplit = () => {
    // Esc always snaps the live pane back to the bottom AND
    // closes the split if it is open. So a user who scrolled
    // up via mouse wheel or PageUp gets jumped back to the
    // live tail with one keystroke whether the split is
    // showing or not.
    if (splitOpen) setSplitOpen(false);
    termRef.current?.scrollToBottom();
  };

  const onHistoryLoaded = () => {
    // Position the history pane so its bottom row is the
    // line immediately above the live pane's full row
    // range. The live pane is `position: absolute` with
    // both top and bottom pinned (overlay model), so its
    // xterm renders the full terminal-area row count even
    // while the history overlay covers part of it. If
    // history's bottom landed inside live's row range the
    // same lines would render in both panes — opaque
    // overlay hides that visually, but the depth
    // chip still makes more sense when the panes
    // describe disjoint buffer regions. Pre-split live
    // rows captured in the wheel handler because reading
    // the live pane's size here is racey.
    // Fast path: when the load callback fires, position and
    // reveal immediately. The frame-polled effect above also
    // positions / reveals / repaints, so this is idempotent and
    // a no-op when the callback never fires.
    const h = historyTermRef.current;
    const scrollBack = preSplitLiveRowsRef.current;
    if (h) {
      if (scrollBack > 0) h.scrollLines(-scrollBack);
      pageOnOpen(pendingPageRef, h, false);
    }
    setHistoryReady(true);
  };

  const onHistoryScroll = (back: number, max: number) => setHistoryScrollPos({ back, max });

  // Find found its match up in scrollback, so the history pane shows it
  // while the live pane keeps its tail. A split that is still loading
  // runs the search once its history is ready.
  const showHistoryMatch = (query: string, opts: FindOptions, direction: 'next' | 'previous') => {
    if (!splitOpen) {
      pendingFindRef.current.set(session, { query, opts, direction });
      preSplitLiveRowsRef.current = termRef.current?.getSize().rows ?? 0;
      setSplitOpen(true);
    } else if (historyTermRef.current && historyReady) {
      if (direction === 'next') historyTermRef.current.findNext(query, opts);
      else historyTermRef.current.findPrevious(query, opts);
    } else {
      pendingFindRef.current.set(session, { query, opts, direction });
    }
  };

  const hideHistoryMatch = () => {
    if (splitOpen) {
      historyTermRef.current?.clearSearch();
      setSplitOpen(false);
    }
  };

  const clearQueuedSearch = () => {
    pendingFindRef.current.delete(session);
  };

  return {
    splitOpen,
    historyReady,
    historyScrollPos,
    toggleSplit,
    middleClick,
    pageSplit,
    exitSplit,
    onHistoryLoaded,
    onHistoryScroll,
    showHistoryMatch,
    hideHistoryMatch,
    clearQueuedSearch,
  };
}
