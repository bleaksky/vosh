import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { useEscape } from '../../lib/escapeStack';

// A floating menu in the One Window recipe (SPEC 3 and 7): raised
// ground, radius 16, the floating shadow, 6 px padding, 30 px rows.
// It renders into document.body with role="menu", which is also what
// tells the Windows and Linux native terminal surface to step aside
// while a menu is open.
//
// The surface owns placement (kept inside the window, flipped when it
// would run off an edge), roving focus with the arrow keys, and
// closing on Esc (through the escape stack), Tab, a click outside, a
// resize, or the window losing focus. What the rows do is up to the
// caller.

/** Where the menu wants to sit, in viewport pixels. */
export interface MenuPlacement {
  /** Left edge. */
  x: number;
  /** Top edge. */
  y: number;
  /** Right edge to use instead when the menu does not fit at `x`. */
  flipX?: number;
  /** Bottom edge to use instead when the menu does not fit at `y`. */
  flipY?: number;
}

export type MenuCloseReason = 'escape' | 'outside' | 'left';

// Space kept between a menu and the window edge.
const EDGE = 8;

const ITEM_SELECTOR = ':scope > li > [role="menuitem"]:not([aria-disabled="true"])';

interface Props {
  label: string;
  at: MenuPlacement;
  onClose: (reason: MenuCloseReason) => void;
  children: ReactNode;
  className?: string;
  /** The button that opened the menu. A press on it is left to its
   *  own toggle instead of counting as a click outside. */
  anchor?: HTMLElement | null;
  /** A submenu. Its parent handles clicks outside and window events,
   *  and ArrowLeft closes it. */
  nested?: boolean;
  /** Focus the first row once placed. A submenu opened by hover
   *  leaves focus on its parent row. */
  autoFocus?: boolean;
  id?: string;
}

export function MenuSurface({
  label,
  at,
  onClose,
  children,
  className,
  anchor,
  nested,
  autoFocus = true,
  id,
}: Props) {
  const ref = useRef<HTMLMenuElement | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  // Measure, then place, before the first paint so the menu never
  // flashes at the wrong spot.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let left = at.x;
    if (left + w > vw - EDGE && at.flipX !== undefined) left = at.flipX - w;
    left = Math.max(EDGE, Math.min(left, vw - EDGE - w));
    let top = at.y;
    if (top + h > vh - EDGE && at.flipY !== undefined) top = at.flipY - h;
    top = Math.max(EDGE, Math.min(top, vh - EDGE - h));
    setPos({ left: Math.round(left), top: Math.round(top) });
  }, [at.x, at.y, at.flipX, at.flipY]);

  // Focus the first row once placed, the way a native menu opens with
  // its first item ready for Return.
  useEffect(() => {
    if (!pos || !autoFocus) return;
    const el = ref.current;
    if (!el || el.contains(document.activeElement)) return;
    el.querySelector<HTMLElement>(ITEM_SELECTOR)?.focus();
  }, [pos, autoFocus]);

  useEffect(() => {
    if (nested) return;
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node | null;
      if (!target) return;
      if (target instanceof Element && target.closest('[data-menu-surface]')) return;
      if (anchor && anchor.contains(target)) return;
      onCloseRef.current('outside');
    };
    const onDismiss = () => onCloseRef.current('outside');
    document.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('resize', onDismiss);
    window.addEventListener('blur', onDismiss);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('resize', onDismiss);
      window.removeEventListener('blur', onDismiss);
    };
  }, [nested, anchor]);

  // Esc goes through the escape stack, so it closes this menu before
  // anything opened under it (a submenu closes before its parent).
  useEscape(true, () => onCloseRef.current('escape'));

  const onKeyDown = (e: KeyboardEvent<HTMLMenuElement>) => {
    const el = ref.current;
    if (!el) return;
    const items = Array.from(el.querySelectorAll<HTMLElement>(ITEM_SELECTOR));
    const at = items.indexOf(document.activeElement as HTMLElement);
    const focus = (i: number) => items[(i + items.length) % items.length]?.focus();
    switch (e.key) {
      case 'ArrowDown':
        focus(at + 1);
        break;
      case 'ArrowUp':
        focus(at < 0 ? items.length - 1 : at - 1);
        break;
      case 'Home':
        focus(0);
        break;
      case 'End':
        focus(items.length - 1);
        break;
      case 'Tab':
        onClose('escape');
        break;
      case 'ArrowLeft':
        if (!nested) return;
        onClose('left');
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  };

  return createPortal(
    <menu
      ref={ref}
      id={id}
      role="menu"
      aria-label={label}
      data-menu-surface=""
      className={`pane-menu${className ? ` ${className}` : ''}`}
      style={
        pos ? { left: pos.left, top: pos.top } : { left: at.x, top: at.y, visibility: 'hidden' }
      }
      onKeyDown={onKeyDown}
    >
      {children}
    </menu>,
    document.body,
  );
}

interface ItemProps {
  children: ReactNode;
  onSelect?: () => void;
  disabled?: boolean;
  /** Right aligned: a check, a chevron, a shortcut. */
  trailing?: ReactNode;
  /** Menu attributes for a row that opens a submenu. */
  submenu?: { open: boolean; controls: string; onOpen: (focusFirst: boolean) => void };
  /** Pointer entered the row. Rows without a submenu use it to close
   *  a sibling's submenu. */
  onHover?: () => void;
  itemRef?: (el: HTMLButtonElement | null) => void;
}

/** One 30 px row. Hovering focuses it, so the pointer and the arrow
 *  keys share one highlight. */
export function MenuItem({
  children,
  onSelect,
  disabled,
  trailing,
  submenu,
  onHover,
  itemRef,
}: ItemProps) {
  return (
    <li role="none">
      <button
        ref={itemRef}
        type="button"
        role="menuitem"
        className="pane-menu-item"
        aria-disabled={disabled || undefined}
        aria-haspopup={submenu ? 'menu' : undefined}
        aria-expanded={submenu ? submenu.open : undefined}
        aria-controls={submenu?.open ? submenu.controls : undefined}
        tabIndex={-1}
        onPointerMove={(e) => {
          if (disabled) return;
          if (document.activeElement !== e.currentTarget) e.currentTarget.focus();
        }}
        onPointerEnter={() => {
          if (disabled) return;
          if (submenu) submenu.onOpen(false);
          else onHover?.();
        }}
        onKeyDown={(e) => {
          if (submenu && (e.key === 'ArrowRight' || e.key === 'Enter' || e.key === ' ')) {
            e.preventDefault();
            e.stopPropagation();
            submenu.onOpen(true);
          }
        }}
        onClick={() => {
          if (disabled) return;
          if (submenu) submenu.onOpen(true);
          else onSelect?.();
        }}
      >
        <span className="pane-menu-text">{children}</span>
        {trailing}
      </button>
    </li>
  );
}

export function MenuSeparator() {
  return <li role="separator" className="pane-menu-sep" />;
}
