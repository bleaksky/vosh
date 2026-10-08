import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent } from 'react';
import { snoopWindowOpen } from '../ipc/snoop';
import { saveSnoopSize, useSnoopSize } from '../stores/config/snoopSizeStore';
import { selectSnoop, setSnoopsFolded, useSnoops } from '../stores/session/snoopStore';
import { pushToast } from '../stores/toasts';
import { FindToolbar } from '../terminal/FindToolbar';
import { SnoopTerminal } from '../terminal/SnoopTerminal';
import { SnoopStrip } from './SnoopStrip';
import { SNOOP_REQUEST_EVENT, type SnoopRequest } from './snoopKeys';
import {
  dragTo,
  SNOOP_FOLDED,
  snoopHeight,
  type SnoopDrag,
  type SnoopRoom,
} from './snoopSplitSize';
import { useSnoopFind } from './useSnoopFind';

// The snoop split at the top of the terminal column. The strip with a
// tab for each player the selected session snoops
// (shell/SnoopStrip.tsx), and under it a terminal for each tab, the one
// in front shown.
//
// It opens at the profile's saved share of the column in whole rows,
// and the line under it drags. A drag to the top folds it to the strip,
// and a double click on the line folds it or opens it again. While it is
// folded, a tab that gets lines takes the unread dot.
//
// It shows nothing with no tab or while the session's snoops sit in
// their window. Nothing here takes the caret: a start leaves it on the
// command line, and a press on the strip hands it back there.
//
// Cmd J is the one way in. From the command line it puts the caret in
// the tab in front, unfolding the split first, and inside a snoop it
// steps to the next tab. While the snoops sit in their window it brings
// the window forward. Cmd F inside a snoop opens its Find, and Copy in
// the menu bar copies what you selected in it (shell/snoopKeys.ts).

interface Props {
  session: number;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  themeTerminalColors: boolean;
  /** Put the caret back on the command line. */
  onCaret: () => void;
}

export function SnoopSplit({
  session,
  fontFamily,
  fontSize,
  lineHeight,
  themeTerminalColors,
  onCaret,
}: Props) {
  const snoops = useSnoops();
  const { tabs, windowed, selected } = snoops;
  const size = useSnoopSize();
  const finder = useSnoopFind(session, selected);
  const [section, setSection] = useState<HTMLElement | null>(null);
  const [column, setColumn] = useState(0);
  const [row, setRow] = useState(0);
  const [drag, setDrag] = useState<SnoopDrag | null>(null);
  const pull = useRef<{ y: number; height: number; to: SnoopDrag | null } | null>(null);
  // Cmd J asked for the caret in the tab in front, which takes it once
  // the tab shows.
  const [caretAsked, setCaretAsked] = useState(0);
  const caretWanted = useRef(false);

  const folded = drag ? drag.folded : size.folded;
  const shown = tabs.length > 0 && !windowed;

  // Lines that reach the front tab while the split is folded leave the
  // unread dot. The fold is the profile's, which the session shown plays.
  useEffect(() => {
    setSnoopsFolded(size.folded, session);
  }, [size.folded, session]);

  // The column's height sizes the split.
  useLayoutEffect(() => {
    const parent = section?.parentElement;
    if (!section || !parent || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => setColumn(parent.clientHeight));
    observer.observe(parent);
    return () => observer.disconnect();
  }, [section]);

  const failed = (e: unknown) => pushToast({ kind: 'error', message: String(e) });
  const fold = (on: boolean) => {
    void saveSnoopSize({ share: size.share, folded: on }).catch(failed);
  };
  const openFind = () => {
    if (size.folded) fold(false);
    finder.open();
  };

  // What Cmd J, Cmd F and Copy ask of the split.
  const onRequest = (request: SnoopRequest) => {
    if (tabs.length === 0) return;
    if (windowed) {
      if (request === 'enter' || request === 'next') snoopWindowOpen(session).catch(failed);
      return;
    }
    if (request === 'copy') {
      const text = finder.front()?.selection() ?? '';
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
    const handle = finder.front();
    if (!handle) return;
    caretWanted.current = false;
    handle.focus();
    // finder.front reads the tab in front, which `selected` names.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
      <SnoopStrip
        session={session}
        snoops={snoops}
        split={{ folded: size.folded, onFold: () => fold(!size.folded) }}
        onFind={openFind}
        onCaret={onCaret}
      />
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
            onRowHeight={setRow}
            {...finder.terminal(tab.name)}
          />
        ))}
      </div>
      {finder.finding && !folded && (
        <FindToolbar
          key={selected}
          results={finder.results}
          onFindNext={(query, options) => finder.find(query, 'next', options)}
          onFindPrevious={(query, options) => finder.find(query, 'previous', options)}
          onClose={() => {
            finder.close();
            onCaret();
          }}
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
    </section>
  );
}
