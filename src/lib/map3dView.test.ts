import { describe, expect, it } from 'vitest';
import {
  DEFAULT_MAP_3D_VIEW,
  DEFAULT_TILT,
  FLOOR_CHOICES,
  MAP_3D_VIEW_KEY,
  TILT_MAX,
  TILT_MIN,
  dragView,
  isNorthUp,
  isResetView,
  keyView,
  loadMap3dView,
  resetView,
  type Map3dView,
} from './map3dView';

function store(value: string | null) {
  return { getItem: (key: string) => (key === MAP_3D_VIEW_KEY ? value : null) };
}

const view = (patch: Partial<Map3dView> = {}): Map3dView => ({ ...DEFAULT_MAP_3D_VIEW, ...patch });

describe('the default view', () => {
  it('holds north at the top, tilted low, with one floor up and down and no sprites', () => {
    expect(DEFAULT_MAP_3D_VIEW).toEqual({
      turn: 0,
      tilt: DEFAULT_TILT,
      floors: 'adjacent',
      sprites: false,
    });
    expect(DEFAULT_TILT).toBeGreaterThan(TILT_MIN);
    expect(DEFAULT_TILT).toBeLessThan(45);
    expect(FLOOR_CHOICES).toEqual(['yours', 'adjacent', 'all']);
  });
});

describe('loadMap3dView', () => {
  it('gives the default view when nothing is stored', () => {
    expect(loadMap3dView(store(null))).toEqual(DEFAULT_MAP_3D_VIEW);
  });

  it('keeps a view you left', () => {
    const left = { turn: 135, tilt: 60, floors: 'all', sprites: true };
    expect(loadMap3dView(store(JSON.stringify(left)))).toEqual(left);
  });

  it('wraps the turn, holds the tilt in range, and defaults what it cannot read', () => {
    const odd = { turn: -90, tilt: 120, floors: 'some', sprites: 'yes' };
    expect(loadMap3dView(store(JSON.stringify(odd)))).toEqual({
      ...DEFAULT_MAP_3D_VIEW,
      turn: 270,
      tilt: TILT_MAX,
    });
    expect(loadMap3dView(store('{not json'))).toEqual(DEFAULT_MAP_3D_VIEW);
    expect(loadMap3dView(store('7'))).toEqual(DEFAULT_MAP_3D_VIEW);
    const broken = {
      getItem: () => {
        throw new Error('denied');
      },
    };
    expect(loadMap3dView(broken)).toEqual(DEFAULT_MAP_3D_VIEW);
  });
});

describe('dragView', () => {
  it('turns across and tilts down, keeping the floors and sprites', () => {
    const v = view({ floors: 'all', sprites: true });
    expect(dragView(v, 90, 0)).toEqual({ ...v, turn: 45 });
    expect(dragView(v, -90, 0)).toEqual({ ...v, turn: 315 });
    expect(dragView(v, 0, 30).tilt).toBeCloseTo(DEFAULT_TILT + 10);
  });

  it('tilts no lower than 20 degrees and no higher than 86', () => {
    expect(dragView(view(), 0, -1000).tilt).toBe(TILT_MIN);
    expect(dragView(view(), 0, 1000).tilt).toBe(TILT_MAX);
  });
});

describe('keyView', () => {
  it('moves the view as a short drag the way of the arrow', () => {
    expect(keyView(view(), 'ArrowRight')?.turn).toBe(15);
    expect(keyView(view(), 'ArrowLeft')?.turn).toBe(345);
    expect(keyView(view(), 'ArrowDown')?.tilt).toBeCloseTo(DEFAULT_TILT + 5);
    expect(keyView(view(), 'ArrowUp')?.tilt).toBeCloseTo(DEFAULT_TILT - 5);
    expect(keyView(view(), 'Enter')).toBeNull();
  });
});

describe('resetView', () => {
  it('puts north back at the top at the default tilt and keeps the rest', () => {
    const v = view({ turn: 200, tilt: 70, floors: 'yours', sprites: true });
    expect(resetView(v)).toEqual({ ...v, turn: 0, tilt: DEFAULT_TILT });
    expect(isResetView(v)).toBe(false);
    expect(isResetView(resetView(v))).toBe(true);
  });
});

describe('isNorthUp', () => {
  it('holds within half a degree either side of north', () => {
    expect(isNorthUp(view({ turn: 0 }))).toBe(true);
    expect(isNorthUp(view({ turn: 359.8 }))).toBe(true);
    expect(isNorthUp(view({ turn: 1 }))).toBe(false);
    expect(isNorthUp(view({ turn: 180 }))).toBe(false);
  });
});
