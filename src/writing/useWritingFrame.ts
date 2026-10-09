import {
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type HTMLAttributes,
} from 'react';
import type { Draft } from '../ipc/writing';
import type { PromptCardHost } from '../prompt/PromptCard';
import type { CellSize } from '../prompt/pinnedDock';
import { usePanelLayout } from '../panel/panelLayoutStore';
import { saveWritingCardPrefs, useWritingCardPrefs } from '../stores/config/writingCardStore';
import {
  BOX_COLS,
  BOX_ROWS_MIN,
  CARD_MARGIN,
  DRAG_SLOP,
  boxColsFor,
  boxRowsFor,
  boxWidthFor,
  clampPlace,
  fitCols,
  fitRows,
  fitsMoved,
  movedFit,
  savedPlace,
  startsMove,
  type Point,
} from './cardPlace';
import type { KindInfo } from './kinds';
import { pinWritingPane, useWritingSlot } from './pinnedPane';
import { useBoxSize, useWritingPlace } from './useWritingPlace';

// Where the writing card sits and how big it is: over the terminal in
// its own place, where you moved it, or pinned in its pane, with the
// rows and columns of its box. The card draws through a portal into a
// box of its own, which this moves between the window and the pane.

/** The width of one column of the terminal face at `px`. */
function columnWidth(family: string, px: number): number {
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  if (!ctx) return px * 0.6;
  ctx.font = `${px}px ${family}`;
  return ctx.measureText('0'.repeat(10)).width / 10 || px * 0.6;
}

interface FrameArgs {
  host: PromptCardHost;
  cell: CellSize | null;
  fontFamily: string;
  fontSize: number;
  /** The guide shows beside the box. */
  guideOn: boolean;
  info: KindInfo;
  draft: Draft;
  /** The room the session stands in, for a report's room field. */
  roomName: string | null;
  /** The lines in the box, which a box with no rows set grows with. */
  lineCount: number;
  folded: boolean;
  preview: boolean;
}

