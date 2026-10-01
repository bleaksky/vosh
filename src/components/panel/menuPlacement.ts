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

// Space kept between a menu and the window edge.
const EDGE = 8;

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
