import { describe, expect, it } from 'vitest';
import { aabahranMapPacket } from '../../test/aabahranGmcp';
import { getCell, type MapTilesPayload, type ServerCell } from './mapTiles';
import { planWalk, speedwalk, type WalkPlan } from './mapWalk';

// The packets come from fixtures/gmcp/aabahran/map, built by the game's
// own generate_map and gmcp_send_map over its area files. A test that
// needs a door or a sector the captures lack changes one cell of a copy.

function tiles(name: string): MapTilesPayload {
  const { package: pkg, data } = aabahranMapPacket(name);
  expect(pkg, name).toBe('Map.Tiles');
  return structuredClone(data) as MapTilesPayload;
}

/** You stand in The Central Square of Val Miran, room 20605, radius 10,
 *  at [10][10]. */
const VAL_MIRAN = 'val-miran-central-square.gmcp';
/** You stand West of the City Fountain in Caranduin, radius 7. */
const CARANDUIN = 'caranduin-west-of-the-fountain.gmcp';
/** You stand at An Enormous Gate in Mahn-Tor's Dungeon, radius 7, and
 *  the only way on is a locked door to the south. */
const MAHN_TOR = 'mahn-tor-an-enormous-gate.gmcp';

function cell(payload: MapTilesPayload, row: number, col: number): ServerCell {
  const found = getCell(payload, row, col);
  if (!found) throw new Error(`no room at ${row},${col}`);
  return found;
}

/** Each step's room is the one the `ex` of the cell it leaves names,
 *  starting from you at [radius][radius]. */
function expectRoomsFromEx(payload: MapTilesPayload, plan: WalkPlan, radius = 10): void {
  let at = { row: radius, col: radius };
  plan.steps.forEach((dir, i) => {
    expect(plan.rooms[i]).toBe(Number(cell(payload, at.row, at.col).ex?.[dir]));
    at = plan.cells[i];
  });
}

describe('planWalk', () => {
  it("walks ten steps north to The Forest's Edge with the rooms from ex", () => {
    const payload = tiles(VAL_MIRAN);
    const plan = planWalk(payload, 0, 10);
    expect(plan).toEqual({
      steps: Array(10).fill('n'),
      cells: Array.from({ length: 10 }, (_, i) => ({ row: 9 - i, col: 10 })),
      rooms: [20604, 20603, 20602, 20601, 20671, 20672, 20675, 20676, 20677, 8190],
      kind: 'open',
    });
    expectRoomsFromEx(payload, plan!);
  });

  it('routes around a closed door when the tiles show another way', () => {
    const payload = tiles(CARANDUIN);
    expect(speedwalk(planWalk(payload, 1, 8)!.steps)).toBe('ne5n');
    cell(payload, 4, 8).d = { n: 'closed' };
    const plan = planWalk(payload, 1, 8);
    expect(plan?.kind).toBe('open');
    expect(speedwalk(plan!.steps)).toBe('ne2nenw2n');
    expectRoomsFromEx(payload, plan!, 7);
  });

  it('stops before the locked door in Val Miran with kind door', () => {
    // The room at [7][15] has one way in, its locked door north.
    const plan = planWalk(tiles(VAL_MIRAN), 7, 15);
    expect(plan).toMatchObject({ kind: 'door' });
    expect(speedwalk(plan!.steps)).toBe('4n5e');
    expect(plan!.cells.at(-1)).toEqual({ row: 6, col: 15 });
  });

  it('stops before a closed door in Caranduin', () => {
    const plan = planWalk(tiles(CARANDUIN), 7, 14);
    expect(plan?.kind).toBe('door');
    expect(speedwalk(plan!.steps)).toBe('n2es4e');
    expect(plan!.cells.at(-1)).toEqual({ row: 7, col: 13 });
  });

  it('offers no steps behind the locked gate of Mahn-Tor and still says door', () => {
    expect(planWalk(tiles(MAHN_TOR), 8, 7)).toEqual({
      steps: [],
      cells: [],
      rooms: [],
      kind: 'door',
    });
  });

  it('stops on the shore before water with kind shore', () => {
    // [10][4] is water, sector 7, and the only way west.
    const plan = planWalk(tiles(VAL_MIRAN), 10, 2);
    expect(plan).toMatchObject({ kind: 'shore', rooms: [20614, 20615, 20616, 20617, 20621] });
    expect(speedwalk(plan!.steps)).toBe('5w');
  });

  it('routes around air and stops before air it has to enter', () => {
    const payload = tiles(CARANDUIN);
    cell(payload, 6, 7).s = 9;
    const around = planWalk(payload, 1, 8);
    expect(around?.kind).toBe('open');
    expect(speedwalk(around!.steps)).toBe('s2e2nw5n');
    expect(around!.cells).not.toContainEqual({ row: 6, col: 7 });
    expect(planWalk(payload, 6, 7)).toMatchObject({ kind: 'shore', steps: [] });
  });

  it('offers nothing for a step whose ex names no room', () => {
    const payload = tiles(VAL_MIRAN);
    delete cell(payload, 10, 10).ex;
    expect(planWalk(payload, 9, 10)).toBeNull();
  });

  it('offers nothing for your own room or an empty cell', () => {
    const payload = tiles(VAL_MIRAN);
    expect(planWalk(payload, 10, 10)).toBeNull();
    expect(getCell(payload, 0, 0)).toBeNull();
    expect(planWalk(payload, 0, 0)).toBeNull();
  });
});

describe('speedwalk', () => {
  it('writes each run as its count and letter', () => {
    expect(speedwalk(['n', 'n', 'n', 'e', 'e'])).toBe('3n2e');
    expect(speedwalk(['n', 'e', 'n'])).toBe('nen');
    expect(speedwalk([])).toBe('');
  });
});
