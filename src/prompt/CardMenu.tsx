import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { useEscape } from '../lib/escapeStack';
import { menuPosition, type MenuPlace } from './cardRules';

// A menu the prompt card opens from one of its buttons: More, Presets,
// From another profile, More styles, or the name menu on another game.
// The ov-menu recipe (radius 16, padding 6, --raised, --shadow-float),
// placed 4 px from its button and kept inside the window, on the other
// side of its button when its own side has no room (menuPosition). Esc
// closes it before the card, a press outside closes it, and the arrow
// keys move between its items. Focus moves into it as it opens and back
// to its button as it closes.

const ITEMS =
  '[role="menuitem"]:not(:disabled),[role="menuitemradio"]:not(:disabled),[role="menuitemcheckbox"]:not(:disabled)';

interface CardMenuProps {
  /** The button the menu opens from. A press on it is left to its own
   *  handler, which closes the menu. */
  anchor: HTMLElement;
  place: MenuPlace;
  width: number;
  label: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
}

export function CardMenu({
  anchor,
  place,
  width,
  label,
  onClose,
  children,
  className,
}: CardMenuProps) {
  const ref = useRef<HTMLUListElement | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number; maxHeight?: number } | null>(null);

  useEscape(true, onClose);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const rect = anchor.getBoundingClientRect();
    setPos(
      menuPosition(rect, { width, height: el.offsetHeight }, place, {
        width: window.innerWidth,
        height: window.innerHeight,
      }),
    );
  }, [anchor, place, width]);

  // Callers pass inline functions, so the listeners read the latest one.
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  // Focus moves into the menu once it is placed: until then it is
  // hidden, and a hidden element takes no focus.
  const focused = useRef(false);
  useEffect(() => {
    if (!pos || focused.current) return;
    focused.current = true;
    ref.current?.focus({ preventScroll: true });
  }, [pos]);

  // Closing, by Esc, Tab, a press outside or a choice, hands focus back
  // to the button the menu opened from, unless it went somewhere already.
  useEffect(
    () => () => {
      const active = document.activeElement;
      if ((!active || active === document.body) && anchor.isConnected) {
        anchor.focus({ preventScroll: true });
      }
    },
    [anchor],
  );

  useEffect(() => {
    const onPointer = (e: PointerEvent) => {
      const target = e.target instanceof Node ? e.target : null;
      if (!target || ref.current?.contains(target) || anchor.contains(target)) return;
      closeRef.current();
    };
    document.addEventListener('pointerdown', onPointer);
    return () => document.removeEventListener('pointerdown', onPointer);
  }, [anchor]);

  const move = (step: number | 'first' | 'last') => {
    const items = Array.from(ref.current?.querySelectorAll<HTMLElement>(ITEMS) ?? []);
    if (items.length === 0) return;
    const at = items.indexOf(document.activeElement as HTMLElement);
    let next: number;
    if (step === 'first') next = 0;
    else if (step === 'last') next = items.length - 1;
    else if (at < 0) next = step > 0 ? 0 : items.length - 1;
    else next = (at + step + items.length) % items.length;
    items[next]?.focus();
  };

  return (
    <ul
      ref={ref}
      role="menu"
      aria-label={label}
      tabIndex={-1}
      className={['pc-menu', className].filter(Boolean).join(' ')}
      style={{
        width,
        left: pos?.left ?? -9999,
        top: pos?.top ?? -9999,
        // A window too short for the menu on either side of its button.
        maxHeight: pos?.maxHeight,
        overflowY: pos?.maxHeight === undefined ? undefined : 'auto',
        visibility: pos ? 'visible' : 'hidden',
      }}
      onMouseUp={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          move(1);
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          move(-1);
        } else if (e.key === 'Home') {
          e.preventDefault();
          move('first');
        } else if (e.key === 'End') {
          e.preventDefault();
          move('last');
        } else if (e.key === 'Tab') {
          e.preventDefault();
          onClose();
        }
        // A menu inside the card keeps its keys, so Delete, Return, the
        // arrows and typing never reach the part picked behind it. Esc
        // closes it from the window, and Cmd and Ctrl shortcuts pass.
        if (e.key !== 'Escape' && !e.metaKey && !e.ctrlKey) e.stopPropagation();
      }}
    >
      {children}
    </ul>
  );
}

/** A separator between groups of a card menu. */
export function MenuSeparator() {
  return <li role="separator" className="pc-menu-sep" />;
}
