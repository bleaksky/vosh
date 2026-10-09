import {
  combineDoorStates,
  doorStateAt,
  getCell,
  gridDims,
  hasExit,
  playerCellOf,
  sectorCodeOf,
  type Dir,
  type MapTilesPayload,
} from './mapTiles';
import type { WalkProgress } from '../../ipc/session';

// Click to walk on the tiles the game sends. A path runs over the
// lowercase n, e, s and w exits of your floor, the ones that land on
// the room in the next cell, and each step expects the room the `ex`
// of the cell it leaves names, which the walker checks against
// Room.Info.
//
// The walk routes around a closed, locked or hidden door, and around
// water (sector 7) and air (sector 9), even when you fly or carry a
// boat. With no other way it walks to the room before the first door or
// shore and stops there, and you take the last steps yourself. The game
// has the final word on every step it sends.

/** A cell of the grid, the packet's `g[row][col]`. */
export interface GridSpot {
  row: number;
  col: number;
}

/** How a walk ends. `open` reaches the room you clicked. `door` stops
 *  before a closed, locked or hidden door, and `shore` before water or
 *  air, since no open way reaches the room. */
export type WalkKind = 'open' | 'door' | 'shore';

/** A walk to a room you clicked. `steps` are the directions, `cells`
 *  the cell each step reaches and `rooms` the room num each step should
 *  reach, all in order. A `door` or `shore` walk can hold no steps when
 *  the door or the shore is the first step, so the map still marks the
 *  room as one with no way in. */
export interface WalkPlan {
  steps: Dir[];
  cells: GridSpot[];
  rooms: number[];
  kind: WalkKind;
}

/** Sectors a walk never enters, water that needs a boat and air that
 *  needs flight. */
const SHORE_SECTORS = new Set(['7', '9']);

const MOVES: Array<[Dir, Dir, number, number]> = [
  ['n', 's', -1, 0],
  ['e', 'w', 0, 1],
  ['s', 'n', 1, 0],
  ['w', 'e', 0, -1],
];

/** One step out of a cell, with what stands in its way. */
interface Edge extends GridSpot {
  dir: Dir;
  room: number;
  door: boolean;
  shore: boolean;
}

/** The steps out of the cell at row, col. Each takes a lowercase exit
 *  to the room in the next cell, and its door state joins the door
 *  there when that room's exit back is lowercase too, as the corridors
 *  draw it. A step whose `ex` names no room is left out, since the
 *  walker could not check where it lands. */
function edgesFrom(payload: MapTilesPayload, row: number, col: number): Edge[] {
  const cell = getCell(payload, row, col);
  if (!cell) return [];
  const out: Edge[] = [];
  for (const [dir, back, dr, dc] of MOVES) {
    const next = getCell(payload, row + dr, col + dc);
    if (!next || !hasExit(cell, dir)) continue;
    const room = Number(cell.ex?.[dir]);
    if (!Number.isInteger(room) || room <= 0) continue;
    const state = combineDoorStates(
      doorStateAt(cell, dir),
      hasExit(next, back) ? doorStateAt(next, back) : null,
    );
    out.push({
      row: row + dr,
      col: col + dc,
      dir,
      room,
      door: state !== null && state !== 'open',
      shore: SHORE_SECTORS.has(sectorCodeOf(next.s)),
    });
  }
  return out;
}

/** The shortest run of edges from `from` to `to`, breadth first in the
 *  order north, east, south, west, over the edges `may` lets through.
 *  Null when none reaches it. */
function search(
  payload: MapTilesPayload,
  from: GridSpot,
  to: GridSpot,
  may: (edge: Edge) => boolean,
): Edge[] | null {
  const key = (p: GridSpot) => `${p.row},${p.col}`;
  /** The edge each cell was reached by, and the cell it left. */
  const came = new Map<string, { edge: Edge; from: GridSpot } | null>([[key(from), null]]);
  const queue: GridSpot[] = [from];
  for (let i = 0; i < queue.length; i++) {
    const at = queue[i];
    if (at.row === to.row && at.col === to.col) break;
    for (const edge of edgesFrom(payload, at.row, at.col)) {
      if (!may(edge) || came.has(key(edge))) continue;
      came.set(key(edge), { edge, from: at });
      queue.push(edge);
    }
  }
  if (!came.has(key(to))) return null;
  const path: Edge[] = [];
  for (let step = came.get(key(to)); step; step = came.get(key(step.from))) {
    path.unshift(step.edge);
  }
  return path;
}

function planOf(path: Edge[], kind: WalkKind): WalkPlan {
  return {
    steps: path.map((edge) => edge.dir),
    cells: path.map(({ row, col }) => ({ row, col })),
    rooms: path.map((edge) => edge.room),
    kind,
  };
}

/** The walk from your room, or from the cell `from` on your floor, to
 *  the room at targetRow, targetCol, or null when there is none to
 *  offer. The room it starts from, an empty cell and a room no
 *  lowercase exits reach give null. Rooms on other floors are not in
 *  the grid, so they never take a walk. */
