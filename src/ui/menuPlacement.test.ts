import { describe, expect, it } from 'vitest';
import { menuBelow, menuUnder, placeMenu, submenuAt } from './menuPlacement';

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

describe('submenuAt', () => {
  // A menu 232 wide whose row is 30 tall, 6 inside the menu's padding.
  const menuAt = (left: number, top: number) => ({
    menu: { left, right: left + 232, top, bottom: top + 200 },
    row: { left: left + 6, right: left + 226, top: top + 66, bottom: top + 96 },
  });

  it('opens right of the menu, its first row level with the row', () => {
    const { menu, row } = menuAt(100, 100);
    // 4 past the menu, and up by its own 6 of padding.
    expect(placeMenu(submenuAt(row, menu), W, H, VW, VH)).toEqual({ left: 336, top: 160 });
  });

  it('opens left of the menu at the right edge', () => {
    const { menu, row } = menuAt(1000, 100);
    expect(placeMenu(submenuAt(row, menu), W, H, VW, VH)).toEqual({ left: 796, top: 160 });
  });

  it('rises from the row at the bottom edge', () => {
    const { menu, row } = menuAt(100, 560);
    // Its bottom sits 6 below the row's, so its last row is level with it.
    expect(placeMenu(submenuAt(row, menu), W, H, VW, VH)).toEqual({ left: 336, top: 362 });
  });

  it('carries preferFlip only when asked', () => {
    const { menu, row } = menuAt(100, 100);
    expect(submenuAt(row, menu)).not.toHaveProperty('preferFlip');
    expect(submenuAt(row, menu, true).preferFlip).toBe(true);
  });
});

describe('menuBelow', () => {
  it('opens 4 px under the button, and over it or left of it at the edges', () => {
    // A row's more button, 28 by 24.
    const button = { left: 812, right: 840, top: 92, bottom: 116 };
    expect(menuBelow(button)).toEqual({ x: 812, y: 120, flipX: 840, flipY: 88 });
    expect(placeMenu(menuBelow(button), W, H, VW, VH)).toEqual({ left: 812, top: 120 });
    const low = { left: 1200, right: 1228, top: 700, bottom: 724 };
    expect(placeMenu(menuBelow(low), W, H, VW, VH)).toEqual({ left: 1028, top: 396 });
  });
});

describe('menuUnder', () => {
  // A title band button, 28 by 28, and a menu 272 by 300.
  const size = { width: 272, height: H };
  const viewport = { width: VW, height: VH };
  const at = (left: number) => ({
    getBoundingClientRect: () => ({ left, right: left + 28, top: 6, bottom: 34 }),
  });

  it('hangs centered under its button, its height kept 8 above the foot', () => {
    expect(menuUnder(at(600), 'center', 12)(size, viewport)).toEqual({
      left: 478,
      top: 46,
      maxHeight: 746,
    });
  });

  it('lines up right edges, and stays 8 inside the window', () => {
    expect(menuUnder(at(1100), 'end', 6)(size, viewport)).toEqual({
      left: 856,
      top: 40,
      maxHeight: 752,
    });
    expect(menuUnder(at(1250), 'center', 12)(size, viewport).left).toBe(1000);
    expect(menuUnder(at(20), 'center', 12)(size, viewport).left).toBe(8);
  });

  it('sits at the top left with no button', () => {
    expect(menuUnder(null, 'end', 12)(size, viewport)).toEqual({
      left: 8,
      top: 8,
      maxHeight: 784,
    });
  });
});
