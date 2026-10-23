// Picking several rows of an Automation list, to change them all at
// once. While picking is off the list selects one row and its card
// shows. Select beside the filter, or a Cmd or Ctrl click, or a Shift
// click turns it on: each row and group heading wears a checkbox, a
// click checks or clears a row, and the card acts on every checked row.

/** The rows picked for one change. */
export interface ListPick {
  /** Picking is on, so a click checks a row instead of opening it. */
  on: boolean;
  /** The checked rows, in the order you checked them. */
  uids: readonly string[];
  /** Where a Shift click or a Shift arrow measures its run from. */
  anchor: string | null;
}

export const NO_PICK: ListPick = { on: false, uids: [], anchor: null };

/** Picking with the row you were on checked, or none. */
export function startPick(uid: string | null): ListPick {
  return { on: true, uids: uid === null ? [] : [uid], anchor: uid };
}

/** `pick` with `uid` checked or cleared. From the row you were on, a
 *  Cmd or Ctrl click checks both. The anchor moves to `uid`. */
export function togglePick(pick: ListPick, uid: string, current: string | null): ListPick {
  const from = pick.on ? pick : startPick(current);
  const has = from.uids.includes(uid);
  const uids = has ? from.uids.filter((u) => u !== uid) : [...from.uids, uid];
  return { on: true, uids, anchor: uid };
}

/** The rows of `order` from the anchor to `uid`, both ends in. The run
 *  takes the place of what was checked, so a Shift click can shrink it
 *  too. With no anchor in `order` it starts at `current`, the row you
 *  were on. */
export function rangePick(
  pick: ListPick,
  uid: string,
  order: readonly string[],
  current: string | null,
): ListPick {
  const anchor = pick.on && pick.anchor !== null ? pick.anchor : current;
  const to = order.indexOf(uid);
  if (to === -1) return pick;
  const from = anchor === null ? -1 : order.indexOf(anchor);
  if (from === -1) return { on: true, uids: [uid], anchor: uid };
  const [lo, hi] = from <= to ? [from, to] : [to, from];
  return { on: true, uids: order.slice(lo, hi + 1), anchor };
}

/** `pick` with every row of `uids` checked, or every one cleared, as
 *  the checkbox on a group heading does. */
export function groupPick(pick: ListPick, uids: readonly string[], check: boolean): ListPick {
  const base = pick.on ? pick.uids : [];
  const drop = new Set(uids);
  const kept = base.filter((u) => !drop.has(u));
  return {
    on: true,
    uids: check ? [...kept, ...uids] : kept,
    anchor: check ? (uids[0] ?? pick.anchor) : pick.anchor,
  };
}

/** Every row of `order` checked, the rows you can see. */
export function allPick(order: readonly string[]): ListPick {
  return { on: true, uids: [...order], anchor: order[0] ?? null };
}

/** `pick` without the rows `exists` no longer knows, as after a delete
 *  or a load. The same pick when nothing went. */
export function prunePick(pick: ListPick, exists: (uid: string) => boolean): ListPick {
  if (pick.uids.every(exists)) return pick;
  const uids = pick.uids.filter(exists);
  const anchor = pick.anchor !== null && exists(pick.anchor) ? pick.anchor : (uids[0] ?? null);
  return { ...pick, uids, anchor };
}

/** How many of `uids` are checked: all, some or none. A group heading's
 *  checkbox shows it, the some as a dash. */
export function pickState(
  checked: ReadonlySet<string>,
  uids: readonly string[],
): 'all' | 'some' | 'none' {
  let n = 0;
  for (const uid of uids) if (checked.has(uid)) n += 1;
  if (n === 0) return 'none';
  return n === uids.length ? 'all' : 'some';
}