export function useWritingFrame({
  host,
  cell,
  fontFamily,
  fontSize,
  guideOn,
  info,
  draft,
  roomName,
  lineCount,
  folded,
  preview,
}: FrameArgs) {
  // The card is as wide as 80 columns of your terminal face. In a window
  // too narrow for that it spans the window and sets its text at 11 px
  // to keep 80 columns.
  const naturalColumn = useMemo(() => columnWidth(fontFamily, fontSize), [fontFamily, fontSize]);
  const naturalWidth = 32 + 82 * naturalColumn + 32 + (guideOn ? 248 : 0);
  const place = useWritingPlace(host, cell, naturalWidth);
  // Where you moved the card, how tall you made its box, and whether it
  // lives in its pane in the panel. A pinned card floats while the
  // panel is hidden, and goes back into its pane when the panel shows.
  const prefs = useWritingCardPrefs();
  const slot = useWritingSlot();
  const docked = prefs.pinned && slot !== null;
  // While the pane a pinned card opens into is on its way, the card waits
  // unseen rather than flash over the terminal first.
  const panel = usePanelLayout();
  const awaitingPane = prefs.pinned && slot === null && (panel === null || panel.panel_open);
  const [moving, setMoving] = useState<Point | null>(null);
  const [sizing, setSizing] = useState<number | null>(null);
  const [sizingCols, setSizingCols] = useState<number | null>(null);
  const viewW = place?.viewW ?? window.innerWidth;
  const viewH = place?.viewH ?? window.innerHeight;
  const view = { w: viewW, h: viewH };
  const at = moving ?? savedPlace(prefs.left, prefs.top);
  const roomy = fitsMoved(naturalWidth, viewW);
  const moved = !docked && at !== null && roomy;
  const narrow = !docked && !moved && place !== null && place.right !== null;
  const px = narrow ? 11 : fontSize;
  const lineH = Math.round(px * 1.3);
  const colW = useMemo(() => columnWidth(fontFamily, px), [fontFamily, px]);
  // The fields over the text: To and Subject, and a third row for the
  // language or the room when the card shows one. The rows count only
  // the fields the card draws, so a note with no language keeps the two
  // rows' height and its box the rows the window has room for.
  const fieldLanguage = info.language && draft.language !== undefined ? draft.language : null;
  const fieldRoom = info.room ? (draft.room ?? roomName) : null;
  const fieldsH = info.board ? (fieldLanguage !== null || fieldRoom !== null ? 102 : 68) : 0;
  const chrome = 46 + 1 + 1 + 52 + 12 + 16 + 10 + fieldsH;
  const cardRef = useRef<HTMLDivElement | null>(null);
  const [cardEl, setCardEl] = useState<HTMLDivElement | null>(null);
  const cardRefOf = useCallback((el: HTMLDivElement | null) => {
    cardRef.current = el;
    setCardEl(el);
  }, []);
  const cardSize = useBoxSize(cardEl);
  const slotSize = useBoxSize(docked ? slot : null);
  // A pinned header can wrap, so the rows take what it grows by.
  const headSize = useBoxSize(
    docked ? (cardEl?.querySelector<HTMLElement>('.pc-head') ?? null) : null,
  );
  const headGrew = Math.max(0, (headSize?.h ?? 46) - 46);
  // The columns the box may take: what the window or the pane leaves
  // beside the card's own 16 px each side and the guide. A narrow window
  // keeps 80 in a smaller face.
  const besideBox = 32 + (guideOn ? 248 : 0);
  const colsRoom = docked
    ? (slotSize?.w ?? 0)
    : moved
      ? viewW - 2 * CARD_MARGIN
      : viewW - (place?.left ?? 12) - 12;
  const colsFit = fitCols(colsRoom - besideBox, colW);
  const boxCols = narrow ? BOX_COLS : boxColsFor(sizingCols ?? prefs.cols, colsFit);
  const boxWidth = boxWidthFor(boxCols, colW);
  const fit = docked
    ? fitRows((slotSize?.h ?? 0) - headGrew, chrome, lineH)
    : moved
      ? movedFit(viewH, chrome, lineH)
      : place
        ? fitRows(place.maxHeight, chrome, lineH)
        : 12;
  // A pinned box fills its pane. Rows you set hold the box at that
  // height, and without them it grows with the text.
  const rowsSet = sizing ?? prefs.rows;
  const boxRows = docked ? Math.max(BOX_ROWS_MIN, fit) : boxRowsFor(rowsSet, lineCount, fit);
  const boxMinRows = docked || rowsSet !== null ? boxRows : BOX_ROWS_MIN;
  const movedAt = moved && at ? clampPlace(at, cardSize ?? { w: naturalWidth, h: 0 }, view) : null;

  // ── Moving and pinning ────────────────────────────────────────────
  /** Back to the place over the terminal the card works out itself. */
  const putBack = () => void saveWritingCardPrefs({ left: null, top: null }).catch(() => {});

  const togglePin = () => {
    void saveWritingCardPrefs({ pinned: !prefs.pinned }).catch(() => {});
    if (!prefs.pinned) pinWritingPane();
  };

  // A press on the header that travels a few pixels moves the card. It
  // stays whole in the window, and lands where you let go.
  const moveRef = useRef<{ x: number; y: number; from: Point; to: Point | null } | null>(null);
  const endMove = () => {
    const d = moveRef.current;
    moveRef.current = null;
    if (d?.to) void saveWritingCardPrefs({ left: d.to.left, top: d.to.top }).catch(() => {});
    setMoving(null);
  };
  const canMove = !docked && !folded && roomy && place !== null;
  const drag: Pick<
    HTMLAttributes<HTMLDivElement>,
    'onPointerDown' | 'onPointerMove' | 'onPointerUp' | 'onPointerCancel' | 'onDoubleClick'
  > | null = canMove
    ? {
        onPointerDown: (e) => {
          if (e.button !== 0 || !startsMove(e.target)) return;
          const box = cardRef.current?.getBoundingClientRect();
          if (!box) return;
          moveRef.current = {
            x: e.clientX,
            y: e.clientY,
            from: { left: box.left, top: box.top },
            to: null,
          };
          e.currentTarget.setPointerCapture(e.pointerId);
        },
        onPointerMove: (e) => {
          const d = moveRef.current;
          if (!d) return;
          const dx = e.clientX - d.x;
          const dy = e.clientY - d.y;
          if (d.to === null && Math.hypot(dx, dy) < DRAG_SLOP) return;
          const size = cardSize ?? { w: naturalWidth, h: 0 };
          d.to = clampPlace({ left: d.from.left + dx, top: d.from.top + dy }, size, view);
          setMoving(d.to);
        },
        onPointerUp: endMove,
        onPointerCancel: endMove,
        onDoubleClick: (e) => {
          if (startsMove(e.target) && prefs.left !== null) putBack();
        },
      }
    : null;

  // The grip sits on the edge that moves: the foot of a card you moved,
  // which hangs from its top, and the top of a card in its own place,
  // whose foot stays over the six rows above your prompt. A pinned box
  // fills its pane and takes no grip.
  const gripEdge: 'foot' | 'top' | null =
    docked || folded || preview ? null : moved ? 'foot' : 'top';

  // The card's own box, which moves between the window and the pane.
  // Focus inside it stays where it was across the move.
  const [hostEl] = useState(() => {
    const el = document.createElement('div');
    el.className = 'wr-host';
    document.body.appendChild(el);
    return el;
  });
  useLayoutEffect(() => {
    const parent = docked && slot ? slot : document.body;
    if (hostEl.parentElement === parent) return;
    const active = document.activeElement;
    const inside = active instanceof HTMLElement && hostEl.contains(active);
    parent.appendChild(hostEl);
    if (inside) active.focus({ preventScroll: true });
  }, [docked, slot, hostEl]);
  useLayoutEffect(() => () => hostEl.remove(), [hostEl]);

  return {
    place,
    prefs,
    docked,
    awaitingPane,
    moving,
    setSizing,
    setSizingCols,
    viewH,
    narrow,
    px,
    lineH,
    colW,
    fieldLanguage,
    fieldRoom,
    cardRef,
    cardRefOf,
    colsFit,
    boxCols,
    boxWidth,
    fit,
    boxRows,
    boxMinRows,
    movedAt,
    putBack,
    togglePin,
    drag,
    gripEdge,
    hostEl,
  };
}
