import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent } from 'react';
import { snoopClose, snoopStop } from '../ipc/snoop';
import { saveSnoopSize, useSnoopSize } from '../stores/config/snoopSizeStore';
import { selectSnoop, setSnoopsFolded, useSnoops } from '../stores/session/snoopStore';
import { pushToast } from '../stores/toasts';
import { SnoopTerminal } from '../terminal/SnoopTerminal';
import { EyeIcon } from './icons';
import { useMinuteClock } from './sessionLine';
import { endedLine, tabTitle } from './snoopLine';
import {
  dragTo,
  SNOOP_FOLDED,
  snoopHeight,
  type SnoopDrag,
  type SnoopRoom,
} from './snoopSplitSize';

// The snoop split at the top of the terminal column, boards 01, 03 and
// 04 of the Snoop review (SN1, SN2, SN3, SN5 and SN7). A strip with the
// eye and a tab for each player the selected session snoops, each with
// the sessions sidebar's mark, a dot while it runs and a ring once it
// ended, and an accent dot on a tab behind with lines you have not read.
// Stop sends `snoop stop` with the player in front, and an ended tab says
// when it ended and offers Close. Under the strip, a terminal for each
// tab, the one in front shown.
//
// It opens at the profile's saved share of the column in whole rows,
// and the line under it drags. A drag to the top folds it to the strip,
// and a double click on the line folds it or opens it again. While it is
// folded, a tab that gets lines takes the unread dot.
//
// It shows nothing with no tab or while the session's snoops sit in
// their window. Nothing here takes the caret: a start leaves it on the
// command line, and a press on the strip hands it back there.

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
  const { tabs, windowed, selected, unread } = useSnoops();
  const size = useSnoopSize();
  const now = useMinuteClock();
  const [section, setSection] = useState<HTMLElement | null>(null);
  const [column, setColumn] = useState(0);
  const [row, setRow] = useState(0);
  const [drag, setDrag] = useState<SnoopDrag | null>(null);
  const pull = useRef<{ y: number; height: number; to: SnoopDrag | null } | null>(null);

  const folded = drag ? drag.folded : size.folded;
  const front = tabs.find((tab) => tab.name === selected) ?? null;
  const shown = tabs.length > 0 && !windowed;

  // Lines that reach the front tab while the split is folded leave the
  // unread dot.
  useEffect(() => {
    setSnoopsFolded(size.folded);
  }, [size.folded]);

  // The column's height sizes the split.
  useLayoutEffect(() => {
    const parent = section?.parentElement;
    if (!section || !parent || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => setColumn(parent.clientHeight));
    observer.observe(parent);
    return () => observer.disconnect();
  }, [section]);

  if (!shown) return null;

  const room: SnoopRoom | null = column > 0 && row > 0 ? { column, row } : null;
  const height = folded
    ? `${SNOOP_FOLDED}px`
    : drag && !drag.folded
      ? `${drag.height}px`
      : room
        ? `${snoopHeight(size.share, room)}px`
        : `${size.share * 100}%`;

  const failed = (e: unknown) => pushToast({ kind: 'error', message: String(e) });
  const act = (run: () => Promise<void>) => {
    run().catch(failed);
    onCaret();
  };
  const fold = (on: boolean) => {
    void saveSnoopSize({ share: size.share, folded: on }).catch(failed);
  };
  const stopFront = () => {
    if (!front) return;
    act(() => (front.live ? snoopStop(session, front.name) : snoopClose(session, front.name)));
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
      <div className="snoop-strip">
        <span className="snoop-eye" role="img" aria-label="Snoop">
          <EyeIcon />
        </span>
        <div className="snoop-tabs" role="tablist" aria-label="Snooped players">
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
        {front && (
          <div className="snoop-end">
            {!front.live && <span className="snoop-meta">{endedLine(front, now)}</span>}
            <button type="button" className="snoop-btn" onClick={stopFront}>
              {front.live ? 'Stop' : 'Close'}
            </button>
          </div>
        )}
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
            onRowHeight={setRow}
          />
        ))}
      </div>
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