export function planWalk(
  payload: MapTilesPayload,
  targetRow: number,
  targetCol: number,
  from?: GridSpot,
): WalkPlan | null {
  const target = { row: targetRow, col: targetCol };
  if (!getCell(payload, targetRow, targetCol)) return null;
  const { rows, cols } = gridDims(payload);
  const you = from ?? playerCellOf(payload, rows, cols);
  if (you.row === targetRow && you.col === targetCol) return null;

  const open = search(payload, you, target, (edge) => !edge.door && !edge.shore);
  if (open) return planOf(open, 'open');

  const any = search(payload, you, target, () => true);
  if (!any) return null;
  const stop = any.findIndex((edge) => edge.door || edge.shore);
  return planOf(any.slice(0, stop), any[stop].door ? 'door' : 'shore');
}

/** The steps as a `#walk` string, each run of one direction as its
 *  count and letter, so n n n e e reads 3n2e. */
export function speedwalk(steps: Dir[]): string {
  let out = '';
  for (let i = 0; i < steps.length; ) {
    let j = i;
    while (j < steps.length && steps[j] === steps[i]) j++;
    out += (j - i > 1 ? String(j - i) : '') + steps[i];
    i = j;
  }
  return out;
}

/** A walk the map draws, to the room at `target`, under the pointer or
 *  under way. `cells` are the rooms the path passes after yours, in
 *  order, and `kind` says how it ends. A stopped walk holds `solid`, the
 *  legs from your room Vosh sent but never saw land, and the steps after
 *  them dash. */
export interface WalkMark {
  cells: GridSpot[];
  target: GridSpot;
  kind: WalkKind;
  solid?: number;
}

/** The route a click on the map sent, which the walk store keeps. The
 *  cells of the grid the click planned on and the room num of each,
 *  the room the walk starts from first and then in the order the steps
 *  reach them, the room clicked and how the walk ends. */
export interface WalkRoute {
  cells: GridSpot[];
  rooms: number[];
  target: GridSpot;
  kind: WalkKind;
}

/** The walk the map offers for `plan`, to the room at `target`. */
export function offerOf(plan: WalkPlan, target: GridSpot): WalkMark {
  return { cells: plan.cells, target, kind: plan.kind };
}

/** What is left of a walk a click sent, on the tiles the game sent
 *  last. The tiles center on the room you stand in, `here` as the last
 *  Room.Info names it, which the route finds by its num, so the route
 *  moves with you and holds still while a step goes unseen. Null with
 *  no walk, or once you stand off the route. */
export function walkAhead(
  payload: MapTilesPayload,
  route: WalkRoute,
  progress: WalkProgress,
  here: number | null,
): WalkMark | null {
  if (progress.kind === 'idle') return null;
  const { rows, cols } = gridDims(payload);
  const you = playerCellOf(payload, rows, cols);
  const at = here === null ? -1 : route.rooms.indexOf(here);
  if (at < 0) return null;
  const was = route.cells[at];
  const move = ({ row, col }: GridSpot) => ({
    row: row + you.row - was.row,
    col: col + you.col - was.col,
  });
  const mark = {
    cells: route.cells.slice(at + 1).map(move),
    target: move(route.target),
    kind: route.kind,
  };
  return progress.kind === 'stopped' ? { ...mark, solid: Math.max(0, progress.done - at) } : mark;
}

/** A room on your floor, its cell and its num. */
export interface WalkStart {
  cell: GridSpot;
  room: number;
}

/** Where the step on its way lands while you walk, so a click mid walk
 *  plans from there. The walker lets a new walk take over only once
 *  that step lands, and drops one planned from another room. A click
 *  walk names the room in its route, after `here`, the room the last
 *  Room.Info names. A walk you typed takes the first step it has left
 *  out of your cell, to the room that exit's `ex` names. Null when
 *  nothing walks or the step leaves your floor. */
export function stepOnItsWay(
  payload: MapTilesPayload,
  route: WalkRoute | null,
  progress: WalkProgress,
  here: number | null,
): WalkStart | null {
  if (progress.kind !== 'walking' || here === null) return null;
  const { rows, cols } = gridDims(payload);
  const you = playerCellOf(payload, rows, cols);
  if (progress.route) {
    const at = route ? route.rooms.indexOf(here) : -1;
    if (!route || at < 0 || at + 1 >= route.rooms.length) return null;
    const [was, next] = [route.cells[at], route.cells[at + 1]];
    return {
      cell: { row: next.row + you.row - was.row, col: next.col + you.col - was.col },
      room: route.rooms[at + 1],
    };
  }
  const dir = /[a-z]/.exec(progress.left)?.[0];
  const edge = edgesFrom(payload, you.row, you.col).find((e) => e.dir === dir);
  return edge ? { cell: { row: edge.row, col: edge.col }, room: edge.room } : null;
}
