import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { useEscape } from '../lib/escapeStack';
import { pointAt, pointerLeft, trackMenuPointer } from './menuAim';
import { placeMenu, type MenuPlacement } from './menuPlacement';

export type { MenuPlacement } from './menuPlacement';

// A floating menu in the One Window recipe (SPEC 3 and 7): raised
// ground, radius 16, the floating shadow, 6 px padding, 30 px rows.
// It renders into document.body with role="menu".
//
// The surface owns placement (kept inside the window, flipped when it
// would run off an edge), roving focus with the arrow keys, and
// closing on Esc (through the escape stack), Tab, a click outside, a
// resize, or the window losing focus. What the rows do is up to the
// caller.

export type MenuCloseReason = 'escape' | 'outside' | 'left';

// Rows and checkable rows alike, menuitem and menuitemcheckbox.
const ITEM_SELECTOR = ':scope > li > [role^="menuitem"]:not([aria-disabled="true"])';

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
    // Read field by field, so a new object with the same numbers does not
    // place the menu again.
    const placement: MenuPlacement = {
      x: at.x,
      y: at.y,
      ...(at.flipX !== undefined && { flipX: at.flipX }),
      ...(at.flipY !== undefined && { flipY: at.flipY }),
      ...(at.preferFlip !== undefined && { preferFlip: at.preferFlip }),
    };
    setPos(
      placeMenu(placement, el.offsetWidth, el.offsetHeight, window.innerWidth, window.innerHeight),
    );
  }, [at.x, at.y, at.flipX, at.flipY, at.preferFlip]);

  // Focus the first row once placed, the way a native menu opens with
  // its first item ready for Return.
  useEffect(() => {
    if (!pos || !autoFocus) return;
    const el = ref.current;
    if (!el || el.contains(document.activeElement)) return;
    el.querySelector<HTMLElement>(ITEM_SELECTOR)?.focus();
  }, [pos, autoFocus]);

  // Follow the pointer, so a row it crosses on its way into a submenu
  // waits (menuAim.ts).
  useEffect(() => trackMenuPointer(), []);

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
      data-menu-nested={nested ? '' : undefined}
      className={`menu${className ? ` ${className}` : ''}`}
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
  /** A row that toggles, read out as checked or not. */
  checked?: boolean;
  /** Right aligned: a check, a chevron, a shortcut. */
  trailing?: ReactNode;
  /** Menu attributes for a row that opens a submenu. */
  submenu?: { open: boolean; controls: string; onOpen: (focusFirst: boolean) => void };
  /** Pointer entered the row. Rows without a submenu use it to close
   *  a sibling's submenu. */
  onHover?: () => void;
  /** The row took focus, from the pointer or the arrow keys. A menu
   *  with several submenus uses it to close the ones this row does not
   *  open. */
  onFocus?: () => void;
  itemRef?: (el: HTMLButtonElement | null) => void;
}

/** One 30 px row. Hovering focuses it, so the pointer and the arrow
 *  keys share one highlight. A row the pointer crosses on its way into
 *  an open submenu waits until the pointer turns away or rests. */
export function MenuItem({
  children,
  onSelect,
  disabled,
  checked,
  trailing,
  submenu,
  onHover,
  onFocus,
  itemRef,
}: ItemProps) {
  // The pointer on the row gives it the highlight and opens its submenu,
  // or closes a sibling's, unless it is passing through on its way into
  // an open submenu.
  const point = (e: ReactPointerEvent<HTMLButtonElement>) => {
    if (disabled) return;
    const el = e.currentTarget;
    pointAt(el, () => {
      if (document.activeElement !== el) el.focus();
      if (submenu) submenu.onOpen(false);
      else onHover?.();
    });
  };
  return (
    <li role="none">
      <button
        ref={itemRef}
        type="button"
        role={checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
        aria-checked={checked}
        className="menu-item"
        aria-disabled={disabled || undefined}
        aria-haspopup={submenu ? 'menu' : undefined}
        aria-expanded={submenu ? submenu.open : undefined}
        aria-controls={submenu?.open ? submenu.controls : undefined}
        tabIndex={-1}
        onFocus={onFocus}
        onPointerEnter={point}
        onPointerMove={point}
        onPointerLeave={(e) => pointerLeft(e.currentTarget)}
        onKeyDown={(e) => {
          // A click focuses a button on Windows and Linux, disabled or
          // not, so the keys must check too.
          if (disabled) return;
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
        <span className="menu-label">{children}</span>
        {trailing}
      </button>
    </li>
  );
}

export function MenuSeparator() {
  return <li role="separator" className="menu-sep" />;
}
