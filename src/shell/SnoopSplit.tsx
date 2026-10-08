import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent } from 'react';
import { snoopClose, snoopStop, snoopWindowOpen } from '../ipc/snoop';
import { saveSnoopSize, useSnoopSize } from '../stores/config/snoopSizeStore';
import { selectSnoop, setSnoopsFolded, useSnoops } from '../stores/session/snoopStore';
import { pushToast } from '../stores/toasts';
import { FindToolbar } from '../terminal/FindToolbar';
import type { FindOptions } from '../terminal/terminalHandle';
import {
  SnoopTerminal,
  type SnoopFindResults,
  type SnoopTerminalHandle,
} from '../terminal/SnoopTerminal';
import { MoreIcon } from '../ui/icons';
import { SnoopMenu, type SnoopPick } from './SnoopMenu';
import { EyeIcon } from './icons';
import { useMinuteClock } from './sessionLine';
import { SNOOP_REQUEST_EVENT, type SnoopRequest } from './snoopKeys';
import { endedLine, tabTitle } from './snoopLine';
import {
  dragTo,
  SNOOP_FOLDED,
  snoopHeight,
  type SnoopDrag,
  type SnoopRoom,
} from './snoopSplitSize';

// The snoop split at the top of the terminal column, boards 01 to 04 of
// the Snoop review (SN1, SN2, SN3, SN5 and SN7). A strip with the eye and
// a tab for each player the selected session snoops, each with the
// sessions sidebar's mark, a dot while it runs and a ring once it ended,
// and an accent dot on a tab behind with lines you have not read. Stop
// sends `snoop stop` with the player in front, and an ended tab says when
// it ended and offers Close. The more button opens the split's menu.
// Under the strip, a terminal for each tab, the one in front shown.
//
// It opens at the profile's saved share of the column in whole rows,
// and the line under it drags. A drag to the top folds it to the strip,
// and a double click on the line folds it or opens it again. While it is
// folded, a tab that gets lines takes the unread dot.
//
// In a narrow window Stop leaves the strip first, since the menu holds
// it too. Then the names end in an ellipsis, the one in front last.
//
// It shows nothing with no tab or while the session's snoops sit in
// their window. Nothing here takes the caret: a start leaves it on the
// command line, and a press on the strip hands it back there.
//
// Cmd J is the one way in (SN7). From the command line it puts the
// caret in the tab in front, unfolding the split first, and inside a
// snoop it steps to the next tab. While the snoops sit in their window
// it brings the window forward. Cmd F inside a snoop opens its Find,
// and Copy in the menu bar copies what you selected in it
// (shell/snoopKeys.ts).

interface Props {
  session: number;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  themeTerminalColors: boolean;
  /** Put the caret back on the command line. */
  onCaret: () => void;
}

/** The find bar before a search runs. */
const NO_RESULTS: SnoopFindResults = { index: -1, count: 0 };

/** The strip's widths besides the tabs and Stop: its insets, the eye,
 *  the space before the end and the more button. */
const STRIP_REST = 12 + 10 + 28 + 12 + 28;
/** The space between two tabs. */
const TAB_GAP = 2;

