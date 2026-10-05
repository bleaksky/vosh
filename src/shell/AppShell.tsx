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

// The One Window frame (SPEC 1). A CSS grid with the terminal column
// and the panel column. Rows are the 32 px title band, the terminal,
// the input band (at least 40, taller while you compose several
// lines), and the 28 px status line. The band spans both columns and
// the panel spans every row, so both grounds run to the top edge.
//
// Every slot renders at a fixed position in a fixed parent, and the
// panel toggle and the width drag only change the column template
// through two custom properties on the root. So showing, hiding, or
// resizing the panel never moves the Terminal or the Input to a new
// parent, and neither remounts. A remount would reload the scrollback,
// drop the underlay's pointer forwarding, and lose command history.
//
// The root also publishes --panel-w (the panel's width, open or not)
// and --panel-col (the width the panel takes, 0 while hidden) for the
// panel and for overlays that keep clear of it.
//
// On Windows and Linux the panel draws at least 248 px wide, so the
// title band's buttons and window controls all sit over it. A saved
// width under that stays saved, and macOS draws it as saved.

/** The terminal keeps at least this much width when you drag the
 *  panel wider. */
const MIN_TERMINAL_WIDTH = 320;
/** Arrow keys on the panel edge move it this far. */
const KEY_STEP = 8;

interface Props {
  panelOpen: boolean;
  /** The saved panel width in CSS pixels. The panel draws no
   *  narrower than panelWidthFloor. */
  panelWidth: number;
  /** Save a panel width after a drag or a key press. Null goes back to
   *  the stock width. */
  onPanelWidth: (px: number | null) => void;
  titleBand: ReactNode;
  terminal: ReactNode;
  input: ReactNode;
  statusLine: ReactNode;
  panel: ReactNode;
  onMouseUp?: (event: MouseEvent<HTMLElement>) => void;
  /** Floating surfaces: menus, the palette, toasts, dialogs. */
  children?: ReactNode;
}

function clampWidth(px: number, floor: number): number {
  const room = window.innerWidth - MIN_TERMINAL_WIDTH;
  const max = Math.max(floor, Math.min(PANEL_WIDTH_MAX, room));
  return Math.round(Math.min(max, Math.max(floor, px)));
}

export function AppShell({
  panelOpen,
  panelWidth,
  onPanelWidth,
  titleBand,
  terminal,
  input,
  statusLine,
  panel,
  onMouseUp,
  children,
}: Props) {
  const floor = panelWidthFloor(isMacPlatform());
  const width = Math.max(floor, panelWidth);
  const rootRef = useRef<HTMLElement | null>(null);
  const panelRef = useRef<HTMLElement | null>(null);
  const dragRef = useRef<{ pointerId: number; startX: number; start: number; last: number } | null>(
    null,
  );

  // A hidden panel keeps its panes mounted (so the map and chat keep
  // their state) but leaves the tab order and the accessibility tree.
  useEffect(() => {
    if (panelRef.current) panelRef.current.inert = !panelOpen;
  }, [panelOpen]);

  // Drag frames write the width straight to the root and tell the
  // terminal to refit in the same frame, the way Resizable does. React
  // state catches up once, on release.
  const preview = (px: number) => {
    const root = rootRef.current;
    if (!root) return;
    root.style.setProperty('--panel-w', `${px}px`);
    root.style.setProperty('--panel-col', `min(${px}px, calc(100vw - ${MIN_TERMINAL_WIDTH}px))`);
    window.dispatchEvent(new CustomEvent('vosh:resize-progress', { detail: { size: px } }));
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = {
      pointerId: e.pointerId,
      startX: e.clientX,
      start: width,
      last: width,
    };
    document.body.style.cursor = 'col-resize';
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    const next = clampWidth(drag.start + drag.startX - e.clientX, floor);
    if (next === drag.last) return;
    drag.last = next;
    preview(next);
  };

  const endDrag = (e: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    dragRef.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    document.body.style.cursor = '';
    if (drag.last !== drag.start) onPanelWidth(drag.last);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    // The panel sits on the right, so Left widens it.
    const delta = e.key === 'ArrowLeft' ? KEY_STEP : e.key === 'ArrowRight' ? -KEY_STEP : 0;
    if (delta === 0) return;
    e.preventDefault();
    const next = clampWidth(width + delta, floor);
    preview(next);
    onPanelWidth(next);
  };

  // The column never squeezes the terminal under its floor, even when
  // the window shrinks below a width saved on a bigger screen.
  const frame = {
    '--panel-w': `${width}px`,
    '--panel-col': panelOpen ? `min(${width}px, calc(100vw - ${MIN_TERMINAL_WIDTH}px))` : '0px',
  } as CSSProperties;

  return (
    <main
      ref={rootRef}
      className="app shell"
      data-panel={panelOpen ? 'open' : 'hidden'}
      style={frame}
      onMouseUp={onMouseUp}
    >
      <div className="shell-slot-band">{titleBand}</div>
      <div className="shell-slot-term">{terminal}</div>
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
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={() => onPanelWidth(null)}
        onKeyDown={onKeyDown}
      />
      {children}
    </main>
  );
}
