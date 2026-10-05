import { useEffect, useRef, useState, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
  /** Stable key used to persist the size in localStorage. */
  storageKey: string;
  defaultSize: number;
  minSize?: number;
  maxSize?: number;
  /**
   * Viewport pixels reserved for whatever sits below this panel. The
   * panel's effective max is capped to `viewport - reservePx` so the
   * sibling content always has room.
   */
  reservePx?: number;
  /** Extra class for the wrapper. */
  className?: string;
  /** ARIA label for the drag handle. */
  handleLabel?: string;
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

// Reserve at least this many pixels of vertical space for the terminal
// area below the panel.
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

function viewportHeight(): number {
  if (typeof window === 'undefined') return 1280;
  return window.innerHeight;
}

/**
 * Resizable panel wrapper for the history split. The panel sits at the
 * top of its parent and a 4px handle on its bottom edge grows or
 * shrinks its height. Size persists per `storageKey` so the choice
 * survives reloads.
 */
export function Resizable({
  children,
  storageKey,
  defaultSize,
  minSize = DEFAULT_MIN,
  maxSize = DEFAULT_MAX,
  reservePx,
  className,
  handleLabel = 'resize panel',
  snapPx,
  onSizeChange,
}: Props) {
  const [size, setSize] = useState<number>(() => loadSize(storageKey, defaultSize));
  const [viewport, setViewport] = useState<number>(viewportHeight);
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
    const handler = () => setViewport(viewportHeight());
    window.addEventListener('resize', handler);
    return () => window.removeEventListener('resize', handler);
  }, []);

  const reserve = reservePx ?? RESERVE_VERTICAL;
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
    const start = event.clientY;
    dragStateRef.current = { start, startSize: clamped, lastSnapped: clamped };
    document.body.style.cursor = 'row-resize';
    // Drag visual (the stretched ember tick) is a direct class flip so
    // it lands in the same frame as the pointer capture, not a render.
    target.classList.add('is-dragging');
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragStateRef.current;
    if (!drag) return;
    // The panel hangs from the top, so dragging the handle down grows it.
    const delta = event.clientY - drag.start;
    const raw = drag.startSize + delta;
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
      wrapperRef.current.style.height = `${bounded}px`;
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
    // Down points away from the top (toward the sibling content) and
    // grows the panel. Up points into the panel and shrinks it.
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setSize((w) => Math.min(effectiveMax, w + step));
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setSize((w) => Math.max(minSize, w - step));
    }
  };

  const wrapperStyle: React.CSSProperties = { height: clamped };

  return (
    <div
      ref={wrapperRef}
      className={`resizable resizable-vertical resizable-anchor-top ${className ?? ''}`}
      style={wrapperStyle}
    >
      <div
        className="resizable-handle resizable-handle-vertical resizable-handle-bottom"
        role="separator"
        aria-orientation="horizontal"
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