export function SnoopSplit({
  session,
  fontFamily,
  fontSize,
  lineHeight,
  themeTerminalColors,
  onCaret,
}: Props) {
  const { tabs, windowed, selected, unread } = useSnoops();
  const size = useSnoopSize();
  const now = useMinuteClock();
  const [section, setSection] = useState<HTMLElement | null>(null);
  const [column, setColumn] = useState(0);
  const [row, setRow] = useState(0);
  const [drag, setDrag] = useState<SnoopDrag | null>(null);
  const [narrow, setNarrow] = useState(false);
  const [tight, setTight] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [finding, setFinding] = useState(false);
  const [results, setResults] = useState<SnoopFindResults>(NO_RESULTS);
  const handles = useRef(new Map<string, SnoopTerminalHandle>());
  const pull = useRef<{ y: number; height: number; to: SnoopDrag | null } | null>(null);
  const stripRef = useRef<HTMLDivElement | null>(null);
  const tabsRef = useRef<HTMLDivElement | null>(null);
  const actRef = useRef<HTMLDivElement | null>(null);
  const actWidth = useRef(0);
  const moreRef = useRef<HTMLButtonElement | null>(null);
  // Cmd J asked for the caret in the tab in front, which takes it once
  // the tab shows.
  const [caretAsked, setCaretAsked] = useState(0);
  const caretWanted = useRef(false);

  const folded = drag ? drag.folded : size.folded;
  const front = tabs.find((tab) => tab.name === selected) ?? null;
  const shown = tabs.length > 0 && !windowed;

  // Lines that reach the front tab while the split is folded leave the
  // unread dot.
  useEffect(() => {
    setSnoopsFolded(size.folded);
  }, [size.folded]);

  // The find bar searches the tab in front, so it closes when another
  // comes to the front.
  useEffect(() => {
    setFinding(false);
    setResults(NO_RESULTS);
  }, [selected, session]);

  // Stop leaves the strip once the tabs at their full names and Stop no
  // longer fit beside each other. The name in front gives way once the
  // others are as narrow as they go.
  const measure = () => {
    const strip = stripRef.current;
    const list = tabsRef.current;
    // A strip with no width yet has nothing to measure.
    if (!strip?.clientWidth || !list) return;
    if (actRef.current) actWidth.current = actRef.current.offsetWidth + 4;
    const gaps = TAB_GAP * Math.max(0, list.children.length - 1);
    let names = gaps;
    let least = gaps;
    for (const tab of Array.from(list.children) as HTMLElement[]) {
      const name = tab.querySelector<HTMLElement>('.snoop-name');
      const full = tab.offsetWidth + (name ? name.scrollWidth - name.clientWidth : 0);
      names += full;
      least +=
        tab.getAttribute('aria-selected') === 'true'
          ? full
          : parseFloat(getComputedStyle(tab).minWidth) || 0;
    }
    setNarrow(names + actWidth.current > strip.clientWidth - STRIP_REST);
    setTight(least > list.clientWidth + 0.5);
  };

  // The column's height sizes the split, and the strip's width says
  // whether Stop fits.
  useLayoutEffect(() => {
    const parent = section?.parentElement;
    if (!section || !parent || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => {
      setColumn(parent.clientHeight);
      measure();
    });
    observer.observe(parent);
    observer.observe(section);
    return () => observer.disconnect();
  }, [section]);

  // The names and Stop change as tabs come, go and change hands.
  useLayoutEffect(measure, [tabs, selected, unread, front?.live]);

  const failed = (e: unknown) => pushToast({ kind: 'error', message: String(e) });
  const fold = (on: boolean) => {
    void saveSnoopSize({ share: size.share, folded: on }).catch(failed);
  };
  const openFind = () => {
    if (size.folded) fold(false);
    setFinding(true);
  };

  // What Cmd J, Cmd F and Copy ask of the split.
  const onRequest = (request: SnoopRequest) => {
    if (tabs.length === 0) return;
    if (windowed) {
      if (request === 'enter' || request === 'next') snoopWindowOpen(session).catch(failed);
      return;
    }
    const handle = selected ? handles.current.get(selected) : undefined;
    if (request === 'copy') {
      const text = handle?.selection() ?? '';
      if (text) void navigator.clipboard.writeText(text).catch(() => {});
      return;
    }
    if (request === 'find') {
      openFind();
      return;
    }
    if (request === 'next') {
      const at = tabs.findIndex((tab) => tab.name === selected);
      selectSnoop(tabs[(at + 1) % tabs.length].name);
    }
    if (size.folded) fold(false);
    caretWanted.current = true;
    setCaretAsked((n) => n + 1);
  };
  const requests = useRef(onRequest);
  requests.current = onRequest;
  useEffect(() => {
    const hear = (event: Event) => requests.current((event as CustomEvent<SnoopRequest>).detail);
    window.addEventListener(SNOOP_REQUEST_EVENT, hear);
    return () => window.removeEventListener(SNOOP_REQUEST_EVENT, hear);
  }, []);

  // The caret goes to the tab in front once it shows, after the split
  // unfolds and the tab Cmd J stepped to comes to the front.
  useEffect(() => {
    if (!caretWanted.current || size.folded || windowed || !selected) return;
    const handle = handles.current.get(selected);
    if (!handle) return;
    caretWanted.current = false;
    handle.focus();
  }, [caretAsked, size.folded, windowed, selected]);

  if (!shown) return null;

  const room: SnoopRoom | null = column > 0 && row > 0 ? { column, row } : null;
  const height = folded
    ? `${SNOOP_FOLDED}px`
    : drag && !drag.folded
      ? `${drag.height}px`
      : room
        ? `${snoopHeight(size.share, room)}px`
        : `${size.share * 100}%`;

  const act = (run: () => Promise<void>) => {
    run().catch(failed);
    onCaret();
  };
  const closeFind = () => {
    if (selected) handles.current.get(selected)?.clearSearch();
    setFinding(false);
    setResults(NO_RESULTS);
    onCaret();
  };
  const find = (query: string, direction: 'next' | 'previous', options: FindOptions) => {
    const handle = selected ? handles.current.get(selected) : undefined;
    if (!handle) return false;
    return direction === 'next'
      ? handle.findNext(query, options)
      : handle.findPrevious(query, options);
  };

  const stopFront = () => {
    if (!front) return;
    act(() => (front.live ? snoopStop(session, front.name) : snoopClose(session, front.name)));
  };
  const onPick = (pick: SnoopPick) => {
    if (pick === 'stop') stopFront();
    else if (pick === 'stop-all') act(() => snoopStop(session));
    else if (pick === 'find') openFind();
    else if (pick === 'window') act(() => snoopWindowOpen(session));
    else fold(!size.folded);
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || !section) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    e.currentTarget.classList.add('is-dragging');
    document.body.style.cursor = 'row-resize';
    pull.current = { y: e.clientY, height: section.getBoundingClientRect().height, to: null };
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const start = pull.current;
    if (!start || !room) return;
    const to = dragTo(start.height + e.clientY - start.y, room);
    start.to = to;
    setDrag(to);
  };
  const endDrag = (e: PointerEvent<HTMLDivElement>) => {
    const target = e.currentTarget;
    if (target.hasPointerCapture(e.pointerId)) target.releasePointerCapture(e.pointerId);
    target.classList.remove('is-dragging');
    document.body.style.cursor = '';
    const to = pull.current?.to;
    pull.current = null;
    setDrag(null);
    if (!to) return;
    const next = to.folded
      ? { share: size.share, folded: true }
      : { share: to.share, folded: false };
    void saveSnoopSize(next).catch(failed);
  };

  return (
    <section
      ref={setSection}
      className={'snoop' + (folded ? ' is-folded' : '')}
      style={{ height }}
      aria-label="Snoop"
    >
      <div ref={stripRef} className="snoop-strip">
        <span className="snoop-eye" role="img" aria-label="Snoop">
          <EyeIcon />
        </span>
        <div
          ref={tabsRef}
          className={'snoop-tabs' + (tight ? ' is-tight' : '')}
          role="tablist"
          aria-label="Snooped players"
        >
          {tabs.map((tab) => (
            <button
              key={tab.name}
              type="button"
              role="tab"
              className={
                'snoop-tab' +
                (unread.has(tab.name) ? ' is-unread' : '') +
                (tab.live ? '' : ' is-ended')
              }
              aria-selected={tab.name === selected}
              title={tabTitle(tab, now)}
              onClick={() => {
                selectSnoop(tab.name);
                onCaret();
              }}
            >
              <span className="snoop-mark">
                <span className="snoop-dot" />
              </span>
              <span className="snoop-name">{tab.name}</span>
            </button>
          ))}
        </div>
        <div className="snoop-end">
          {front && !narrow && (
            <div ref={actRef} className="snoop-act">
              {!front.live && <span className="snoop-meta">{endedLine(front, now)}</span>}
              <button type="button" className="snoop-btn" onClick={stopFront}>
                {front.live ? 'Stop' : 'Close'}
              </button>
            </div>
          )}
          <button
            ref={moreRef}
            type="button"
            className={'shell-icon-button snoop-more' + (menuOpen ? ' is-open' : '')}
            aria-label="Snoop options"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen((open) => !open)}
          >
            <MoreIcon />
          </button>
        </div>
      </div>
      <div className="snoop-body">
        {tabs.map((tab) => (
          <SnoopTerminal
            key={`${session}:${tab.name}`}
            session={session}
            name={tab.name}
            shown={tab.name === selected && !folded}
            fontFamily={fontFamily}
            fontSize={fontSize}
            lineHeight={lineHeight}
            themeTerminalColors={themeTerminalColors}
            onReady={(handle) => {
              if (handle) handles.current.set(tab.name, handle);
              else handles.current.delete(tab.name);
            }}
            onRowHeight={setRow}
            onFindResults={(found) => {
              if (tab.name === selected) setResults(found);
            }}
          />
        ))}
      </div>
      {finding && !folded && (
        <FindToolbar
          key={selected}
          results={results}
          onFindNext={(query, options) => find(query, 'next', options)}
          onFindPrevious={(query, options) => find(query, 'previous', options)}
          onClose={closeFind}
        />
      )}
      <div
        className="snoop-handle resizable-handle"
        role="separator"
        aria-orientation="horizontal"
        aria-label="Snoop height"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={() => fold(!size.folded)}
      />
      {menuOpen && moreRef.current && (
        <SnoopMenu
          anchor={moreRef.current}
          front={front}
          folded={size.folded}
          onPick={onPick}
          onClose={(reason) => {
            setMenuOpen(false);
            if (reason !== 'outside') onCaret();
          }}
        />
      )}
    </section>
  );
}
