// Where a menu sits: at its spot when it fits, flipped at the window's
// edge, kept inside the window. MenuSurface places every menu with it.

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
  /** Try `flipX` first and fall back to `x`. A submenu passes it when
   *  its parent opened to the left, so the cascade keeps going left
   *  instead of opening back over the menu before it. */
  preferFlip?: boolean;
}

/** A width and a height, of a menu or of the window. */
export interface MenuSize {
  width: number;
  height: number;
}

/** Where a menu sits, and how tall it may grow before it scrolls. */
export interface MenuSpot {
  left: number;
  top: number;
  maxHeight?: number;
}

/** Places a menu of `size` in a window of `viewport`, for a menu whose
 *  spot a MenuPlacement cannot say, as one hung under a title band
 *  button. */
export type MenuPlacer = (size: MenuSize, viewport: MenuSize) => MenuSpot;

// Space kept between a menu and the window edge.
const EDGE = 8;
// Space between a menu and a submenu beside it.
const SUBMENU_GAP = 4;
// A menu's inner padding. A submenu rises by it, so its first row sits
// level with the row that opened it.
const MENU_PAD = 6;

/** The edges of a box on screen, as getBoundingClientRect gives them. */
export type MenuBox = Pick<DOMRect, 'left' | 'right' | 'top' | 'bottom'>;

// Space between a button and the menu it opens.
const BUTTON_GAP = 4;

/** Where the menu `button` opens wants to sit: under the button from
 *  its left edge, ending at its right edge at the window's right edge,
 *  and over it at the bottom edge. A row's more button opens its menu
 *  here. */
export function menuBelow(button: MenuBox): MenuPlacement {
  return {
    x: button.left,
    y: button.bottom + BUTTON_GAP,
    flipX: button.right,
    flipY: button.top - BUTTON_GAP,
  };
}

/** A menu hung `gap` under `anchor`, centered under it or with the
 *  right edges lined up, kept inside the window. It never runs past the
 *  window's foot, and stops there 8 above it. With no anchor it sits at
 *  the window's top left. The title band's menus hang here. */
export function menuUnder(
  anchor: { getBoundingClientRect: () => MenuBox } | null,
  align: 'center' | 'end',
  gap: number,
): MenuPlacer {
  return (size, viewport) => {
    if (!anchor) return { left: EDGE, top: EDGE, maxHeight: viewport.height - 2 * EDGE };
    const r = anchor.getBoundingClientRect();
    const ideal = align === 'center' ? (r.left + r.right - size.width) / 2 : r.right - size.width;
    const left = Math.round(Math.max(EDGE, Math.min(ideal, viewport.width - size.width - EDGE)));
    const top = Math.round(r.bottom + gap);
    return { left, top, maxHeight: viewport.height - top - EDGE };
  };
}

/** Where a submenu wants to sit beside `row`, a row of `menu`: to the
 *  right of the menu with its first row level with `row`, flipped to the
 *  left of the menu at the window's right edge, and up from the bottom
 *  of `row` at the bottom edge. Pass `preferFlip` when `menu` itself
 *  opened to the left of its parent. */
export function submenuAt(row: MenuBox, menu: MenuBox, preferFlip?: boolean): MenuPlacement {
  return {
    x: menu.right + SUBMENU_GAP,
    y: row.top - MENU_PAD,
    flipX: menu.left - SUBMENU_GAP,
    flipY: row.bottom + MENU_PAD,
    ...(preferFlip !== undefined && { preferFlip }),
  };
}

/** Where a menu `w` by `h` sits in a `vw` by `vh` window. */
export function placeMenu(
  at: MenuPlacement,
  w: number,
  h: number,
  vw: number,
  vh: number,
): { left: number; top: number } {
  let left = at.x;
  if (at.preferFlip && at.flipX !== undefined && at.flipX - w >= EDGE) left = at.flipX - w;
  else if (left + w > vw - EDGE && at.flipX !== undefined) left = at.flipX - w;
  left = Math.max(EDGE, Math.min(left, vw - EDGE - w));
  let top = at.y;
  if (top + h > vh - EDGE && at.flipY !== undefined) top = at.flipY - h;
  top = Math.max(EDGE, Math.min(top, vh - EDGE - h));
  return { left: Math.round(left), top: Math.round(top) };
}
