import { describe, expect, it } from 'vitest';
import {
  ZOOM_MAX,
  ZOOM_MIN,
  clampZoom,
  pinchZoom,
  wheelZoomSteps,
  type WheelInput,
} from './mapZoom';

const scroll = (deltaY: number): WheelInput => ({ deltaY, deltaMode: 0, ctrlKey: false });
const pinch = (deltaY: number): WheelInput => ({ deltaY, deltaMode: 0, ctrlKey: true });

/** The steps a run of wheel events makes, in order. */
function run(events: WheelInput[]): number[] {
  let travel = 0;
  return events.map((e) => {
    const out = wheelZoomSteps(travel, e);
    travel = out.travel;
    return out.steps;
  });
}

describe('clampZoom', () => {
  it('snaps to a quarter step inside the range', () => {
    expect(clampZoom(1.1)).toBe(1);
    expect(clampZoom(1.2)).toBe(1.25);
    expect(clampZoom(0.1)).toBe(ZOOM_MIN);
    expect(clampZoom(9)).toBe(ZOOM_MAX);
  });
});

describe('wheelZoomSteps', () => {
  it('steps once for each notch of a mouse wheel, in, then out', () => {
    expect(run([scroll(-100), scroll(-100), scroll(100)])).toEqual([1, 1, -1]);
  });

  it('gathers the small deltas of a trackpad scroll into steps', () => {
    expect(run(Array.from({ length: 6 }, () => scroll(-25)))).toEqual([0, 0, 1, 0, 0, 1]);
  });

  it('steps on less travel for a pinch, which arrives with ctrlKey', () => {
    expect(run([pinch(-5), pinch(-5), pinch(-5)])).toEqual([0, 0, 1]);
    expect(run([pinch(5), pinch(5), pinch(5)])).toEqual([0, 0, -1]);
  });

  it('starts afresh when you turn back', () => {
    expect(run([scroll(-50), scroll(25), scroll(25), scroll(25)])).toEqual([0, 0, 0, -1]);
  });

  it('counts lines and pages as pixels and leaves a sideways scroll alone', () => {
    expect(run([{ deltaY: -3, deltaMode: 1, ctrlKey: false }])).toEqual([0]);
    expect(run([{ deltaY: -4, deltaMode: 1, ctrlKey: false }])).toEqual([1]);
    expect(run([{ deltaY: 1, deltaMode: 2, ctrlKey: false }])).toEqual([-1]);
    expect(wheelZoomSteps(0.5, scroll(0))).toEqual({ travel: 0.5, steps: 0 });
  });
});

describe('pinchZoom', () => {
  it('scales the zoom the pinch started at, to a step in the range', () => {
    expect(pinchZoom(1, 1.6)).toBe(1.5);
    expect(pinchZoom(2, 0.5)).toBe(1);
    expect(pinchZoom(1, 0.1)).toBe(ZOOM_MIN);
  });
});
