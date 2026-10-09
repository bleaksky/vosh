import type { MenuBox } from './menuPlacement';

// The pointer on its way to an open submenu, the way a macOS menu reads
// it. Going from a row to its submenu on a slant crosses the rows next
// to it, and each of those would take the highlight and close the
// submenu before the pointer got there. While the pointer heads into
// the submenu, a row it crosses waits instead. The row takes over once
// the pointer turns away or rests on it for HOLD_MS.

export interface Point {
  x: number;
  y: number;
}

/** How long a crossed row waits for the pointer to move on. */
export const HOLD_MS = 300;
// Pointer positions kept. The slant counts from any of them, so a
// jitter of a pixel or two, or a turn just made, does not read as
// turning away.
const TRAIL = 4;
// A submenu's edge reaches this far past its corners, for a pointer
// aimed at its first or last row.
const SLACK = 8;

/** Whether `to` lies in the triangle `a`, `b`, `c`, edges included. */
function inTriangle(to: Point, a: Point, b: Point, c: Point): boolean {
  const side = (p: Point, q: Point) => (q.x - p.x) * (to.y - p.y) - (q.y - p.y) * (to.x - p.x);
  const d1 = side(a, b);
  const d2 = side(b, c);
  const d3 = side(c, a);
  const neg = d1 < 0 || d2 < 0 || d3 < 0;
  const pos = d1 > 0 || d2 > 0 || d3 > 0;
  return !(neg && pos);
}

/** Whether a pointer that moved from `from` to `to` is heading into
 *  `box`, a submenu beside the menu: it got closer to the submenu's
 *  near edge and stays inside the triangle from `from` to that edge. */
export function headsInto(from: Point, to: Point, box: MenuBox): boolean {
  let edge: number;
  if (to.x <= box.left) edge = box.left;
  else if (to.x >= box.right) edge = box.right;
  else return false;
  if (Math.abs(edge - to.x) >= Math.abs(edge - from.x)) return false;
  return inTriangle(to, from, { x: edge, y: box.top - SLACK }, { x: edge, y: box.bottom + SLACK });
}

const trail: Point[] = [];
let tracking = 0;

/** Note where the pointer is. The listener below calls it. */
export function notePointer(x: number, y: number): void {
  trail.push({ x, y });
  if (trail.length > TRAIL) trail.shift();
}

const onMove = (e: PointerEvent) => notePointer(e.clientX, e.clientY);

/** Follow the pointer while a menu is open. Returns the stop. Menus
 *  share one listener. It runs in the capture phase, so a row hears the
 *  pointer after the trail does. */
export function trackMenuPointer(): () => void {
  if (tracking++ === 0) document.addEventListener('pointermove', onMove, true);
  return () => {
    if (--tracking === 0) {
      document.removeEventListener('pointermove', onMove, true);
      trail.length = 0;
    }
  };
}

/** Whether the pointer heads into one of `boxes`. */
export function aiming(boxes: MenuBox[]): boolean {
  const to = trail[trail.length - 1];
  if (!to) return false;
  return trail.slice(0, -1).some((from) => boxes.some((box) => headsInto(from, to, box)));
}

/** The open submenus `row` could be in the way of: each one not holding
 *  `row` and not the one `row` opens. */
function submenusPast(row: Element): MenuBox[] {
  const own = row.getAttribute('aria-controls');
  return Array.from(document.querySelectorAll('[data-menu-surface][data-menu-nested]'))
    .filter((el) => !el.contains(row) && el.id !== own)
    .map((el) => el.getBoundingClientRect());
}

let held: { row: Element; timer: ReturnType<typeof setTimeout> } | null = null;

function drop(): void {
  if (!held) return;
  clearTimeout(held.timer);
  held = null;
}

/** The pointer reached `row`. Run `take`, which gives the row the
 *  highlight and opens or closes submenus, now, or once the pointer
 *  stops heading into an open submenu past `row`. */
export function pointAt(row: Element, take: () => void, boxes = submenusPast(row)): void {
  drop();
  if (!aiming(boxes)) {
    take();
    return;
  }
  const timer = setTimeout(() => {
    held = null;
    take();
  }, HOLD_MS);
  held = { row, timer };
}

/** The pointer left `row`, so whatever it held waits no more. */
export function pointerLeft(row: Element): void {
  if (held?.row === row) drop();
}

/** The pointer is on a row drawn without MenuItem, in a menu with no
 *  submenu, as the prompt card's More styles and name menus draw theirs.
 *  The row takes the highlight, so the pointer and the keys share one. */
export function focusUnderPointer(e: { currentTarget: HTMLElement }): void {
  if (document.activeElement !== e.currentTarget) e.currentTarget.focus();
}

/** Forget the trail and anything held. For tests. */
export function resetMenuAim(): void {
  drop();
  trail.length = 0;
}
