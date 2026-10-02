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

// Wheel travel in CSS pixels for each step after the first of a run. A
// trackpad pinch reaches the page as a wheel event with ctrlKey set and
// far smaller deltas than a scroll, so it steps on less travel.
const SCROLL_PER_STEP = 60;
const PINCH_PER_STEP = 12;
// A pause this many milliseconds long ends a run of wheel events.
const RUN_GAP_MS = 150;
// deltaMode 1 counts lines and 2 counts pages.
const LINE_PX = 16;
const PAGE_PX = 400;

/** The parts of a WheelEvent the zoom reads. */
export interface WheelInput {
  deltaY: number;
  deltaMode: number;
  ctrlKey: boolean;
  timeStamp: number;
}

/** A run of wheel events so far: the way it zooms, 1 in and -1 out,
 *  its travel toward the next step, and when its last event came. */
export interface WheelRun {
  dir: number;
  travel: number;
  at: number;
}

/** No run yet. */
export const NO_WHEEL_RUN: WheelRun = { dir: 0, travel: 0, at: 0 };

/** Fold one wheel event into the run so far. Scrolling up or pinching
 *  out zooms in. The first event of a run steps at once, so one notch of
 *  a mouse wheel is one step however small a delta its platform reports.
 *  Later events in the run gather travel and step once it reaches a
 *  step, never more than one step for one event. A pause or a turn back
 *  starts a new run, and the travel left over goes with the old one.
 *  Returns the steps the event makes and the run it leaves. */
export function wheelZoomSteps(run: WheelRun, e: WheelInput): { run: WheelRun; steps: number } {
  const px =
    e.deltaMode === 1 ? e.deltaY * LINE_PX : e.deltaMode === 2 ? e.deltaY * PAGE_PX : e.deltaY;
  if (px === 0) return { run, steps: 0 };
  const dir = px < 0 ? 1 : -1;
  const at = e.timeStamp;
  if (dir !== run.dir || at - run.at > RUN_GAP_MS) {
    return { run: { dir, travel: 0, at }, steps: dir };
  }
  const travel = run.travel + Math.abs(px) / (e.ctrlKey ? PINCH_PER_STEP : SCROLL_PER_STEP);
  if (travel < 1) return { run: { dir, travel, at }, steps: 0 };
  return { run: { dir, travel: 0, at }, steps: dir };
}

/** The zoom a pinch reaches, from the zoom it started at and the scale
 *  WebKit reports for it so far. */
export function pinchZoom(start: number, scale: number): number {
  return clampZoom(start * scale);
}
