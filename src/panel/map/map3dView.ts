// How you look at the 3D map, and how Vosh keeps it across steps and
// restarts. Pure over a storage reader, so the fallbacks and the drag
// and key math are unit tested.

/** Which floors the 3D map draws besides yours. */
export type Floors = 'yours' | 'adjacent' | 'all';

/** The floor choices the map menu offers, in its order. */
export const FLOOR_CHOICES: readonly Floors[] = ['yours', 'adjacent', 'all'];

export interface Map3dView {
  /** How far the map turns from north at the top, in degrees from 0 up
   *  to 360. A positive turn swings north toward the left. */
  turn: number;
  /** How far above the horizon you look, in degrees. */
  tilt: number;
  floors: Floors;
  /** Paint the atlas sprites on the lit roofs instead of plain color. */
  sprites: boolean;
}

export const TILT_MIN = 20;
export const TILT_MAX = 86;
/** Low enough that each floor stands well apart from the next while
 *  north holds the top. */
export const DEFAULT_TILT = 35.5;

export const DEFAULT_MAP_3D_VIEW: Map3dView = {
  turn: 0,
  tilt: DEFAULT_TILT,
  floors: 'adjacent',
  sprites: false,
};

export const MAP_3D_VIEW_KEY = 'vosh.map.view3d';

// A drag turns half a degree for each pixel across and tilts a third
// of one for each pixel down. An arrow key moves as far as a short drag.
const TURN_PER_PX = 0.5;
const TILT_PER_PX = 1 / 3;
const TURN_PER_KEY = 15;
const TILT_PER_KEY = 5;

function wrap(turn: number): number {
  const t = turn % 360;
  return t < 0 ? t + 360 : t;
}

function clampTilt(tilt: number): number {
  return Math.max(TILT_MIN, Math.min(TILT_MAX, tilt));
}

/** The view a stored value holds. Each field it lacks or cannot read
 *  takes its default, and storage that throws gives the default view. */
export function loadMap3dView(storage: Pick<Storage, 'getItem'>): Map3dView {
  let raw: unknown = null;
  try {
    raw = JSON.parse(storage.getItem(MAP_3D_VIEW_KEY) ?? 'null');
  } catch {
    // Unreadable storage or text. The defaults stand.
  }
  const v = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  const num = (x: unknown) => typeof x === 'number' && Number.isFinite(x);
  const d = DEFAULT_MAP_3D_VIEW;
  return {
    turn: num(v.turn) ? wrap(v.turn as number) : d.turn,
    tilt: num(v.tilt) ? clampTilt(v.tilt as number) : d.tilt,
    floors: FLOOR_CHOICES.includes(v.floors as Floors) ? (v.floors as Floors) : d.floors,
    sprites: typeof v.sprites === 'boolean' ? v.sprites : d.sprites,
  };
}

/** The view after a drag of dx, dy pixels. The map follows the pointer
 *  as if you held its near edge. Across turns it, and down tilts it
 *  toward looking straight down, inside 20 to 86 degrees. */
export function dragView(view: Map3dView, dx: number, dy: number): Map3dView {
  return {
    ...view,
    turn: wrap(view.turn + dx * TURN_PER_PX),
    tilt: clampTilt(view.tilt + dy * TILT_PER_PX),
  };
}

/** The view after an arrow key, as a short drag that way, or null for
 *  any other key. */
export function keyView(view: Map3dView, key: string): Map3dView | null {
  switch (key) {
    case 'ArrowLeft':
      return dragView(view, -TURN_PER_KEY / TURN_PER_PX, 0);
    case 'ArrowRight':
      return dragView(view, TURN_PER_KEY / TURN_PER_PX, 0);
    case 'ArrowUp':
      return dragView(view, 0, -TILT_PER_KEY / TILT_PER_PX);
    case 'ArrowDown':
      return dragView(view, 0, TILT_PER_KEY / TILT_PER_PX);
    default:
      return null;
  }
}

/** North back at the top, at the default tilt. Floors and sprites stay. */
export function resetView(view: Map3dView): Map3dView {
  return { ...view, turn: 0, tilt: DEFAULT_TILT };
}

/** Whether north holds the top, within half a degree. */
export function isNorthUp(view: Map3dView): boolean {
  return Math.min(view.turn, 360 - view.turn) < 0.5;
}

/** Whether Reset view would change nothing. */
export function isResetView(view: Map3dView): boolean {
  return isNorthUp(view) && Math.abs(view.tilt - DEFAULT_TILT) < 0.5;
}
