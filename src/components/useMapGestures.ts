import { useEffect, useRef, type RefObject } from 'react';
import { dragView, keyView, resetView, type Map3dView } from '../lib/map3dView';
import { NO_WHEEL_RUN, ZOOM_STEP, clampZoom, pinchZoom, wheelZoomSteps } from '../lib/mapZoom';

// What your pointer, wheel and keys do over the map drawing. Plain
// scroll and a trackpad pinch zoom every style. Chromium sends a pinch
// as a wheel event with ctrlKey set, and WebKit sends gesture events
// with a scale, so both are read and the webview on each platform zooms
// the same. In 3D a drag turns and tilts the map, a double click puts
// north back at the top, and the arrow keys turn and tilt it while the
// drawing has focus.

/** WebKit's gesture event, which the DOM types leave out. */
interface GestureLike extends Event {
  scale: number;
}

interface Options {
  zoom: number;
  setZoom: (zoom: number) => void;
  /** The 3D view while the map draws in 3D, else null. */
  view: Map3dView | null;
  setView: (view: Map3dView) => void;
}

export function useMapGestures(ref: RefObject<HTMLElement | null>, options: Options): void {
  // The listeners stay bound for the life of the view and read the
  // latest values and setters through this ref. A setter also writes
  // its value here, so a second event before the next render builds on
  // the first.
  const latest = useRef(options);
  latest.current = options;

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let wheel = NO_WHEEL_RUN;
    let pinchFrom: number | null = null;
    let drag: { id: number; x: number; y: number } | null = null;

    const zoomTo = (z: number) => {
      const o = latest.current;
      if (z === o.zoom) return;
      o.setZoom(z);
      o.zoom = z;
    };
    const viewTo = (v: Map3dView) => {
      const o = latest.current;
      o.setView(v);
      o.view = v;
    };

    // Attached by hand, not passive, so the drawing can keep the wheel
    // from scrolling the panel and a pinch from zooming the page.
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      // WebKit may send a pinch as both, and the gesture wins.
      if (pinchFrom !== null && e.ctrlKey) return;
      const out = wheelZoomSteps(wheel, e);
      wheel = out.run;
      if (out.steps !== 0) zoomTo(clampZoom(latest.current.zoom + out.steps * ZOOM_STEP));
    };
    const onGestureStart = (e: Event) => {
      e.preventDefault();
      pinchFrom = latest.current.zoom;
    };
    const onGestureChange = (e: Event) => {
      e.preventDefault();
      if (pinchFrom !== null) zoomTo(pinchZoom(pinchFrom, (e as GestureLike).scale));
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
      if (!latest.current.view || e.button !== 0 || onButton(e)) return;
      drag = { id: e.pointerId, x: e.clientX, y: e.clientY };
      el.setPointerCapture?.(e.pointerId);
    };
    const onPointerMove = (e: PointerEvent) => {
      const v = latest.current.view;
      if (!drag || e.pointerId !== drag.id || !v) return;
      const dx = e.clientX - drag.x;
      const dy = e.clientY - drag.y;
      drag = { ...drag, x: e.clientX, y: e.clientY };
      if (dx !== 0 || dy !== 0) viewTo(dragView(v, dx, dy));
    };
    const onPointerUp = (e: PointerEvent) => {
      if (!drag || e.pointerId !== drag.id) return;
      drag = null;
      if (el.hasPointerCapture?.(e.pointerId)) el.releasePointerCapture(e.pointerId);
    };
    const onDoubleClick = (e: MouseEvent) => {
      const v = latest.current.view;
      if (v && !onButton(e)) viewTo(resetView(v));
    };
    const onKeyDown = (e: KeyboardEvent) => {
      const v = latest.current.view;
      if (!v || e.target !== el || e.altKey || e.ctrlKey || e.metaKey) return;
      const next = keyView(v, e.key);
      if (!next) return;
      e.preventDefault();
      viewTo(next);
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
      on('dblclick', onDoubleClick),
      on('keydown', onKeyDown),
    ];
    return () => offs.forEach((off) => off());
  }, [ref]);
}
