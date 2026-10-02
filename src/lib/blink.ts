import { useEffect, useState } from 'react';

// Blinking text (SGR 5), from the game or your prompt. The native
// grid, xterm and the pinned band all flip on one clock, counted from the
// Unix epoch, so whatever blinks on screen shows and hides together. The
// native grid keeps the same clock in src-tauri/src/cell_render.rs
// (BLINK_MS, blink_shown and until_blink_flip), so keep the two in step.

/** How long blinking text shows and how long it hides: the blink of
 *  xterm's cursor in its WebGL renderer. */
export const BLINK_MS = 600;

/** Blinking text shows at `now`, in milliseconds since the epoch. */
export function blinkShown(now: number): boolean {
  return Math.floor(now / BLINK_MS) % 2 === 0;
}

/** Milliseconds from `now` until blinking text flips. */
export function untilBlinkFlip(now: number): number {
  return BLINK_MS - (now % BLINK_MS);
}

/** Milliseconds from `now` until the next shown half starts. */
export function untilBlinkShows(now: number): number {
  return 2 * BLINK_MS - (now % (2 * BLINK_MS));
}

/** Blinking text is on: the choice you made in Settings, or with none
 *  on unless your system asks to reduce motion. */
export function resolveBlinkText(setting: boolean | null, reduceMotion: boolean): boolean {
  return setting ?? !reduceMotion;
}

export const REDUCE_MOTION_QUERY = '(prefers-reduced-motion: reduce)';

interface MediaList {
  matches: boolean;
  addEventListener?: (type: 'change', cb: (e: { matches: boolean }) => void) => void;
  removeEventListener?: (type: 'change', cb: (e: { matches: boolean }) => void) => void;
}

interface MediaWindow {
  matchMedia?: (query: string) => MediaList;
}

function media(win: MediaWindow | undefined): MediaList | null {
  return typeof win?.matchMedia === 'function' ? win.matchMedia(REDUCE_MOTION_QUERY) : null;
}

function currentWindow(): MediaWindow | undefined {
  return typeof window === 'undefined' ? undefined : window;
}

/** Your system asks apps to reduce motion. */
export function systemReducesMotion(win: MediaWindow | undefined = currentWindow()): boolean {
  return media(win)?.matches === true;
}

/** Hear each change of the system's reduce motion setting. Returns the
 *  unsubscribe. */
export function subscribeReduceMotion(
  cb: (reduce: boolean) => void,
  win: MediaWindow | undefined = currentWindow(),
): () => void {
  const list = media(win);
  if (!list?.addEventListener) return () => {};
  const onChange = (e: { matches: boolean }) => cb(e.matches);
  list.addEventListener('change', onChange);
  return () => list.removeEventListener?.('change', onChange);
}

/** Your system's reduce motion setting, kept current. */
export function useReduceMotion(): boolean {
  const [reduce, setReduce] = useState(() => systemReducesMotion());
  useEffect(() => subscribeReduceMotion(setReduce), []);
  return reduce;
}
