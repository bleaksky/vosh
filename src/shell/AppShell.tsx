import {
  useEffect,
  useRef,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type ReactNode,
} from 'react';
import { isMacPlatform } from '../lib/shortcuts';
import { PANEL_WIDTH_MAX, panelWidthFloor } from '../panel/paneLayout';
import {
  clampSessionsWidth,
  MIN_TERMINAL_WIDTH,
  SESSIONS_WIDTH_MAX,
  SESSIONS_WIDTH_MIN,
  SESSIONS_WIDTH_STOCK,
  sessionsColumn,
} from './sessionsColumn';

// The One Window frame (SPEC 1). A CSS grid with the sessions column,
// the terminal column and the panel column. Rows are the 32 px title
// band, the terminal, the input band (at least 40, taller while you
// compose several lines), and the 28 px status line. The band spans
// the terminal and panel columns, and the sessions sidebar and the
// panel span every row, so all three grounds run to the top edge.
//
// Every slot renders at a fixed position in a fixed parent, and the
// panel toggle, the width drag and the sessions sidebar only change the
// column template through custom properties on the root. So showing,
// hiding, or resizing the panel or the sidebar never moves the Terminal
// or the Input to a new parent, and neither remounts. A remount would
// reload the scrollback, drop the underlay's pointer forwarding, and
// lose command history.
//
// The root also publishes --panel-w (the panel's width, open or not)
// and --panel-col (the width the panel takes, 0 while hidden) for the
// panel and for overlays that keep clear of it. While the sessions
// sidebar shows it publishes --sessions-col, its column, which the
// panel's clamp counts. With one session the sidebar is gone, the first
// column takes 0, and the frame is the one it was before sessions.
//
// The sessions toggle holds one spot in the frame's top left corner,
// over the sidebar's top while it shows and at the band's left end while
// it hides (Sessions toggle T1). The root says which with data-lead, so
// the band's title keeps clear of it only while it sits over the band.
// In a window too narrow for the sidebar's column, the sidebar slides
// over the terminal from the left edge instead (T5), under the toggle.
//
// The sidebar's 1 px line is its width handle too, below the sidebar's
// top 32, the way the panel's edge is the panel's. Each drag writes its
// columns straight to the root and keeps the width on release.
//
// On Windows and Linux the panel draws at least 248 px wide, so the
// title band's buttons and window controls all sit over it. A saved
// width under that stays saved, and macOS draws it as saved.

/** Arrow keys on either edge move it this far. */
const KEY_STEP = 8;

/** The panel's edge, or the sessions sidebar's line. */
type Edge = 'panel' | 'sessions';

interface Props {
  panelOpen: boolean;
  /** The saved panel width in CSS pixels. The panel draws no
   *  narrower than panelWidthFloor. */
  panelWidth: number;
  /** Save a panel width after a drag or a key press. Null goes back to
   *  the stock width. */
  onPanelWidth: (px: number | null) => void;
  /** The sessions sidebar while it shows, which takes the first column,
   *  or null. */
  sessions?: ReactNode;
  /** The sidebar's rows' width, 180 to 320, without its line. */
  sessionsWidth?: number;
  /** Keep the width a drag or a key gave the sidebar. */
  onSessionsWidth?: (px: number) => void;
  /** The sessions sidebar over the terminal, in a window too narrow for
   *  its column, or null. */
  sessionsOverlay?: ReactNode;
  /** The sessions toggle, in the top left corner, or null. */
  sessionsToggle?: ReactNode;
  titleBand: ReactNode;
  /** The snoop split, at the top of the terminal column, or null. It
   *  renders first in the terminal's slot, so the terminal keeps its
   *  parent and its place among the slot's children as the split comes
   *  and goes, and never remounts. */
  snoop?: ReactNode;
  terminal: ReactNode;
  input: ReactNode;
  statusLine: ReactNode;
  panel: ReactNode;
  onMouseUp?: (event: MouseEvent<HTMLElement>) => void;
  /** Floating surfaces: menus, the palette, toasts, dialogs. */
  children?: ReactNode;
}

/** The panel width a drag or a key lands on, past the sessions column
 *  `side` and the terminal's floor. */
function clampWidth(px: number, floor: number, side: number): number {
  const room = window.innerWidth - side - MIN_TERMINAL_WIDTH;
  const max = Math.max(floor, Math.min(PANEL_WIDTH_MAX, room));
  return Math.round(Math.min(max, Math.max(floor, px)));
}

/** The panel column for a panel `px` wide, which never squeezes the
 *  terminal under its floor past the sessions column `side`, even when
 *  the window shrinks below a width saved on a bigger screen. */
function panelColumn(px: number, side: number): string {
  return `min(${px}px, calc(100vw - ${side + MIN_TERMINAL_WIDTH}px))`;
}

