import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from 'react';
import { useEscape } from './escapeStack';

// Dragging a row of a list to another place, as the sessions sidebar
// moves a session and Customize vitals moves a vital. A press on a row
// that moves 4 px lifts it. The row follows the pointer, the rows
// between its place and where it would land part to make room, and an
// accent line marks that place. Letting go moves the row there. A press
// that never moves stays a click. Near the top or the bottom of the
// list the list scrolls, so a row can travel the whole of it.
//
// From the keyboard, Space on a row's grip lifts it, the arrow keys move
// it a place at a time, Space drops it and Escape puts it back.
//
// The drag hears the pointer on the window instead of capturing it on
// the row, so every row lets the pointer through while one is lifted
// and none lights under it. Each list passes its rows' pitch, the
// height of a row and the gap after it.

/** How far a press moves before its row lifts. */
const LIFT_AFTER = 4;
/** The band at the list's top and bottom edges that scrolls it, and how
 *  far it scrolls each frame. */
const EDGE = 19;
const EDGE_STEP = 6;

/** A row in the air. */
export interface RowDrag<K> {
  id: K;
  /** Its place when it lifted, counting from 0. */
  from: number;
  /** The place it lands on among the other rows. */
  to: number;
  /** How far it sits from its own place. */
  dy: number;
  /** Lifted from the keyboard, so it moves a place at a time. */
  keyed: boolean;
}

/** How far a row lifted from `from` may sit from its place among `count`
 *  rows `pitch` apart, `dy` held inside the list. */
export function liftTravel(from: number, dy: number, count: number, pitch: number): number {
  return Math.max(-from * pitch, Math.min((count - 1 - from) * pitch, dy));
}

/** The place a row lifted from `from` and sitting `dy` from it lands on
 *  among `count` rows `pitch` apart, the slot its middle is over. */
export function dropPlace(from: number, dy: number, count: number, pitch: number): number {
  return Math.max(0, Math.min(count - 1, Math.round(from + dy / pitch)));
}

/** How far the row at `at` parts while the row from `from` would land at
 *  `to`: a row it passed steps one slot of `pitch` toward its place. */
export function partShift(at: number, from: number, to: number, pitch: number): number {
  if (from < to && at > from && at <= to) return -pitch;
  if (to < from && at >= to && at < from) return pitch;
  return 0;
}

/** Where a row lifted from the keyboard lands after `key`, among `count`
 *  rows, or null for a key that does not move it. */
export function keyedPlace(key: string, to: number, count: number): number | null {
  if (key === 'ArrowUp') return Math.max(0, to - 1);
  if (key === 'ArrowDown') return Math.min(count - 1, to + 1);
  return null;
}

/**
 * The drag of a row in `list`, whose rows hold the ids of `order`, each
 * `pitch` px below the one before. `press` starts one from a row's
 * pointerdown, `keyDown` from its grip's keys, `drag` is the row in the
 * air, and `dropped` says the click that follows a drop selects nothing.
 * `onMove` hears where a row landed, only when it moved.
 */
export function useRowDrag<K>(
  list: RefObject<HTMLElement | null>,
  order: readonly K[],
  onMove: (id: K, to: number) => void,
  pitch: number,
) {
  const [drag, setDrag] = useState<RowDrag<K> | null>(null);
  // The window listeners read the rows as they are now, since a row can
  // come or go while one is in the air.
  const latest = useRef({ order, onMove, drag });
  useEffect(() => {
    latest.current = { order, onMove, drag };
  });
  const dropped = useRef(false);
  const cancel = useRef<(() => void) | null>(null);
  useEffect(() => () => cancel.current?.(), []);
  // Escape puts a row lifted from the keyboard back, before anything
  // under the list hears it.
  useEscape(drag?.keyed === true, () => setDrag(null));

  const press = (e: ReactPointerEvent, id: K) => {
    if (e.button !== 0 || cancel.current || drag || !list.current) return;
    const scroller: HTMLElement = list.current;
    const startY = e.clientY;
    const startScroll = scroller.scrollTop;
    let pointerY = startY;
    let lifted = false;
    let frame = 0;
    let now: RowDrag<K> | null = null;

    function end(drop: boolean) {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerCancel);
      scroller.removeEventListener('scroll', onScroll);
      if (frame) cancelAnimationFrame(frame);
      cancel.current = null;
      if (!lifted) return;
      setDrag(null);
      // The click a drop ends in comes in this same task, so the flag
      // goes once it has passed.
      dropped.current = true;
      setTimeout(() => (dropped.current = false), 0);
      if (drop && now && now.to !== now.from) latest.current.onMove(id, now.to);
    }
    // Where the row sits for the pointer and the list's scroll.
    function follow() {
      const { order: rows } = latest.current;
      const from = rows.indexOf(id);
      if (from < 0) return end(false);
      const moved = pointerY - startY + scroller.scrollTop - startScroll;
      const dy = liftTravel(from, moved, rows.length, pitch);
      now = { id, from, to: dropPlace(from, dy, rows.length, pitch), dy, keyed: false };
      setDrag(now);
    }
    // While the pointer rests near an edge the list keeps scrolling, and
    // each scroll carries the row along.
    function edgeScroll() {
      frame = 0;
      const box = scroller.getBoundingClientRect();
      const step =
        pointerY < box.top + EDGE ? -EDGE_STEP : pointerY > box.bottom - EDGE ? EDGE_STEP : 0;
      const was = scroller.scrollTop;
      if (step !== 0) scroller.scrollTop = was + step;
      if (scroller.scrollTop !== was) frame = requestAnimationFrame(edgeScroll);
    }
    function onPointerMove(ev: PointerEvent) {
      pointerY = ev.clientY;
      if (!lifted && Math.abs(pointerY - startY) < LIFT_AFTER) return;
      lifted = true;
      follow();
      if (!frame) frame = requestAnimationFrame(edgeScroll);
    }
    function onPointerUp() {
      end(true);
    }
    function onPointerCancel() {
      end(false);
    }
    function onScroll() {
      if (lifted) follow();
    }

    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('pointercancel', onPointerCancel);
    scroller.addEventListener('scroll', onScroll);
    cancel.current = () => end(false);
  };

  /** Space lifts the row `id` and drops it again, and the arrow keys
   *  move it while it is lifted. Escape goes through the escape stack. */
  const keyDown = (e: ReactKeyboardEvent, id: K) => {
    if (cancel.current || e.altKey || e.ctrlKey || e.metaKey) return;
    const { order: rows, drag: now } = latest.current;
    const from = rows.indexOf(id);
    if (from < 0) return;
    if (e.key === ' ') {
      e.preventDefault();
      if (!now) {
        setDrag({ id, from, to: from, dy: 0, keyed: true });
        return;
      }
      if (now.id !== id) return;
      setDrag(null);
      if (now.to !== now.from) latest.current.onMove(id, now.to);
      return;
    }
    if (now?.id !== id || !now.keyed) return;
    const to = keyedPlace(e.key, now.to, rows.length);
    if (to === null) return;
    e.preventDefault();
    setDrag({ ...now, to, dy: (to - now.from) * pitch });
  };

  /** Put a row lifted from the keyboard back, as its grip loses focus. */
  const putBack = () => {
    if (latest.current.drag?.keyed) setDrag(null);
  };

  return { drag, press, keyDown, putBack, dropped: () => dropped.current };
}
