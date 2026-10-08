import { useEffect, useRef, type Dispatch, type RefObject, type SetStateAction } from 'react';
import { dragView, keyView, resetView, type Map3dView } from './map3dView';
import { NO_WHEEL_RUN, ZOOM_STEP, clampZoom, pinchZoom, wheelZoomSteps } from './mapZoom';

// What your pointer, wheel and keys do over the map drawing. Plain
// scroll and a trackpad pinch zoom every style. Chromium sends a pinch
// as a wheel event with ctrlKey set, and WebKit sends gesture events
// with a scale, so both are read and the webview on each platform zooms
// the same. The map hears where the pointer is, and a press and release
// is a click, which walks. The second press of a double click walks no
// further. In 3D a press that moves 4 px or more is a drag instead,
// which turns and tilts the map, a double click on bare ground puts
// north back at the top, and the arrow keys turn and tilt it while the
// drawing has focus.

/** How far a press in 3D moves before it turns the map instead of
 *  walking. */
const CLICK_SLOP_PX = 4;

/** WebKit's gesture event, which the DOM types leave out. */
interface GestureLike extends Event {
  scale: number;
}

/** A point in the drawing, from its top left, with the drawing's size. */
export interface MapPoint {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface Options {
  /** The zoom a pinch starts from. */
  zoom: number;
  setZoom: Dispatch<SetStateAction<number>>;
  /** The 3D view while the map draws in 3D, else null. */
  view: Map3dView | null;
  setView: Dispatch<SetStateAction<Map3dView>>;
  /** Where the pointer is, or null once it leaves the drawing, rests
   *  on the map's button or turns the 3D map. */
  onPoint: (at: MapPoint | null) => void;
  /** A click, where it was. */
  onPick: (at: MapPoint) => void;
  /** Whether a point lands on a room, so a double click there walks
   *  once and leaves the 3D view as it is. */
  onRoom: (at: MapPoint) => boolean;
}

export function useMapGestures(ref: RefObject<HTMLElement | null>, options: Options): void {
  // The listeners stay bound for the life of the view and read the
  // latest values and setters through this ref. Each change goes
  // through an updater, so a second event before the next render builds
  // on the first.
  const latest = useRef(options);
  latest.current = options;

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let wheel = NO_WHEEL_RUN;
    let pinchFrom: number | null = null;
    /** A press in 3D: where it went down, where it was last, and
     *  whether it has moved far enough to turn the map. */
    let drag: {
      id: number;
      x0: number;
      y0: number;
      x: number;
      y: number;
      turning: boolean;
    } | null = null;
    /** The pointer pressed on a flat style, which a release makes a click. */
    let press: number | null = null;
    /** The clicks the last press counts, which its mousedown says, so
     *  the second press of a double click does not walk again. */
    let clicks = 1;
    const pointAt = (e: MouseEvent): MapPoint => {
      const box = el.getBoundingClientRect();
      return {
        x: e.clientX - box.left,
        y: e.clientY - box.top,
        width: el.clientWidth,
        height: el.clientHeight,
      };
    };

    // Attached by hand, not passive, so the drawing can keep the wheel
    // from scrolling the panel and a pinch from zooming the page.
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      // WebKit may send a pinch as both, and the gesture wins.
      if (pinchFrom !== null && e.ctrlKey) return;
      const { run, steps } = wheelZoomSteps(wheel, e);
      wheel = run;
      if (steps !== 0) latest.current.setZoom((z) => clampZoom(z + steps * ZOOM_STEP));
    };
    const onGestureStart = (e: Event) => {
      e.preventDefault();
      pinchFrom = latest.current.zoom;
    };
    const onGestureChange = (e: Event) => {
      e.preventDefault();
      if (pinchFrom === null) return;
      latest.current.setZoom(pinchZoom(pinchFrom, (e as GestureLike).scale));
    };
    const onGestureEnd = (e: Event) => {
      e.preventDefault();
      pinchFrom = null;
    };

    // A press on the map's own button or on the walk chip is theirs.
    const onControl = (e: Event) =>
      e.target instanceof Element && e.target.closest('button, .walk-chip') !== null;
    // The command line keeps the caret while you drag the map.
    const onMouseDown = (e: MouseEvent) => {
      clicks = e.detail;
      if (latest.current.view && !onControl(e)) e.preventDefault();
    };
    const onPointerDown = (e: PointerEvent) => {
      clicks = 1;
      if (e.button !== 0 || onControl(e)) return;
      if (!latest.current.view) {
        press = e.pointerId;
        return;
      }
      const { clientX: x, clientY: y } = e;
      drag = { id: e.pointerId, x0: x, y0: y, x, y, turning: false };
      el.setPointerCapture?.(e.pointerId);
    };
    const onPointerMove = (e: PointerEvent) => {
      if (!drag || e.pointerId !== drag.id || !latest.current.view) {
        latest.current.onPoint(onControl(e) ? null : pointAt(e));
        return;
      }
      // The map holds still until the press moves 4 px, then follows
      // the pointer from where it went down.
      const { clientX: x, clientY: y } = e;
      if (!drag.turning && Math.hypot(x - drag.x0, y - drag.y0) < CLICK_SLOP_PX) return;
      if (!drag.turning) latest.current.onPoint(null);
      const dx = x - drag.x;
      const dy = y - drag.y;
      drag = { ...drag, x, y, turning: true };
      if (dx !== 0 || dy !== 0) latest.current.setView((v) => dragView(v, dx, dy));
    };
    const onPointerUp = (e: PointerEvent) => {
      const up = e.type === 'pointerup' && !onControl(e) && clicks < 2;
      if (press !== null && e.pointerId === press) {
        press = null;
        if (up && !latest.current.view) latest.current.onPick(pointAt(e));
        return;
      }
      press = null;
      if (!drag || e.pointerId !== drag.id) return;
      const click = !drag.turning;
      drag = null;
      if (el.hasPointerCapture?.(e.pointerId)) el.releasePointerCapture(e.pointerId);
      if (click && up && latest.current.view) latest.current.onPick(pointAt(e));
    };
    const onPointerLeave = () => latest.current.onPoint(null);
    const onDoubleClick = (e: MouseEvent) => {
      if (!latest.current.view || onControl(e)) return;
      if (!latest.current.onRoom(pointAt(e))) latest.current.setView(resetView);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      const v = latest.current.view;
      if (!v || e.target !== el || e.altKey || e.ctrlKey || e.metaKey) return;
      const { key } = e;
      if (!keyView(v, key)) return;
      e.preventDefault();
      latest.current.setView((cur) => keyView(cur, key) ?? cur);
    };

    const on = <E extends Event>(
      type: string,
      fn: (e: E) => void,
      opts?: AddEventListenerOptions,
    ) => {
      el.addEventListener(type, fn as EventListener, opts);
      return () => el.removeEventListener(type, fn as EventListener);
    };
    const offs = [
      on('wheel', onWheel, { passive: false }),
      on('gesturestart', onGestureStart),
      on('gesturechange', onGestureChange),
      on('gestureend', onGestureEnd),
      on('mousedown', onMouseDown),
      on('pointerdown', onPointerDown),
      on('pointermove', onPointerMove),
      on('pointerup', onPointerUp),
      on('pointercancel', onPointerUp),
      on('pointerleave', onPointerLeave),
      on('dblclick', onDoubleClick),
      on('keydown', onKeyDown),
    ];
    return () => offs.forEach((off) => off());
  }, [ref]);
}
