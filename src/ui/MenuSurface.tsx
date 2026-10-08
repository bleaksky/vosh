import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  type RefObject,
} from 'react';
import { createPortal } from 'react-dom';
import { useEscape } from '../lib/escapeStack';
import { shortcutLabel } from '../lib/shortcuts';
import { pointAt, pointerLeft, trackMenuPointer } from './menuAim';
import { placeMenu, type MenuPlacement, type MenuPlacer, type MenuSpot } from './menuPlacement';

export type { MenuPlacement, MenuPlacer } from './menuPlacement';

// The one floating menu in the One Window recipe (SPEC 3 and 7):
// raised ground, radius 16, the floating shadow, 6 px padding, 30 px
// rows, 232 to 320 wide from its longest row. It renders into
// document.body with role="menu", or role="dialog" while it holds a
// form.
//
// The surface owns placement (kept inside the window, flipped when it
// would run off an edge), roving focus with the arrow keys, and
// closing on Esc (through the escape stack), Tab, a click outside, a
// resize, or the window losing focus. A menu hung from the title band
// follows its button instead of closing on a resize or a blur. What
// the rows do is up to the caller.

export type MenuCloseReason = 'escape' | 'outside' | 'left';

// The rows you can move to: menuitem, menuitemcheckbox and
// menuitemradio, at any depth. A submenu renders into the body apart
// from its parent, so its rows are never caught here.
const ITEM_SELECTOR = '[role^="menuitem"]:not([aria-disabled="true"]):not(:disabled)';

// What a dialog focuses first.
const FIELD_SELECTOR = 'input, select, button';

// The fields of a placement, none when a placer places the menu.
const NO_SPOT: Partial<MenuPlacement> = {};

interface Props {
  label: string;
  /** Where the menu wants to sit, or a placer that works it out from
   *  the menu's size and the window's. */
  at: MenuPlacement | MenuPlacer;
  onClose: (reason: MenuCloseReason) => void;
  children: ReactNode;
  className?: string;
  /** The button that opened the menu. A press on it is left to its
   *  own toggle instead of counting as a click outside. */
  anchor?: HTMLElement | null;
  /** Hung from `anchor` in the title band. It follows the button when
   *  the window or the button's slot changes size, and stays open on a
   *  resize or when the window loses focus. */
  anchored?: boolean;
  /** A submenu. Its parent handles clicks outside and window events,
   *  and ArrowLeft closes it. */
  nested?: boolean;
  /** Take focus once placed. A submenu opened by hover leaves focus on
   *  its parent row. */
  autoFocus?: boolean;
  /** Where focus lands: the first row, or the menu itself with no row
   *  lit until an arrow key lights one. */
  focus?: 'first' | 'surface';
  /** A form instead of commands. It focuses its first field and leaves
   *  the arrow keys and Tab to the fields. */
  kind?: 'menu' | 'dialog';
  /** A fixed width. The session menu alone sets one. */
  width?: number;
  /** Keep every key but Esc and the Cmd or Ctrl shortcuts from reaching
   *  what sits under the menu in the React tree, as a card does. */
  keepKeys?: boolean;
  id?: string;
}

