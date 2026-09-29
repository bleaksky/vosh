import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { useEscape } from '../../lib/escapeStack';

// The floating menu the title band opens: the session menu and Add a
// pane. SPEC 7 menu recipe on the SPEC 3 floating ground. It hangs 12
// below its button (the session popover's 28 to 40), stays 8 inside
// the window, closes on Esc through the escape stack or on a press
// outside it, and moves focus with the arrow keys. The role (menu, or
// dialog while it holds a form) is what tells the Windows and Linux
// on-top terminal to step aside while it is open.

const GAP_BELOW_ANCHOR = 12;
const WINDOW_INSET = 8;

interface Props {
  /** The button that opened the menu. Presses on it are left to its
   *  own toggle. */
  anchor: HTMLElement | null;
  /** Center under the button, or line up the right edges. */
  align: 'center' | 'end';
  width: number;
  label: string;
  /** `dialog` while the surface holds a form instead of commands. */
  kind?: 'menu' | 'dialog';
  onClose: () => void;
  children: ReactNode;
}

const ITEM_SELECTOR = '[role="menuitem"]:not(:disabled)';

function placeUnder(
  anchor: HTMLElement | null,
  align: 'center' | 'end',
  width: number,
): { left: number; top: number } {
  if (!anchor) return { left: WINDOW_INSET, top: WINDOW_INSET };
  const r = anchor.getBoundingClientRect();
  const ideal = align === 'center' ? r.left + r.width / 2 - width / 2 : r.right - width;
  const max = window.innerWidth - width - WINDOW_INSET;
  return {
    left: Math.round(Math.max(WINDOW_INSET, Math.min(ideal, max))),
    top: Math.round(r.bottom + GAP_BELOW_ANCHOR),
  };
}

export function ShellMenu({
  anchor,
  align,
  width,
  label,
  kind = 'menu',
  onClose,
  children,
}: Props) {
  const ref = useRef<HTMLDivElement | null>(null);
  // Placed from the button on the first render, so the surface is
  // visible and focusable from its first frame.
  const [pos, setPos] = useState(() => placeUnder(anchor, align, width));
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  useEscape(true, onClose);

  useLayoutEffect(() => {
    const place = () => setPos(placeUnder(anchor, align, width));
    place();
    window.addEventListener('resize', place);
    return () => window.removeEventListener('resize', place);
  }, [anchor, align, width]);

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
  // keyboard lands inside the surface it just opened.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
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

  return (
    <div
      ref={ref}
      role={kind}
      aria-label={label}
      className="shell-menu"
      style={{ left: pos.left, top: pos.top, width }}
      onKeyDown={onKeyDown}
    >
      {children}
    </div>
  );
}

interface ItemProps {
  children: ReactNode;
  /** Shortcut label drawn at the right, like ⌘R. */
  shortcut?: string;
  danger?: boolean;
  disabled?: boolean;
  onSelect: () => void;
}

export function ShellMenuItem({ children, shortcut, danger, disabled, onSelect }: ItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      className={`shell-menu-item${danger ? ' is-danger' : ''}`}
      disabled={disabled}
      onClick={onSelect}
    >
      <span className="shell-menu-label">{children}</span>
      {shortcut && <kbd className="shell-menu-kbd">{shortcut}</kbd>}
    </button>
  );
}

export function ShellMenuSeparator() {
  return <div role="separator" className="shell-menu-sep" />;
}
