import { useEffect, useRef, type Dispatch, type RefObject, type SetStateAction } from 'react';
import { dragView, keyView, resetView, type Map3dView } from './map3dView';
import { NO_WHEEL_RUN, ZOOM_STEP, clampZoom, pinchZoom, wheelZoomSteps } from './mapZoom';

// What your pointer, wheel and keys do over the map drawing. Plain
// scroll and a trackpad pinch zoom every style. Chromium sends a pinch
// as a wheel event with ctrlKey set, and WebKit sends gesture events
// with a scale, so both are read and the webview on each platform zooms
// the same. In 3D a drag turns and tilts the map, a double click puts
// north back at the top, and the arrow keys turn and tilt it while the
// drawing has focus. In the flat styles the map hears where the pointer
// is, and a press and release is a click, which walks.

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
  /** Where the pointer is over a flat style, or null once it leaves the
   *  drawing or rests on the map's button. */
  onPoint: (at: MapPoint | null) => void;
  /** A click on a flat style, where it was. */
  onPick: (at: MapPoint) => void;
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
    let drag: { id: number; x: number; y: number } | null = null;
    /** The pointer pressed on a flat style, which a release makes a click. */
    let press: number | null = null;
    const pointAt = (e: PointerEvent): MapPoint => {
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

    // A press on the map's own button is the button's.
    const onButton = (e: Event) =>
      e.target instanceof Element && e.target.closest('button') !== null;
    // The command line keeps the caret while you drag the map.
    const onMouseDown = (e: MouseEvent) => {
      if (latest.current.view && !onButton(e)) e.preventDefault();
    };
    const onPointerDown = (e: PointerEvent) => {
      if (e.button !== 0 || onButton(e)) return;
      if (!latest.current.view) {
        press = e.pointerId;
        return;
      }
      drag = { id: e.pointerId, x: e.clientX, y: e.clientY };
      el.setPointerCapture?.(e.pointerId);
    };
    const onPointerMove = (e: PointerEvent) => {
      if (!latest.current.view) {
        latest.current.onPoint(onButton(e) ? null : pointAt(e));
        return;
      }
      if (!drag || e.pointerId !== drag.id || !latest.current.view) return;
      const dx = e.clientX - drag.x;
      const dy = e.clientY - drag.y;
      drag = { ...drag, x: e.clientX, y: e.clientY };
      if (dx !== 0 || dy !== 0) latest.current.setView((v) => dragView(v, dx, dy));
    };
    const onPointerUp = (e: PointerEvent) => {
      if (press !== null && e.pointerId === press && e.type === 'pointerup') {
        press = null;
        if (!latest.current.view && !onButton(e)) latest.current.onPick(pointAt(e));
        return;
      }
      press = null;
      if (!drag || e.pointerId !== drag.id) return;
      drag = null;
      if (el.hasPointerCapture?.(e.pointerId)) el.releasePointerCapture(e.pointerId);
    };
    const onPointerLeave = () => latest.current.onPoint(null);
    const onDoubleClick = (e: MouseEvent) => {
      if (latest.current.view && !onButton(e)) latest.current.setView(resetView);
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
