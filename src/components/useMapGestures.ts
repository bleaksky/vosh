import { useEffect, useRef, type RefObject } from 'react';
import { ZOOM_STEP, clampZoom, pinchZoom, wheelZoomSteps } from '../lib/mapZoom';

// What the wheel and a pinch do over the map drawing. Plain scroll and a
// trackpad pinch zoom every style. Chromium sends a pinch as a wheel
// event with ctrlKey set, and WebKit sends gesture events with a scale,
// so both are read and the webview on each platform zooms the same.

/** WebKit's gesture event, which the DOM types leave out. */
interface GestureLike extends Event {
  scale: number;
}

interface Options {
  zoom: number;
  setZoom: (zoom: number) => void;
}

export function useMapGestures(ref: RefObject<HTMLElement | null>, options: Options): void {
  // The listeners stay bound for the life of the view and read the
  // latest zoom and setter through this ref.
  const latest = useRef(options);
  latest.current = options;

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let travel = 0;
    let pinchFrom: number | null = null;
    const set = (z: number) => {
      if (z === latest.current.zoom) return;
      latest.current.setZoom(z);
      // A second event before the next render steps from this zoom.
      latest.current.zoom = z;
    };
    // Attached by hand, not passive, so the drawing can keep the wheel
    // from scrolling the panel and a pinch from zooming the page.
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      // WebKit may send a pinch as both, and the gesture wins.
      if (pinchFrom !== null && e.ctrlKey) return;
      const out = wheelZoomSteps(travel, e);
      travel = out.travel;
      if (out.steps !== 0) set(clampZoom(latest.current.zoom + out.steps * ZOOM_STEP));
    };
    const onGestureStart = (e: Event) => {
      e.preventDefault();
      pinchFrom = latest.current.zoom;
    };
    const onGestureChange = (e: Event) => {
      e.preventDefault();
      if (pinchFrom !== null) set(pinchZoom(pinchFrom, (e as GestureLike).scale));
    };
    const onGestureEnd = (e: Event) => {
      e.preventDefault();
      pinchFrom = null;
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    el.addEventListener('gesturestart', onGestureStart);
    el.addEventListener('gesturechange', onGestureChange);
    el.addEventListener('gestureend', onGestureEnd);
    return () => {
      el.removeEventListener('wheel', onWheel);
      el.removeEventListener('gesturestart', onGestureStart);
      el.removeEventListener('gesturechange', onGestureChange);
      el.removeEventListener('gestureend', onGestureEnd);
    };
  }, [ref]);
}
