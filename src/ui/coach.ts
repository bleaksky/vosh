import { useSyncExternalStore } from 'react';
import { createStore } from '../stores/store';

// What Show me rings, which CoachRing draws (First Run Q3). Each window
// keeps its own, so showCoach rings in the window that calls it.

export interface Coach {
  /** What to pick. The ring goes around all of them. An empty list
   *  means it has not drawn yet, and the ring keeps looking for a
   *  second. */
  find: () => readonly HTMLElement[];
  /** The line beside the ring. */
  line: string;
}

const coach = createStore<Coach | null>(null);

/** Ring what `next` finds, in place of any ring showing. */
export function showCoach(next: Coach): void {
  coach.set({ ...next });
}

export function clearCoach(): void {
  coach.set(null);
}

/** The rows of the open menu labeled `menu` whose label is one of
 *  `labels`, in the menu's order. */
export function menuRows(menu: string, labels: readonly string[]): HTMLElement[] {
  const surface = document.querySelector(`[role="menu"][aria-label="${CSS.escape(menu)}"]`);
  if (!surface) return [];
  return Array.from(surface.querySelectorAll<HTMLElement>('[role="menuitem"]')).filter((row) =>
    labels.includes(row.firstElementChild?.textContent ?? ''),
  );
}

/** How far out the ring's box sits from its targets. Its 2 px outline
 *  draws outside the box, so the accent runs 2 to 4 px out. */
const OUT = 2;
/** The line's gap from the ring. */
const TIP_GAP = 16;
/** How close the line comes to the window's edge. */
const INSET = 8;
/** How many frames to look for targets that have not drawn yet. */
export const FIND_FRAMES = 60;

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function ringBox(targets: readonly HTMLElement[]): Box {
  const rects = targets.map((t) => t.getBoundingClientRect());
  const left = Math.min(...rects.map((r) => r.left)) - OUT;
  const top = Math.min(...rects.map((r) => r.top)) - OUT;
  const right = Math.max(...rects.map((r) => r.right)) + OUT;
  const bottom = Math.max(...rects.map((r) => r.bottom)) + OUT;
  return { left, top, width: right - left, height: bottom - top };
}

export const sameBox = (a: Box | null, b: Box) =>
  a !== null &&
  a.left === b.left &&
  a.top === b.top &&
  a.width === b.width &&
  a.height === b.height;

/** Where the line sits: right of the ring when it fits, else left,
 *  centered on it, and inside the window either way. */
export function tipPlace(
  ring: Box,
  tip: { width: number; height: number },
  view: { width: number; height: number },
): { left: number; top: number } {
  const right = ring.left + ring.width + TIP_GAP;
  const left = right + tip.width <= view.width - INSET ? right : ring.left - TIP_GAP - tip.width;
  const top = ring.top + ring.height / 2 - tip.height / 2;
  const clamp = (v: number, max: number) => Math.round(Math.max(INSET, Math.min(v, max)));
  return {
    left: clamp(left, view.width - tip.width - INSET),
    top: clamp(top, view.height - tip.height - INSET),
  };
}

/** The coach showing, or null. */
export function useCoach(): Coach | null {
  return useSyncExternalStore(coach.subscribe, coach.get, coach.get);
}
