import { useLayoutEffect, useRef, useState } from 'react';
import { snoopClose, snoopStop, snoopWindowOpen } from '../ipc/snoop';
import { selectSnoop, type Snoops } from '../stores/session/snoopStore';
import { pushToast } from '../stores/toasts';
import { MoreIcon } from '../ui/icons';
import { SnoopMenu, type SnoopPick } from './SnoopMenu';
import { EyeIcon } from './icons';
import { useMinuteClock } from './sessionLine';
import { endedLine, tabTitle } from './snoopLine';

// The strip of the snoop split and of the snoop window, boards 01 to 04
// and 06 of the Snoop review (SN1, SN2 and SN5). The eye, a tab for each
// player the session snoops, each with the sessions sidebar's mark, a
// dot while it runs and a ring once it ended, and an accent dot on a tab
// behind with lines you have not read. Stop sends `snoop stop` with the
// player in front, and an ended tab says when it ended and offers Close.
// The more button opens the menu.
//
// In a narrow strip Stop leaves first, since the menu holds it too. Then
// the names end in an ellipsis, the one in front last.
//
// In the snoop window the strip sits in the 32 band beside the traffic
// lights and drags the window, and its menu has no Open in a window and
// no Fold.

interface Props {
  session: number;
  snoops: Snoops;
  /** The split's fold, which brings Open in a window and Fold to the
   *  menu. The snoop window leaves it out. */
  split?: { folded: boolean; onFold: () => void };
  /** Open Find on the tab in front. */
  onFind: () => void;
  /** Hand the caret back after a press. */
  onCaret: () => void;
}

/** The strip's widths besides the tabs and Stop: its insets, the eye,
 *  the space before the end and the more button. */
const STRIP_REST = 12 + 10 + 28 + 12 + 28;
/** The space between two tabs. */
const TAB_GAP = 2;

export function SnoopStrip({ session, snoops, split, onFind, onCaret }: Props) {
  const { tabs, selected, unread } = snoops;
  const now = useMinuteClock();
  const [narrow, setNarrow] = useState(false);
  const [tight, setTight] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const stripRef = useRef<HTMLDivElement | null>(null);
  const tabsRef = useRef<HTMLDivElement | null>(null);
  const actRef = useRef<HTMLDivElement | null>(null);
  const actWidth = useRef(0);
  const moreRef = useRef<HTMLButtonElement | null>(null);
  const front = tabs.find((tab) => tab.name === selected) ?? null;
  // The band of the snoop window drags the window from any spot of the
  // strip that is not a button.
  const drag = split ? {} : { 'data-tauri-drag-region': '' };

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
  const measures = useRef(measure);
  measures.current = measure;

  // The strip's width says whether Stop fits.
  useLayoutEffect(() => {
    const strip = stripRef.current;
    if (!strip || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => measures.current());
    observer.observe(strip);
    return () => observer.disconnect();
  }, []);

  // The names and Stop change as tabs come, go and change hands.
  useLayoutEffect(measure, [tabs, selected, unread, front?.live]);

  const act = (run: () => Promise<void>) => {
    run().catch((e: unknown) => pushToast({ kind: 'error', message: String(e) }));
    onCaret();
  };
  const stopFront = () => {
    if (!front) return;
    act(() => (front.live ? snoopStop(session, front.name) : snoopClose(session, front.name)));
  };
  const onPick = (pick: SnoopPick) => {
    if (pick === 'stop') stopFront();
    else if (pick === 'stop-all') act(() => snoopStop(session));
    else if (pick === 'find') onFind();
    else if (pick === 'window') act(() => snoopWindowOpen(session));
    else split?.onFold();
  };

  return (
    <div ref={stripRef} className="snoop-strip" {...drag}>
      <span className="snoop-eye" role="img" aria-label="Snoop">
        <EyeIcon />
      </span>
      <div
        ref={tabsRef}
        className={'snoop-tabs' + (tight ? ' is-tight' : '')}
        role="tablist"
        aria-label="Snooped players"
        {...drag}
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
              selectSnoop(tab.name, session);
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
      <div className="snoop-end" {...drag}>
        {front && !narrow && (
          <div ref={actRef} className="snoop-act" {...drag}>
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
      {menuOpen && moreRef.current && (
        <SnoopMenu
          anchor={moreRef.current}
          front={front}
          folded={split?.folded}
          onPick={onPick}
          onClose={(reason) => {
            setMenuOpen(false);
            if (reason !== 'outside') onCaret();
          }}
        />
      )}
    </div>
  );
}