export function MenuSurface({
  label,
  at,
  onClose,
  children,
  className,
  anchor,
  anchored,
  nested,
  autoFocus = true,
  focus = 'first',
  kind = 'menu',
  width,
  keepKeys,
  id,
}: Props) {
  const ref = useRef<HTMLElement | null>(null);
  const [pos, setPos] = useState<MenuSpot | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const placeRef = useRef(() => {});

  // Read field by field, so a new object with the same numbers does not
  // place the menu again.
  const placer = typeof at === 'function' ? at : null;
  const { x, y, flipX, flipY, preferFlip } = typeof at === 'function' ? NO_SPOT : at;

  // Measure, then place, before the first paint so the menu never
  // flashes at the wrong spot.
  useLayoutEffect(() => {
    const place = () => {
      const el = ref.current;
      if (!el) return;
      const size = { width: el.offsetWidth, height: el.offsetHeight };
      const viewport = { width: window.innerWidth, height: window.innerHeight };
      const next: MenuSpot = placer
        ? placer(size, viewport)
        : placeMenu(
            {
              x: x ?? 0,
              y: y ?? 0,
              ...(flipX !== undefined && { flipX }),
              ...(flipY !== undefined && { flipY }),
              ...(preferFlip !== undefined && { preferFlip }),
            },
            size.width,
            size.height,
            viewport.width,
            viewport.height,
          );
      // A placer made fresh on each render places again each time, so
      // the same spot keeps the state as it is.
      setPos((prev) =>
        prev &&
        prev.left === next.left &&
        prev.top === next.top &&
        prev.maxHeight === next.maxHeight
          ? prev
          : next,
      );
    };
    placeRef.current = place;
    place();
  }, [placer, x, y, flipX, flipY, preferFlip]);

  // A menu hung from the title band follows its button. The button
  // moves with the window, and with the column it sits over when the
  // panel or the sessions sidebar comes or goes.
  useEffect(() => {
    if (!anchored) return;
    const place = () => placeRef.current();
    window.addEventListener('resize', place);
    const slot = anchor?.parentElement;
    const watch = slot && typeof ResizeObserver !== 'undefined' ? new ResizeObserver(place) : null;
    if (slot) watch?.observe(slot);
    return () => {
      window.removeEventListener('resize', place);
      watch?.disconnect();
    };
  }, [anchored, anchor]);

  // Focus the first row once placed, the way a native menu opens with
  // its first item ready for Return. A form focuses its first field
  // unless it put the caret in one of its own.
  useEffect(() => {
    if (!pos || !autoFocus) return;
    const el = ref.current;
    if (!el || el.contains(document.activeElement)) return;
    if (kind === 'dialog') el.querySelector<HTMLElement>(FIELD_SELECTOR)?.focus();
    else if (focus === 'surface') el.focus({ preventScroll: true });
    else el.querySelector<HTMLElement>(ITEM_SELECTOR)?.focus();
  }, [pos, autoFocus, focus, kind]);

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
    if (!anchored) {
      window.addEventListener('resize', onDismiss);
      window.addEventListener('blur', onDismiss);
    }
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('resize', onDismiss);
      window.removeEventListener('blur', onDismiss);
    };
  }, [nested, anchor, anchored]);

  // Esc goes through the escape stack, so it closes this menu before
  // anything opened under it (a submenu closes before its parent).
  useEscape(true, () => onCloseRef.current('escape'));

  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    // React hands a key pressed in the menu to whatever renders it, even
    // from the body, so a menu in a card holds its keys back. Esc
    // closes it from the window, and Cmd and Ctrl shortcuts pass.
    if (keepKeys && e.key !== 'Escape' && !e.metaKey && !e.ctrlKey) e.stopPropagation();
    if (kind === 'dialog') return;
    const el = ref.current;
    if (!el) return;
    const items = Array.from(el.querySelectorAll<HTMLElement>(ITEM_SELECTOR));
    const at = items.indexOf(document.activeElement as HTMLElement);
    const move = (i: number) => items[(i + items.length) % items.length]?.focus();
    switch (e.key) {
      case 'ArrowDown':
        move(at + 1);
        break;
      case 'ArrowUp':
        move(at < 0 ? items.length - 1 : at - 1);
        break;
      case 'Home':
        move(0);
        break;
      case 'End':
        move(items.length - 1);
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

  const style = {
    ...(pos
      ? {
          left: pos.left,
          top: pos.top,
          ...(pos.maxHeight !== undefined && {
            maxHeight: pos.maxHeight,
            overflowY: 'auto' as const,
          }),
        }
      : { left: x ?? 0, top: y ?? 0, visibility: 'hidden' as const }),
    ...(width !== undefined && { width }),
  };
  const shared = {
    id,
    'aria-label': label,
    'data-menu-surface': '',
    'data-menu-nested': nested ? '' : undefined,
    className: `menu${className ? ` ${className}` : ''}`,
    style,
    onKeyDown,
    // A click in the menu stays here, so click to type under it does not
    // pull focus to the command line.
    onMouseUp: (e: { stopPropagation: () => void }) => e.stopPropagation(),
  };

  return createPortal(
    kind === 'dialog' ? (
      <div ref={ref as RefObject<HTMLDivElement>} role="dialog" {...shared}>
        {children}
      </div>
    ) : (
      <menu
        ref={ref as RefObject<HTMLMenuElement>}
        role="menu"
        tabIndex={focus === 'surface' ? -1 : undefined}
        {...shared}
      >
        {children}
      </menu>
    ),
    document.body,
  );
}

interface ItemProps {
  children: ReactNode;
  onSelect?: () => void;
  disabled?: boolean;
  /** A row that toggles, read out as checked or not. With `radio`,
   *  the one row of its group that is picked. */
  checked?: boolean;
  /** One of a group of rows where one is picked, read out as a radio. */
  radio?: boolean;
  /** A row that removes or ends something, drawn in the danger tone. */
  danger?: boolean;
  /** The row's shortcut, as a spec like `Mod+K`, drawn at the right in
   *  the platform's own glyphs. */
  keys?: string;
  /** Right aligned after the shortcut: a check or a chevron. */
  trailing?: ReactNode;
  /** Menu attributes for a row that opens a submenu. With `onClose`,
   *  ArrowLeft on the row shuts a submenu the pointer opened while
   *  focus stays on the row. */
  submenu?: {
    open: boolean;
    controls: string;
    onOpen: (focusFirst: boolean) => void;
    onClose?: () => void;
  };
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
  radio,
  danger,
  keys,
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
        role={radio ? 'menuitemradio' : checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
        aria-checked={radio ? checked === true : checked}
        className={danger ? 'menu-item is-danger' : 'menu-item'}
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
          } else if (submenu?.open && submenu.onClose && e.key === 'ArrowLeft') {
            e.preventDefault();
            e.stopPropagation();
            submenu.onClose();
          }
        }}
        onClick={() => {
          if (disabled) return;
          if (submenu) submenu.onOpen(true);
          else onSelect?.();
        }}
      >
        {/* The label comes first, where the coach finds a row by name. */}
        <span className="menu-label">{children}</span>
        {keys && <kbd className="menu-keys">{shortcutLabel(keys)}</kbd>}
        {trailing}
      </button>
    </li>
  );
}

export function MenuSeparator() {
  return <li role="separator" className="menu-sep" />;
}
