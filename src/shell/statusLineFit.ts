// How the status line gives way while it carries your vitals and runs
// short. First your opponent's name ends in an ellipsis and goes, with
// a Target item on another mob, then the labels go, then Values falls
// back to Current, then the moons, then a fine round trip to the game,
// and then the game time go. The tick stays, and so does a round trip
// at 300 ms or more. Every step measures your vitals at their max with
// room kept for an opponent at 100 percent, and the round trip at its
// widest, so a fight or a value that loses a digit moves nothing. Kept
// pure for the tests, with every width in CSS px measured by the
// caller.

/** The 20 px between items, the 6 px between a label and its value or a
 *  name and its health, and the 8 px between the clock's parts, as
 *  frame.css sets them. */
const ITEM_GAP = 20;
const VALUE_GAP = 6;
const CLOCK_GAP = 8;

/** The least a name keeps before it goes, about four letters and the
 *  ellipsis. */
export const NAME_MIN = 48;

/** One vital as the line measures it. */
export interface StatusVitalWidths {
  label: number;
  /** The value at its max in your Values form. */
  value: number;
  /** The value at its max in Current. */
  current: number;
}

export interface StatusLineWidths {
  /** The room inside the line's 16 px sides. */
  room: number;
  /** Items that always stay ahead of your vitals, like Not connected,
   *  with their gaps. */
  lead: number;
  vitals: readonly StatusVitalWidths[];
  /** Your opponent's health at 100 percent, the room the line keeps
   *  for it, or null while your opponent is off. */
  foe: number | null;
  /** In a fight, so your opponent's name shows. */
  fighting: boolean;
  /** A Target item on another mob shows: its caption and the gap
   *  after it, 0 with none. */
  target: number;
  /** The round trip to the game at its widest, 0 before the first
   *  reading. */
  roundTrip: number;
  /** The round trip reads 300 ms or more, so it never gives way. */
  slow: boolean;
  /** The clock's parts, 0 for a part that does not show. */
  tick: number;
  time: number;
  moons: number;
}

/** What the line draws. */
export interface StatusLineFit {
  names: boolean;
  labels: boolean;
  /** Your Values form, or Current where it gave way. */
  current: boolean;
  moons: boolean;
  roundTrip: boolean;
  time: boolean;
}

/** Everything shown, for a line with room or one that does not fit. */
export const FIT_ALL: StatusLineFit = {
  names: true,
  labels: true,
  current: false,
  moons: true,
  roundTrip: true,
  time: true,
};

const STEPS: readonly ((fit: StatusLineFit, w: StatusLineWidths) => StatusLineFit)[] = [
  (fit) => ({ ...fit, names: false }),
  (fit) => ({ ...fit, labels: false }),
  (fit) => ({ ...fit, current: true }),
  (fit) => ({ ...fit, moons: false }),
  (fit, w) => (w.slow ? fit : { ...fit, roundTrip: false }),
  (fit) => ({ ...fit, time: false }),
];

/** The first fit in the give way order the line holds, or the last
 *  when none does. */
export function statusLineFit(widths: StatusLineWidths): StatusLineFit {
  let fit = FIT_ALL;
  for (const step of STEPS) {
    if (fits(widths, fit)) return fit;
    fit = step(fit, widths);
  }
  return fit;
}

function fits(w: StatusLineWidths, fit: StatusLineFit): boolean {
  const fixed = fixedWidth(w, fit);
  if (fixed > w.room) return false;
  if (!fit.names) return true;
  // The name and a Target item share what is left, each down to its
  // least.
  const names =
    (w.fighting && w.foe !== null ? VALUE_GAP + NAME_MIN : 0) +
    (w.target > 0 ? ITEM_GAP + w.target + NAME_MIN : 0);
  return w.room - fixed >= names;
}

/** The items that keep their width: your vitals, the room for your
 *  opponent's health, the round trip and the clock, with the gaps
 *  between them. */
function fixedWidth(w: StatusLineWidths, fit: StatusLineFit): number {
  const items = w.vitals.map(
    (v) => (fit.labels ? v.label + VALUE_GAP : 0) + (fit.current ? v.current : v.value),
  );
  if (w.foe !== null) items.push(w.foe);
  if (fit.roundTrip && w.roundTrip > 0) items.push(w.roundTrip);
  const clock = [w.tick, fit.time ? w.time : 0, fit.moons ? w.moons : 0].filter((p) => p > 0);
  if (clock.length > 0) items.push(sum(clock) + CLOCK_GAP * (clock.length - 1));
  const lead = w.lead > 0 ? w.lead + ITEM_GAP : 0;
  return lead + sum(items) + ITEM_GAP * Math.max(0, items.length - 1);
}

function sum(values: readonly number[]): number {
  return values.reduce((a, b) => a + b, 0);
}
