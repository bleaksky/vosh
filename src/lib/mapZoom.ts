// The map's zoom: its steps, and the scroll and pinch every style shares.
// Pure, so the gesture math is unit tested.

/** The zoom multiplies each style's own scale, and 1 is actual size.
 *  Steps of 0.25 keep a Squares cell on whole pixels. */
export const ZOOM_MIN = 0.5;
export const ZOOM_MAX = 3.0;
export const ZOOM_STEP = 0.25;

/** The nearest step inside the range, so arithmetic on many small
 *  gesture steps never drifts off a step. */
export function clampZoom(z: number): number {
  const snapped = Math.round(z / ZOOM_STEP) * ZOOM_STEP;
  return Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, snapped));
}

// Wheel travel in CSS pixels for one step. A trackpad pinch reaches the
// page as a wheel event with ctrlKey set and far smaller deltas than a
// scroll, so it steps on less travel.
const SCROLL_PER_STEP = 60;
const PINCH_PER_STEP = 12;
// deltaMode 1 counts lines and 2 counts pages.
const LINE_PX = 16;
const PAGE_PX = 400;

/** The parts of a WheelEvent the zoom reads. */
export interface WheelInput {
  deltaY: number;
  deltaMode: number;
  ctrlKey: boolean;
}

/** Fold one wheel event into the travel so far. Scrolling up or
 *  pinching out zooms in. Returns the steps it makes, never more than
 *  one either way so one notch of a mouse wheel is one step, and the
 *  travel it leaves for the next event. Turning back starts afresh. */
export function wheelZoomSteps(travel: number, e: WheelInput): { travel: number; steps: number } {
  const px =
    e.deltaMode === 1 ? e.deltaY * LINE_PX : e.deltaMode === 2 ? e.deltaY * PAGE_PX : e.deltaY;
  if (px === 0) return { travel, steps: 0 };
  const gain = -px / (e.ctrlKey ? PINCH_PER_STEP : SCROLL_PER_STEP);
  const next = (Math.sign(travel) === Math.sign(gain) ? travel : 0) + gain;
  if (Math.abs(next) < 1) return { travel: next, steps: 0 };
  return { travel: 0, steps: Math.sign(next) };
}

/** The zoom a pinch reaches, from the zoom it started at and the scale
 *  WebKit reports for it so far. */
export function pinchZoom(start: number, scale: number): number {
  return clampZoom(start * scale);
}
