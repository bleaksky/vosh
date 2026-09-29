// The rows band under the Map pane's drawing: the room you stand in,
// then the people here. Its height follows the pane alone, never who
// is in the room, so the drawing keeps one size while you walk and
// people come and go. Pure so the counts are unit tested.

/** The most rows the band holds, the room row included: the room and
 *  two people, as on the approved board. Every row here is height the
 *  drawing gives up, filled or not. */
export const MAP_BAND_MAX_ROWS = 3;

/** How many rows the band holds. `shared` is the height the drawing
 *  and the band split between them, `floor` the drawing's minimum, and
 *  `row` one dense row. The room row always stays. */
export function mapBandRows(shared: number, floor: number, row: number): number {
  if (!(row > 0)) return 1;
  const fit = Math.floor((shared - floor + 0.5) / row);
  return Math.max(1, Math.min(MAP_BAND_MAX_ROWS, fit));
}

export interface BandPeople<T> {
  shown: T[];
  /** People past the last slot, which that slot counts instead. Empty
   *  when everyone fits, or when no slot is left to count them in. */
  rest: T[];
}

/** The people rows for `slots` rows under the room row. When more
 *  people are here than fit, the last slot counts the rest instead. */
export function mapBandPeople<T>(people: T[], slots: number): BandPeople<T> {
  const n = Math.max(0, slots);
  if (people.length <= n) return { shown: people, rest: [] };
  if (n === 0) return { shown: [], rest: [] };
  return { shown: people.slice(0, n - 1), rest: people.slice(n - 1) };
}
