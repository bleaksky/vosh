import {
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from 'react';

// Dragging a row of the sessions sidebar to another place, board 8 of
// the Sessions review (Q18). A press on a row that moves 4 px lifts it.
// The row follows the pointer, the rows between its place and where it
// would land part to make room, and an accent line marks that place.
// Letting go moves the session there, so ⌘1 to ⌘9 and the next launch
// follow the new order. A press that never moves stays a click. Near
// the top or the bottom of the list the list scrolls, so a row can
// travel the whole of it.
//
// The drag hears the pointer on the window instead of capturing it on
// the row, so every row lets the pointer through while one is lifted
// and none lights under it.

/** The rows' pitch, a 44 pill in a 46 slot (S1 of the Sessions Sidebar
 *  review). */
export const ROW_PITCH = 46;
/** How far a press moves before its row lifts. */
const LIFT_AFTER = 4;
/** The band at the list's top and bottom edges that scrolls it, half a
 *  row, and how far it scrolls each frame. */
const EDGE = 19;
const EDGE_STEP = 6;

/** A row in the air. */
export interface RowDrag {
  session: number;
  /** Its place when it lifted, counting from 0. */
  from: number;
  /** The place it lands on among the other rows, which session_move
   *  takes. */
  to: number;
  /** How far it sits from its own place. */
  dy: number;
}

/** How far a row lifted from `from` may sit from its place among `count`
 *  rows, `dy` held inside the list. */
export function liftTravel(from: number, dy: number, count: number): number {
  return Math.max(-from * ROW_PITCH, Math.min((count - 1 - from) * ROW_PITCH, dy));
}

/** The place a row lifted from `from` and sitting `dy` from it lands on
 *  among `count` rows, the slot its middle is over. */
export function dropPlace(from: number, dy: number, count: number): number {
  return Math.max(0, Math.min(count - 1, Math.round(from + dy / ROW_PITCH)));
}

/** How far the row at `at` parts while the row from `from` would land at
 *  `to`: a row it passed steps one slot toward its place. */
export function partShift(at: number, from: number, to: number): number {
  if (from < to && at > from && at <= to) return -ROW_PITCH;
  if (to < from && at >= to && at < from) return ROW_PITCH;
  return 0;
}

/**
 * The drag of a row in `list`, whose rows hold the sessions of `order`.
 * `press` starts one from a row's pointerdown, `drag` is the row in the
 * air, and `dropped` says the click that follows a drop selects nothing.
 */
export function useRowDrag(
  list: RefObject<HTMLElement | null>,
  order: readonly number[],
  onMove: (session: number, to: number) => void,
) {
  const [drag, setDrag] = useState<RowDrag | null>(null);
  // The window listeners read the rows as they are now, since a session
  // can open or close while a row is in the air.
  const latest = useRef({ order, onMove });
  useEffect(() => {
    latest.current = { order, onMove };
  });
  const dropped = useRef(false);
  const cancel = useRef<(() => void) | null>(null);
  useEffect(() => () => cancel.current?.(), []);

  const press = (e: ReactPointerEvent, session: number) => {
    if (e.button !== 0 || cancel.current || !list.current) return;
    const scroller: HTMLElement = list.current;
    const startY = e.clientY;
    const startScroll = scroller.scrollTop;
    let pointerY = startY;
    let lifted = false;
    let frame = 0;
    let now: RowDrag | null = null;

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
      if (drop && now && now.to !== now.from) latest.current.onMove(session, now.to);
    }
    // Where the row sits for the pointer and the list's scroll.
    function follow() {
      const { order: rows } = latest.current;
      const from = rows.indexOf(session);
      if (from < 0) return end(false);
      const moved = pointerY - startY + scroller.scrollTop - startScroll;
      const dy = liftTravel(from, moved, rows.length);
      now = { session, from, to: dropPlace(from, dy, rows.length), dy };
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

  return { drag, press, dropped: () => dropped.current };
}
