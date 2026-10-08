import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { useEscape } from '../lib/escapeStack';
import { placeMenu } from '../ui/menuPlacement';

// The floating menu the title band opens: the session menu and Add a
// pane, on the shared menu recipe and floating ground. It hangs 12
// below its button (the session popover's 28 to 40), or 6 while it
// lists the sessions (34), so five rows fit whole at 720 by 450. It
// stays 8 inside the window, closes on Esc through the escape stack or
// on a press outside it, and moves focus with the arrow keys. Its role
// is menu, or dialog while it holds a form. It renders into the body,
// like the pane menus, because the band is a stacking context and the
// find bar and the scroll depth chip would paint over a menu left
// inside it. A session row's menu opens at the pointer instead, as a
// right click menu does, and rises from the pointer near the bottom of
// the window. It never runs past the window's foot: it stops 8 above
// it, and a surface with a list that scrolls, as the session popover's
// list of sessions does, gives way there.

const GAP_BELOW_ANCHOR = 12;
const GAP_BELOW_ANCHOR_LISTED = 6;
const WINDOW_INSET = 8;

/** Where the surface sits: under the button that opened it, or with its
 *  top left at the pointer that right clicked. */
type Placement =
  | {
      /** The button that opened the menu. Presses on it are left to its
       *  own toggle. */
      anchor: HTMLElement | null;
      /** Center under the button, or line up the right edges. */
      align: 'center' | 'end';
      at?: undefined;
    }
  | { at: { x: number; y: number }; anchor?: undefined; align?: undefined };

type Props = Placement & {
  width: number;
  label: string;
  /** `dialog` while the surface holds a form instead of commands. */
  kind?: 'menu' | 'dialog';
  /** It holds a list that scrolls, which takes what height is left. */
  listed?: boolean;
  onClose: () => void;
  children: ReactNode;
};

const ITEM_SELECTOR = '[role="menuitem"]:not(:disabled)';

function placeUnder(
  anchor: HTMLElement | null,
  align: 'center' | 'end',
  width: number,
  gap: number,
): { left: number; top: number } {
  if (!anchor) return { left: WINDOW_INSET, top: WINDOW_INSET };
  const r = anchor.getBoundingClientRect();
  const ideal = align === 'center' ? r.left + r.width / 2 - width / 2 : r.right - width;
  const max = window.innerWidth - width - WINDOW_INSET;
  return {
    left: Math.round(Math.max(WINDOW_INSET, Math.min(ideal, max))),
    top: Math.round(r.bottom + gap),
  };
}

export function ShellMenu({
  anchor = null,
  align = 'center',
  at,
  width,
  label,
  kind = 'menu',
  listed = false,
  onClose,
  children,
}: Props) {
  const ref = useRef<HTMLDivElement | null>(null);
  const gap = listed ? GAP_BELOW_ANCHOR_LISTED : GAP_BELOW_ANCHOR;
  const x = at?.x;
  const y = at?.y;
  // Placed on the first render, so the surface is visible and focusable
  // from its first frame. A surface at the pointer measures its height
  // before the first paint and moves up when it would run off the
  // bottom.
  const [pos, setPos] = useState(() =>
    x === undefined || y === undefined
      ? placeUnder(anchor, align, width, gap)
      : { left: x, top: y },
  );
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  useEscape(true, onClose);

  // The surface follows its button. The button moves with the window,
  // and with the column it sits over when the panel or the sessions
  // sidebar comes or goes, as a New session form's profile pick can do.
  useLayoutEffect(() => {
    const place = () =>
      setPos(
        x === undefined || y === undefined
          ? placeUnder(anchor, align, width, gap)
          : placeMenu(
              { x, y, flipY: y },
              width,
              ref.current?.offsetHeight ?? 0,
              window.innerWidth,
              window.innerHeight,
            ),
      );
    place();
    window.addEventListener('resize', place);
    const slot = anchor?.parentElement;
    const watch = slot && typeof ResizeObserver !== 'undefined' ? new ResizeObserver(place) : null;
    if (slot) watch?.observe(slot);
    return () => {
      window.removeEventListener('resize', place);
      watch?.disconnect();
    };
  }, [anchor, align, width, gap, x, y]);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (ref.current?.contains(target) || anchor?.contains(target)) return;
      onCloseRef.current();
    };
    document.addEventListener('pointerdown', onDown, true);
    return () => document.removeEventListener('pointerdown', onDown, true);
  }, [anchor]);

  // Focus the first command, or the first field of a form, so the
  // keyboard lands inside the surface it just opened. A form that put
  // the caret in a field of its own keeps it there.
  useEffect(() => {
    const el = ref.current;
    if (!el || el.contains(document.activeElement)) return;
    const first = el.querySelector<HTMLElement>(
      kind === 'menu' ? ITEM_SELECTOR : 'input, select, button',
    );
    first?.focus();
  }, [kind]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (kind !== 'menu') return;
    const items = Array.from(ref.current?.querySelectorAll<HTMLElement>(ITEM_SELECTOR) ?? []);
    if (items.length === 0) return;
    const at = items.indexOf(document.activeElement as HTMLElement);
    let next: number | null = null;
    if (e.key === 'ArrowDown') next = at < 0 ? 0 : (at + 1) % items.length;
    else if (e.key === 'ArrowUp')
      next = at < 0 ? items.length - 1 : (at - 1 + items.length) % items.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = items.length - 1;
    else if (e.key === 'Tab') {
      e.preventDefault();
      onClose();
      return;
    }
    if (next === null) return;
    e.preventDefault();
    items[next].focus();
  };

  return createPortal(
    <div
      ref={ref}
      role={kind}
      aria-label={label}
      className={listed ? 'shell-menu is-listed' : 'shell-menu'}
      style={{
        left: pos.left,
        top: pos.top,
        width,
        maxHeight: `calc(100vh - ${pos.top + WINDOW_INSET}px)`,
      }}
      onKeyDown={onKeyDown}
    >
      {children}
    </div>,
    document.body,
  );
}

interface ItemProps {
  children: ReactNode;
  /** Shortcut label drawn at the right, like ⌘R. */
  shortcut?: string | undefined;
  /** Drawn at the right before the shortcut, like the plugin a Lua
   *  pane in Add a pane comes from. */
  trailing?: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  onSelect: () => void;
}

export function ShellMenuItem({
  children,
  shortcut,
  trailing,
  danger,
  disabled,
  onSelect,
}: ItemProps) {
  const kbd = shortcut && <kbd className="shell-menu-kbd">{shortcut}</kbd>;
  return (
    <button
      type="button"
      role="menuitem"
      className={`shell-menu-item${danger ? ' is-danger' : ''}`}
      disabled={disabled}
      onClick={onSelect}
    >
      <span className="shell-menu-label">{children}</span>
      {trailing ? (
        <span className="shell-menu-side">
          {trailing}
          {kbd}
        </span>
      ) : (
        kbd
      )}
    </button>
  );
}

export function ShellMenuSeparator() {
  return <div role="separator" className="shell-menu-sep" />;
}
