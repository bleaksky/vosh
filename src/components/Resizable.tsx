import { useEffect, useRef, useState, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
  /** Stable key used to persist the width in localStorage. */
  storageKey: string;
  defaultWidth: number;
  minWidth?: number;
  maxWidth?: number;
  /** Extra class for the wrapper. */
  className?: string;
  /** ARIA label for the drag handle. */
  handleLabel?: string;
}

const DEFAULT_MIN = 220;
const DEFAULT_MAX = 1200;

// Reserve at least this many pixels of width for the terminal column —
// roughly 75 monospace columns at 14px BerkeleyMono Nerd Font plus the
// terminal pane's own padding. The Resizable wrappers honor this when
// computing their effective max so a stale persisted width can't push
// the MUD output below 75 columns.
const RESERVE_FOR_TERMINAL = 700;

function loadWidth(key: string, fallback: number): number {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    const n = Number(raw);
    return Number.isFinite(n) && n > 0 ? n : fallback;
  } catch {
    return fallback;
  }
}

function viewportWidth(): number {
  return typeof window === 'undefined' ? 1280 : window.innerWidth;
}

/**
 * Wraps a panel that lives on the right side of the layout. Renders a
 * 4 px drag handle on the left edge that lets the user widen or
 * narrow the panel by dragging. Width persists per `storageKey` so the
 * choice survives reloads.
 */
export function Resizable({
  children,
  storageKey,
  defaultWidth,
  minWidth = DEFAULT_MIN,
  maxWidth = DEFAULT_MAX,
  className,
  handleLabel = 'resize panel',
}: Props) {
  const [width, setWidth] = useState<number>(() => loadWidth(storageKey, defaultWidth));
  const [viewportW, setViewportW] = useState<number>(() => viewportWidth());
  const dragStateRef = useRef<{ startX: number; startWidth: number } | null>(null);

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, String(width));
    } catch {
      // ignore
    }
  }, [storageKey, width]);

  // Recompute the cap whenever the OS window resizes so the user
  // can't drag past the 75-char terminal floor on a smaller window.
  useEffect(() => {
    const handler = () => setViewportW(viewportWidth());
    window.addEventListener('resize', handler);
    return () => window.removeEventListener('resize', handler);
  }, []);

  // Cap so the terminal column always has at least 75 monospace cells
  // of width to render game text into.
  const effectiveMax = Math.max(
    minWidth,
    Math.min(maxWidth, viewportW - RESERVE_FOR_TERMINAL),
  );
  const clamped = Math.max(minWidth, Math.min(effectiveMax, width));

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const target = event.currentTarget;
    target.setPointerCapture(event.pointerId);
    dragStateRef.current = { startX: event.clientX, startWidth: clamped };
    document.body.style.cursor = 'col-resize';
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragStateRef.current;
    if (!drag) return;
    // Panel is on the right edge; dragging right shrinks it, left
    // widens it, so subtract the delta.
    const next = drag.startWidth - (event.clientX - drag.startX);
    const bounded = Math.max(minWidth, Math.min(effectiveMax, next));
    setWidth(bounded);
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
    if (event.key === 'ArrowLeft') {
      event.preventDefault();
      setWidth((w) => Math.min(effectiveMax, w + step));
    } else if (event.key === 'ArrowRight') {
      event.preventDefault();
      setWidth((w) => Math.max(minWidth, w - step));
    }
  };

  return (
    <div className={`resizable ${className ?? ''}`} style={{ width: clamped }}>
      <div
        className="resizable-handle"
        role="separator"
        aria-orientation="vertical"
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
