import { describe, expect, it } from 'vitest';
import {
  NO_WHEEL_RUN,
  ZOOM_MAX,
  ZOOM_MIN,
  clampZoom,
  pinchZoom,
  wheelZoomSteps,
  type WheelInput,
} from './mapZoom';

// Wheel events at a time in milliseconds.
const scroll = (deltaY: number, timeStamp: number): WheelInput => ({
  deltaY,
  deltaMode: 0,
  ctrlKey: false,
  timeStamp,
});
const pinch = (deltaY: number, timeStamp: number): WheelInput => ({
  ...scroll(deltaY, timeStamp),
  ctrlKey: true,
});

/** The steps a run of wheel events makes, in order. */
function run(events: WheelInput[]): number[] {
  let state = NO_WHEEL_RUN;
  return events.map((e) => {
    const out = wheelZoomSteps(state, e);
    state = out.run;
    return out.steps;
  });
}

/** Events 16 ms apart, as a trackpad sends them. */
const every16 = (make: (t: number) => WheelInput, n: number) =>
  Array.from({ length: n }, (_, i) => make(i * 16));

describe('clampZoom', () => {
  it('snaps to a quarter step inside the range', () => {
    expect(clampZoom(1.1)).toBe(1);
    expect(clampZoom(1.2)).toBe(1.25);
    expect(clampZoom(0.1)).toBe(ZOOM_MIN);
    expect(clampZoom(9)).toBe(ZOOM_MAX);
  });
});

describe('wheelZoomSteps', () => {
  it('steps once for each notch of a mouse wheel, however small its delta', () => {
    // A notch can come as 100 px or as a few, by platform.
    expect(run([scroll(-100, 0), scroll(-100, 300), scroll(100, 600)])).toEqual([1, 1, -1]);
    expect(run([scroll(-4, 0), scroll(-4, 300), scroll(4, 600)])).toEqual([1, 1, -1]);
  });

  it('steps at the start of a trackpad scroll, then for each 60 px of it', () => {
    expect(run(every16((t) => scroll(-25, t), 6))).toEqual([1, 0, 0, 1, 0, 0]);
  });

  it('steps on less travel for a pinch, which arrives with ctrlKey', () => {
    expect(run(every16((t) => pinch(-5, t), 4))).toEqual([1, 0, 0, 1]);
    expect(run(every16((t) => pinch(5, t), 4))).toEqual([-1, 0, 0, -1]);
  });

  it('starts a new run when you turn back or pause, and drops the travel left', () => {
    expect(run([scroll(-25, 0), scroll(-25, 16), scroll(25, 32), scroll(25, 48)])).toEqual([
      1, 0, -1, 0,
    ]);
    expect(run([scroll(-25, 0), scroll(-50, 16), scroll(-25, 300), scroll(-25, 316)])).toEqual([
      1, 0, 1, 0,
    ]);
  });

  it('counts lines and pages as pixels and leaves a sideways scroll alone', () => {
    const line = (deltaY: number, t: number) => ({ ...scroll(0, t), deltaY, deltaMode: 1 });
    expect(run([line(-1, 0), line(-3, 16), line(-1, 32)])).toEqual([1, 0, 1]);
    const page = (deltaY: number, t: number) => ({ ...scroll(0, t), deltaY, deltaMode: 2 });
    expect(run([page(1, 0), page(1, 16)])).toEqual([-1, -1]);
    const going = { dir: 1, travel: 0.5, at: 0 };
    expect(wheelZoomSteps(going, scroll(0, 16))).toEqual({ run: going, steps: 0 });
  });
});

describe('pinchZoom', () => {
  it('scales the zoom the pinch started at, to a step in the range', () => {
    expect(pinchZoom(1, 1.6)).toBe(1.5);
    expect(pinchZoom(2, 0.5)).toBe(1);
    expect(pinchZoom(1, 0.1)).toBe(ZOOM_MIN);
  });
});
