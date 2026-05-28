import { useEffect, useRef, useState, type ReactNode } from 'react';

type Direction = 'horizontal' | 'vertical';

interface Props {
  children: ReactNode;
  /** Stable key used to persist the size in localStorage. */
  storageKey: string;
  /** Horizontal panes resize width; vertical panes resize height. */
  direction?: Direction;
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
  direction = 'horizontal',
  defaultSize,
  minSize = DEFAULT_MIN,
  maxSize = DEFAULT_MAX,
  reservePx,
  className,
  handleLabel = 'resize panel',
}: Props) {
  const [size, setSize] = useState<number>(() => loadSize(storageKey, defaultSize));
  const [viewport, setViewport] = useState<number>(() => viewportDim(direction));
  const dragStateRef = useRef<{ start: number; startSize: number } | null>(null);

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, String(size));
    } catch {
      // ignore
    }
  }, [storageKey, size]);

  useEffect(() => {
    const handler = () => setViewport(viewportDim(direction));
    window.addEventListener('resize', handler);
    return () => window.removeEventListener('resize', handler);
  }, [direction]);

  // Cap so the sibling side always has room to render. Callers
  // sitting next to the terminal use the default reserve; nested
  // splits (chat / group) override via reservePx with a smaller
  // floor since they share width with the chat half, not the
  // whole window.
  const reserve =
    reservePx ?? (direction === 'horizontal' ? RESERVE_FOR_TERMINAL : RESERVE_VERTICAL);
  const effectiveMax = Math.max(minSize, Math.min(maxSize, viewport - reserve));
  const clamped = Math.max(minSize, Math.min(effectiveMax, size));
  const isVertical = direction === 'vertical';

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
    // Panel sits at the right (horizontal) or bottom (vertical) edge,
    // so moving the cursor toward the trailing edge shrinks it.
    const current = isVertical ? event.clientY : event.clientX;
    const next = drag.startSize - (current - drag.start);
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
    const shrinkKeys = isVertical ? ['ArrowDown'] : ['ArrowRight'];
    const growKeys = isVertical ? ['ArrowUp'] : ['ArrowLeft'];
    if (growKeys.includes(event.key)) {
      event.preventDefault();
      setSize((w) => Math.min(effectiveMax, w + step));
    } else if (shrinkKeys.includes(event.key)) {
      event.preventDefault();
      setSize((w) => Math.max(minSize, w - step));
    }
  };

  const wrapperStyle: React.CSSProperties = isVertical ? { height: clamped } : { width: clamped };

  return (
    <div className={`resizable resizable-${direction} ${className ?? ''}`} style={wrapperStyle}>
      <div
        className={`resizable-handle resizable-handle-${direction}`}
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
