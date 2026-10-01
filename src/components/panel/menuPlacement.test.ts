import { describe, expect, it } from 'vitest';
import { placeMenu } from './menuPlacement';

// A 1280 by 800 window, a menu 200 by 300.
const W = 200;
const H = 300;
const VW = 1280;
const VH = 800;

describe('placeMenu', () => {
  it('opens at x when the menu fits, and flips at the window edge', () => {
    expect(placeMenu({ x: 400, y: 100, flipX: 380 }, W, H, VW, VH)).toEqual({
      left: 400,
      top: 100,
    });
    expect(placeMenu({ x: 1150, y: 100, flipX: 1000 }, W, H, VW, VH)).toEqual({
      left: 800,
      top: 100,
    });
  });

  it('keeps going left once its parent opened to the left', () => {
    // A channel's colors beside Channel colors, which opened to the left
    // of the pane menu at the window's right edge. Opening at x would put
    // it back over the pane menu even though it fits there.
    const at = { x: 900, y: 100, flipX: 690, preferFlip: true };
    expect(placeMenu(at, W, H, VW, VH)).toEqual({ left: 490, top: 100 });
  });

  it('falls back to x when there is no room on the left', () => {
    const at = { x: 260, y: 100, flipX: 150, preferFlip: true };
    expect(placeMenu(at, W, H, VW, VH)).toEqual({ left: 260, top: 100 });
  });

  it('flips up at the bottom edge', () => {
    expect(placeMenu({ x: 100, y: 700, flipY: 650 }, W, H, VW, VH)).toEqual({
      left: 100,
      top: 350,
    });
  });
});