export function AppShell({
  panelOpen,
  panelWidth,
  onPanelWidth,
  sessions = null,
  sessionsWidth = SESSIONS_WIDTH_STOCK,
  onSessionsWidth,
  sessionsOverlay = null,
  sessionsToggle = null,
  titleBand,
  snoop = null,
  terminal,
  input,
  statusLine,
  panel,
  onMouseUp,
  children,
}: Props) {
  const floor = panelWidthFloor(isMacPlatform());
  const width = Math.max(floor, panelWidth);
  const side = sessions === null ? 0 : sessionsColumn(sessionsWidth);
  const rootRef = useRef<HTMLElement | null>(null);
  const panelRef = useRef<HTMLElement | null>(null);
  const dragRef = useRef<{
    edge: Edge;
    pointerId: number;
    startX: number;
    start: number;
    last: number;
  } | null>(null);

  // A hidden panel keeps its panes mounted (so the map and chat keep
  // their state) but leaves the tab order and the accessibility tree.
  useEffect(() => {
    if (panelRef.current) panelRef.current.inert = !panelOpen;
  }, [panelOpen]);

  // The width an edge lands on: the panel past the sessions column and
  // the terminal's floor, the sidebar never so wide that it would fold.
  const clampEdge = (edge: Edge, px: number) =>
    edge === 'panel'
      ? clampWidth(px, floor, side)
      : clampSessionsWidth(px, window.innerWidth, panelOpen ? floor : 0);

  // Drag frames write the columns straight to the root and tell the
  // terminal to refit in the same frame, the way Resizable does. React
  // state catches up once, on release. The panel's column counts the
  // sidebar's, so the sidebar writes both.
  const preview = (edge: Edge, px: number) => {
    const root = rootRef.current;
    if (!root) return;
    if (edge === 'panel') {
      root.style.setProperty('--panel-w', `${px}px`);
      root.style.setProperty('--panel-col', panelColumn(px, side));
    } else {
      root.style.setProperty('--sessions-col', `${sessionsColumn(px)}px`);
      if (panelOpen) root.style.setProperty('--panel-col', panelColumn(width, sessionsColumn(px)));
    }
    window.dispatchEvent(new CustomEvent('vosh:resize-progress', { detail: { size: px } }));
  };

  const keep = (edge: Edge, px: number) =>
    edge === 'panel' ? onPanelWidth(px) : onSessionsWidth?.(px);

  const onPointerDown = (edge: Edge) => (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    const start = edge === 'panel' ? width : sessionsWidth;
    dragRef.current = { edge, pointerId: e.pointerId, startX: e.clientX, start, last: start };
    document.body.style.cursor = 'col-resize';
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    // The panel sits on the right, so a drag left widens it, and the
    // sidebar on the left, so a drag right widens it.
    const moved = e.clientX - drag.startX;
    const next = clampEdge(drag.edge, drag.start + (drag.edge === 'panel' ? -moved : moved));
    if (next === drag.last) return;
    drag.last = next;
    preview(drag.edge, next);
  };

  const endDrag = (e: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    dragRef.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    document.body.style.cursor = '';
    if (drag.last !== drag.start) keep(drag.edge, drag.last);
  };

  const onKeyDown = (edge: Edge) => (e: KeyboardEvent<HTMLDivElement>) => {
    // Left widens the panel and narrows the sidebar.
    const step = e.key === 'ArrowLeft' ? KEY_STEP : e.key === 'ArrowRight' ? -KEY_STEP : 0;
    if (step === 0) return;
    e.preventDefault();
    const next =
      edge === 'panel' ? clampEdge(edge, width + step) : clampEdge(edge, sessionsWidth - step);
    preview(edge, next);
    keep(edge, next);
  };

  const frame = {
    '--panel-w': `${width}px`,
    '--panel-col': panelOpen ? panelColumn(width, side) : '0px',
    ...(side > 0 && { '--sessions-col': `${side}px` }),
  } as CSSProperties;

  return (
    <main
      ref={rootRef}
      className="shell"
      data-panel={panelOpen ? 'open' : 'hidden'}
      data-lead={sessionsToggle === null ? undefined : sessions === null ? 'band' : 'sidebar'}
      style={frame}
      onMouseUp={onMouseUp}
    >
      {/* First, so Tab reaches it before the sidebar's rows. */}
      {sessionsToggle !== null && <div className="shell-lead">{sessionsToggle}</div>}
      {sessions !== null && <div className="shell-slot-sessions">{sessions}</div>}
      {sessionsOverlay !== null && (
        <div className="shell-sessions-overlay" style={{ width: sessionsColumn(sessionsWidth) }}>
          {sessionsOverlay}
        </div>
      )}
      {/* Beside the sidebar, so Tab reaches the line after its rows. */}
      {sessions !== null && (
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label="Sessions width"
          aria-valuemin={SESSIONS_WIDTH_MIN}
          aria-valuemax={SESSIONS_WIDTH_MAX}
          aria-valuenow={sessionsWidth}
          tabIndex={0}
          className="shell-sessions-edge"
          onPointerDown={onPointerDown('sessions')}
          onPointerMove={onPointerMove}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onDoubleClick={() => {
            const stock = clampEdge('sessions', SESSIONS_WIDTH_STOCK);
            preview('sessions', stock);
            keep('sessions', stock);
          }}
          onKeyDown={onKeyDown('sessions')}
        />
      )}
      <div className="shell-slot-band">{titleBand}</div>
      <div className="shell-slot-term">
        {snoop}
        {terminal}
      </div>
      <div className="shell-slot-input">{input}</div>
      <div className="shell-slot-status">{statusLine}</div>
      <aside ref={panelRef} className="shell-slot-panel" aria-label="Panel">
        {panel}
      </aside>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Panel width"
        aria-valuemin={floor}
        aria-valuemax={PANEL_WIDTH_MAX}
        aria-valuenow={width}
        tabIndex={panelOpen ? 0 : -1}
        className="shell-panel-edge"
        onPointerDown={onPointerDown('panel')}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={() => onPanelWidth(null)}
        onKeyDown={onKeyDown('panel')}
      />
      {children}
    </main>
  );
}
