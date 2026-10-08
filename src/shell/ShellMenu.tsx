import type { ReactNode } from 'react';
import { MenuSurface } from '../ui/MenuSurface';
import { menuUnder } from '../ui/menuPlacement';

// The title band's menus, the session menu and Add a pane, and a
// session row's menu, on the one menu surface. A title band menu hangs
// 12 below its button (the session popover's 28 to 40), or 6 while it
// lists the sessions (34), so five rows fit whole at 720 by 450. It
// follows its button, stops 8 above the window's foot, and a list that
// scrolls, as the session popover's list of sessions does, gives way
// there. A session row's
// menu opens at the pointer instead, as a right click menu does, and
// rises from the pointer near the bottom of the window.

const GAP_BELOW_ANCHOR = 12;
const GAP_BELOW_ANCHOR_LISTED = 6;

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
  label: string;
  /** A fixed width. The session menu alone sets one. */
  width?: number;
  /** `dialog` while the surface holds a form instead of commands. */
  kind?: 'menu' | 'dialog';
  /** It holds a list that scrolls, which takes what height is left. */
  listed?: boolean;
  onClose: () => void;
  children: ReactNode;
};

export function ShellMenu({
  anchor = null,
  align = 'center',
  at,
  label,
  width,
  kind = 'menu',
  listed = false,
  onClose,
  children,
}: Props) {
  const placed = at
    ? { at: { x: at.x, y: at.y, flipY: at.y } }
    : {
        at: menuUnder(anchor, align, listed ? GAP_BELOW_ANCHOR_LISTED : GAP_BELOW_ANCHOR),
        anchor,
        anchored: true,
      };
  return (
    <MenuSurface
      {...placed}
      label={label}
      kind={kind}
      {...(width !== undefined && { width })}
      {...(listed && { className: 'is-listed' })}
      onClose={() => onClose()}
    >
      {children}
    </MenuSurface>
  );
}
