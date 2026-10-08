import { useCallback, useEffect, type ReactNode } from 'react';
import { MenuSurface, type MenuPlacer } from '../ui/MenuSurface';
import { menuPosition, type MenuPlace } from './cardRules';

// A menu the prompt card opens from one of its buttons: More, Presets,
// From another profile, More styles, or the name menu of P15. It is the
// one menu (MenuSurface), placed 4 px from its button and kept inside
// the window, on the other side of its button when its own side has no
// room (menuPosition). It takes focus itself with no row lit, keeps its
// keys from the part picked behind it, and hands focus back to its
// button as it closes.

interface CardMenuProps {
  /** The button the menu opens from. A press on it is left to its own
   *  handler, which closes the menu. */
  anchor: HTMLElement;
  place: MenuPlace;
  label: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
}

export function CardMenu({ anchor, place, label, onClose, children, className }: CardMenuProps) {
  // Made once per button and side, so the menu is placed as it opens and
  // not again while it scrolls in a short window.
  const placer = useCallback<MenuPlacer>(
    (size, viewport) => menuPosition(anchor.getBoundingClientRect(), size, place, viewport),
    [anchor, place],
  );

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

  return (
    <MenuSurface
      label={label}
      at={placer}
      anchor={anchor}
      focus="surface"
      keepKeys
      {...(className !== undefined && { className })}
      onClose={() => onClose()}
    >
      {children}
    </MenuSurface>
  );
}
