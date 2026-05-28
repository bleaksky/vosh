import { useEffect, useRef, useState, type ReactNode } from 'react';

type Direction = 'horizontal' | 'vertical';

/** Which edge of the parent the panel anchors to. The drag handle sits
 *  on the opposite edge (the edge facing the sibling content), and the
 *  drag math flips sign so dragging toward the anchor always grows. */
type Anchor = 'left' | 'right' | 'top' | 'bottom';

interface Props {
  children: ReactNode;
  /** Stable key used to persist the size in localStorage. */
  storageKey: string;
  /** Which edge of the parent the panel sits against. Determines which
   *  side the drag handle appears on and the direction. Defaults to
   *  `right` (legacy behavior: panel on the right, handle on the left). */
  anchor?: Anchor;
  defaultSize: number;
  minSize?: number;
  maxSize?: number;
  /**
   * Viewport pixels reserved for whatever sits on the OTHER side
   * of this panel. The panel's effective max is capped to
   * `viewport - reservePx` so the sibling content always has room.
   * Defaults to a value tuned for the map-pane case (sibling = the
   * terminal); pass a smaller value when used in a nested split
   * (chat/group divider, etc) so the panel can grow further.
   */
  reservePx?: number;
  /** Extra class for the wrapper. */
  className?: string;
  /** ARIA label for the drag handle. */
  handleLabel?: string;
  /** Direction. Optional — derived from `anchor` when present. Kept
   *  for legacy callers that supplied direction without anchor. */
  direction?: Direction;
}

const DEFAULT_MIN = 80;
const DEFAULT_MAX = 1200;

// Reserve at least this many pixels of width for the terminal column
// when used in horizontal mode — roughly 75 monospace columns at 14px
// BerkeleyMono Nerd Font plus the terminal pane's own padding.
const RESERVE_FOR_TERMINAL = 700;
// Reserve at least this many pixels of vertical space for the terminal
// area when used in vertical mode. Smaller than the horizontal reserve
// since chat panes are usually shorter than they are tall.
const RESERVE_VERTICAL = 220;

function loadSize(key: string, fallback: number): number {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    const n = Number(raw);
    return Number.isFinite(n) && n > 0 ? n : fallback;
  } catch {
    return fallback;
  }
}

function viewportDim(direction: Direction): number {
  if (typeof window === 'undefined') return 1280;
  return direction === 'horizontal' ? window.innerWidth : window.innerHeight;
}

/**
 * Resizable panel wrapper. In `horizontal` mode (default) the panel
 * sits at the right edge of its parent and a 4px handle on its left
 * widens/narrows it. In `vertical` mode the panel sits at the bottom
 * of its parent and a 4px handle on its top grows/shrinks its height.
 * Size persists per `storageKey` so the choice survives reloads.
 */
export function Resizable({
  children,
  storageKey,
  anchor,
  direction,
  defaultSize,
  minSize = DEFAULT_MIN,
  maxSize = DEFAULT_MAX,
  reservePx,
  className,
  handleLabel = 'resize panel',
}: Props) {
  // Anchor is the source of truth. When omitted, derive from the
  // legacy `direction` prop: horizontal -> right, vertical -> bottom.
  const effectiveAnchor: Anchor = anchor ?? (direction === 'vertical' ? 'bottom' : 'right');
  const effectiveDirection: Direction =
    effectiveAnchor === 'top' || effectiveAnchor === 'bottom' ? 'vertical' : 'horizontal';
  const isVertical = effectiveDirection === 'vertical';
  // Trailing anchor (right or bottom): dragging the cursor away from
  // the anchor (toward the leading edge) grows the panel. Leading
  // anchor (left or top): dragging away from anchor (toward trailing
  // edge) grows. So the sign is opposite for leading anchors.
  const isLeadingAnchor = effectiveAnchor === 'left' || effectiveAnchor === 'top';
  // Handle sits on the edge opposite the anchor.
  const handleSide: 'left' | 'right' | 'top' | 'bottom' =
    effectiveAnchor === 'left'
      ? 'right'
      : effectiveAnchor === 'right'
        ? 'left'
        : effectiveAnchor === 'top'
          ? 'bottom'
          : 'top';

  const [size, setSize] = useState<number>(() => loadSize(storageKey, defaultSize));
  const [viewport, setViewport] = useState<number>(() => viewportDim(effectiveDirection));
  const dragStateRef = useRef<{ start: number; startSize: number } | null>(null);

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, String(size));
    } catch {
      // ignore
    }
  }, [storageKey, size]);

  useEffect(() => {
    const handler = () => setViewport(viewportDim(effectiveDirection));
    window.addEventListener('resize', handler);
    return () => window.removeEventListener('resize', handler);
  }, [effectiveDirection]);

  const reserve = reservePx ?? (isVertical ? RESERVE_VERTICAL : RESERVE_FOR_TERMINAL);
  const effectiveMax = Math.max(minSize, Math.min(maxSize, viewport - reserve));
  const clamped = Math.max(minSize, Math.min(effectiveMax, size));

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const target = event.currentTarget;
    target.setPointerCapture(event.pointerId);
    const start = isVertical ? event.clientY : event.clientX;
    dragStateRef.current = { start, startSize: clamped };
    document.body.style.cursor = isVertical ? 'row-resize' : 'col-resize';
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragStateRef.current;
    if (!drag) return;
    const current = isVertical ? event.clientY : event.clientX;
    const delta = current - drag.start;
    // Leading-anchor panel: cursor moves AWAY from anchor (positive
    // delta on horizontal-left or vertical-top) grows the panel.
    // Trailing-anchor: cursor moving INTO the panel shrinks it.
    const next = isLeadingAnchor ? drag.startSize + delta : drag.startSize - delta;
    const bounded = Math.max(minSize, Math.min(effectiveMax, next));
    setSize(bounded);
  };

  const endDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const target = event.currentTarget;
    if (target.hasPointerCapture(event.pointerId)) {
      target.releasePointerCapture(event.pointerId);
    }
    dragStateRef.current = null;
    document.body.style.cursor = '';
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 64 : 16;
    // Grow keys point AWAY from anchor (toward the sibling content);
    // shrink keys point INTO the panel.
    const growKey = isVertical
      ? isLeadingAnchor
        ? 'ArrowDown'
        : 'ArrowUp'
      : isLeadingAnchor
        ? 'ArrowRight'
        : 'ArrowLeft';
    const shrinkKey = isVertical
      ? isLeadingAnchor
        ? 'ArrowUp'
        : 'ArrowDown'
      : isLeadingAnchor
        ? 'ArrowLeft'
        : 'ArrowRight';
    if (event.key === growKey) {
      event.preventDefault();
      setSize((w) => Math.min(effectiveMax, w + step));
    } else if (event.key === shrinkKey) {
      event.preventDefault();
      setSize((w) => Math.max(minSize, w - step));
    }
  };

  const wrapperStyle: React.CSSProperties = isVertical ? { height: clamped } : { width: clamped };

  return (
    <div
      className={`resizable resizable-${effectiveDirection} resizable-anchor-${effectiveAnchor} ${className ?? ''}`}
      style={wrapperStyle}
    >
      <div
        className={`resizable-handle resizable-handle-${effectiveDirection} resizable-handle-${handleSide}`}
        role="separator"
        aria-orientation={isVertical ? 'horizontal' : 'vertical'}
        aria-label={handleLabel}
        tabIndex={0}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onKeyDown={handleKeyDown}
      />
      <div className="resizable-content">{children}</div>
    </div>
  );
}
