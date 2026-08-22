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
  /** When provided, the size snaps to multiples of this value while
   *  dragging. Used by the split-scrollback divider so it always
   *  lands on a terminal row boundary and never clips a half-line
   *  of content. The function form lets callers compute the snap
   *  lazily from a live source (e.g. the xterm cell height). 0,
   *  negative, or undefined disables snapping. */
  snapPx?: number | (() => number);
  /** Fires synchronously every time the size changes — during
   *  pointer drag, on keyboard nudge, and once on mount with the
   *  initial value. Used by callers that need to keep a sibling
   *  element's geometry in lockstep (e.g. the split-scrollback
   *  layout writes the history height to a CSS variable so the
   *  live pane shrinks in the same paint frame). */
  onSizeChange?: (size: number) => void;
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
  snapPx,
  onSizeChange,
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
  const dragStateRef = useRef<{ start: number; startSize: number; lastSnapped: number } | null>(
    null,
  );
  const wrapperRef = useRef<HTMLDivElement | null>(null);

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

  // Forward size changes that didn't originate from pointer-drag
  // (keyboard nudge, mount with persisted value). Pointer-drag
  // already calls onSizeChange synchronously inside the move
  // handler, so callers get every drag frame. Don't dispatch
  // vosh:resize-progress here — Terminal.tsx's ResizeObserver
  // handles non-drag size changes the next frame, and broadcasting
  // here in addition to from pointermove makes the event fire
  // twice per drag frame (once sync from the move handler, once
  // async after React commits the state). Sibling consumers refit
  // against subtly different dimensions on each fire and the
  // pane wobbles.
  useEffect(() => {
    onSizeChange?.(clamped);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [clamped]);

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    // Primary button only. A right-click capture would race the native
    // context menu for the pointerup and leave a stuck drag state.
    if (event.button !== 0) return;
    event.preventDefault();
    const target = event.currentTarget;
    target.setPointerCapture(event.pointerId);
    const start = isVertical ? event.clientY : event.clientX;
    dragStateRef.current = { start, startSize: clamped, lastSnapped: clamped };
    document.body.style.cursor = isVertical ? 'row-resize' : 'col-resize';
    // Drag visual (the stretched ember tick) is a direct class flip so
    // it lands in the same frame as the pointer capture, not a render.
    target.classList.add('is-dragging');
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragStateRef.current;
    if (!drag) return;
    const current = isVertical ? event.clientY : event.clientX;
    const delta = current - drag.start;
    const raw = isLeadingAnchor ? drag.startSize + delta : drag.startSize - delta;
    const snap = typeof snapPx === 'function' ? snapPx() : snapPx;
    // Hysteresis around the snap boundary. With a plain Math.round
    // the cursor sitting near a row threshold flips the wrapper
    // between two adjacent snap values every pointermove and the
    // text under the divider visibly jitters up and down. Require
    // the cursor to move >60% of a snap step away from the
    // currently-snapped value before we commit to a new one.
    let snapped = raw;
    if (snap && snap > 0) {
      const candidate = Math.round(raw / snap) * snap;
      const last = drag.lastSnapped;
      if (Math.abs(candidate - last) <= snap / 2 + 0.01) {
        // Candidate is the same snap target as `last` (or differs
        // by exactly one step at the half-boundary). Apply
        // hysteresis: only switch if the raw cursor is well past
        // the dead zone around `last`.
        snapped = Math.abs(raw - last) > snap * 0.6 ? candidate : last;
      } else {
        snapped = candidate;
      }
      drag.lastSnapped = snapped;
    }
    const bounded = Math.max(minSize, Math.min(effectiveMax, snapped));
    // Update the wrapper synchronously via direct style assignment
    // and broadcast the new size so listeners (Terminal) can fit +
    // anchor in the same task. Going through React state would
    // schedule an async render, and the wrapper resize would land
    // in a different paint from the xterm fit — that staggered
    // sequence is what made every previous drag jitter. The state
    // setter still fires so React tracks the value, but the DOM
    // is already at the right size by the time React commits.
    if (wrapperRef.current) {
      if (isVertical) {
        wrapperRef.current.style.height = `${bounded}px`;
      } else {
        wrapperRef.current.style.width = `${bounded}px`;
      }
    }
    // onSizeChange first so any sibling-coupling work (e.g. the
    // split-scrollback CSS variable that resizes the live pane)
    // lands BEFORE the event listeners read fresh dimensions.
    // Reverse order would leave live's onResizeProgress reading
    // stale wrapper geometry on every drag frame and the live
    // pane would jitter against the dragged history pane.
    onSizeChange?.(bounded);
    window.dispatchEvent(new CustomEvent('vosh:resize-progress', { detail: { size: bounded } }));
    setSize(bounded);
  };

  const endDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const target = event.currentTarget;
    if (target.hasPointerCapture(event.pointerId)) {
      target.releasePointerCapture(event.pointerId);
    }
    dragStateRef.current = null;
    document.body.style.cursor = '';
    target.classList.remove('is-dragging');
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
      ref={wrapperRef}
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
