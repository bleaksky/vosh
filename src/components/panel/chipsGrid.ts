import { hoursTone, isTrackedRow, type AffectRow } from '../../lib/affectsView';
import { AFFECTS_TWO_COLUMNS_W } from './affectsGrid';

// Where each chip sits in the Grouped chips style (board Affects C).
// Three groups, what to recast first: the tracked affects you miss and
// the ones running out, then the rest you track, then everything else.
// Chips pack into lines 20 tall, 4 apart inside a group and 8 apart
// between groups, from x 18 to 12 from the right, the group's name in
// a 56 px gutter or, under 360 px, run in at the start of its first
// line. The pane draws exactly the lines this packs, so the pane, its
// minimum, and the tests agree. CSS flex wrap never decides.
//
// A page is as tall as the body and holds whole lines. A page that
// cannot hold everything left ends with the count of what follows on
// its last line, and a click on the count scrolls one page on. Pure,
// with the text measure handed in, so the fit is unit tested.

export type ChipGroupId = 'recast' | 'tracked' | 'other';

export interface ChipGroup {
  id: ChipGroupId;
  label: string;
  rows: AffectRow[];
}

/** A row's place by its hours: timed, then permanent, then unknown. */
function rank(ticks: number | null): number {
  if (ticks === null) return Number.MAX_SAFE_INTEGER;
  if (ticks < 0) return Number.MAX_SAFE_INTEGER - 1;
  return ticks;
}

/** Board C's three groups, empty ones dropped. Recast: the tracked
 *  affects you miss, in your order, then the tracked ones running out,
 *  fewest hours first. Tracked: the rest you track, in your order.
 *  Other: everything else as affectsView orders it, harmful first, then
 *  by hours, then by name. */
export function chipGroups(rows: readonly AffectRow[]): ChipGroup[] {
  const tracked = rows.filter(isTrackedRow);
  const recast = [
    ...tracked.filter((r) => r.state === 'missing'),
    ...tracked.filter((r) => r.state === 'expiring').sort((a, b) => rank(a.ticks) - rank(b.ticks)),
  ];
  const groups: ChipGroup[] = [
    { id: 'recast', label: 'Recast', rows: recast },
    { id: 'tracked', label: 'Tracked', rows: tracked.filter((r) => r.state === 'present') },
    { id: 'other', label: 'Other', rows: rows.filter((r) => !isTrackedRow(r)) },
  ];
  return groups.filter((g) => g.rows.length > 0);
}

/** One chip line, 20 tall. */
export const CHIP_H = 20;
/** Between chips on a line, and between the lines of a group. */
export const CHIP_GAP = 4;
/** Between groups. */
export const GROUP_GAP = 8;
/** Above the first line of a page. */
export const CHIPS_TOP = 4;
/** The label gutter, and the space after a label. */
export const GUTTER_W = 56;
export const GUTTER_GAP = 8;
/** The pane's text edges: chips run from x 18 to 12 from the right. */
export const CHIPS_LEFT = 18;
export const CHIPS_RIGHT = 12;

/** Where the group names go. `gutter` is the board's 56 px column,
 *  `runin` sets the name at the start of its group's first line in a
 *  pane under 360 px, and `none` draws no names when you track
 *  nothing, so every chip is Other and a name would say nothing. */
export type LabelMode = 'gutter' | 'runin' | 'none';

export function chipLabelMode(rows: readonly AffectRow[], width: number): LabelMode {
  if (!rows.some(isTrackedRow)) return 'none';
  return width < AFFECTS_TWO_COLUMNS_W ? 'runin' : 'gutter';
}

/** Text widths in the faces the pane draws. */
export interface ChipMeasure {
  /** A name or hours in the terminal face at 12 px. */
  mono: (s: string) => number;
  /** The hours in the terminal face at 12 px and the heaviest weight
   *  they draw in, so a face whose bold runs wider never overflows its
   *  line. The name's measure when left out. */
  hours?: (s: string) => number;
  /** A group name in the UI face at 600 11 px. */
  label: (s: string) => number;
  /** The count, `N more`, in the UI face at 12 px. */
  count: (s: string) => number;
}

/** Widths with no page to measure in, as in a test: 7.2 px a terminal
 *  cell, close to the system face otherwise. */
export const FIXED_MEASURE: ChipMeasure = {
  mono: (s) => s.length * 7.2,
  label: (s) => s.length * 6.5,
  count: (s) => s.length * 6.6,
};

/** A chip's width: 7 px each side (6 and the 1 px dashed border while
 *  missing), the name, then 6 px and the hours when it has any. */
export function chipWidth(name: string, hours: string, measure: ChipMeasure): number {
  const hoursW = measure.hours ?? measure.mono;
  return Math.ceil(7 + measure.mono(name) + (hours ? 6 + hoursW(hours) : 0) + 7);
}

export interface ChipLine {
  group: ChipGroupId;
  /** The group's name shows on this line: the group's first line, or
   *  the first line of a page that starts inside the group. */
  labelled: boolean;
  rows: AffectRow[];
  /** Top of the line from the top of its page. */
  top: number;
}

export interface ChipPage {
  lines: ChipLine[];
  /** Affects on later pages, counted after the page's last chip. */
  more: number;
}

/** Pack `groups` into pages of whole lines, each page `bodyH` tall, in
 *  a pane `width` wide. `hoursOf` is the text a chip shows for its
 *  hours. A page that cannot hold everything left ends with the count
 *  on its last line: while that line has no room for 8 px and the
 *  count, its last chip moves to the next page, keeping at least one
 *  chip on the page. A chip never grows past its line, and its name
 *  ellipsizes instead. */
export function chipPages(
  groups: readonly ChipGroup[],
  width: number,
  hoursOf: (row: AffectRow) => string,
  measure: ChipMeasure,
  labels: LabelMode,
  bodyH: number,
): ChipPage[] {
  const inner =
    width - CHIPS_LEFT - CHIPS_RIGHT - (labels === 'gutter' ? GUTTER_W + GUTTER_GAP : 0);
  const labelOf = (id: ChipGroupId) => groups.find((g) => g.id === id)?.label ?? '';
  const lead = (line: { group: ChipGroupId; labelled: boolean }) =>
    labels === 'runin' && line.labelled
      ? Math.ceil(measure.label(labelOf(line.group))) + GUTTER_GAP
      : 0;
  const queue: { group: ChipGroupId; row: AffectRow }[] = [];
  for (const g of groups) for (const row of g.rows) queue.push({ group: g.id, row });
  const natural = (row: AffectRow) => chipWidth(row.name, hoursOf(row), measure);
  // What a line's chips take, each chip no wider than the room it has.
  const used = (line: ChipLine) => {
    let at = lead(line);
    line.rows.forEach((row, k) => {
      const gap = k === 0 ? 0 : CHIP_GAP;
      at += gap + Math.min(natural(row), Math.max(0, inner - at - gap));
    });
    return at;
  };

  const pages: ChipPage[] = [];
  let i = 0;
  while (i < queue.length) {
    const start = i;
    const lines: ChipLine[] = [];
    let line: ChipLine | null = null;
    let full = false;
    while (i < queue.length) {
      const { group, row } = queue[i];
      const newGroup: boolean = !line || line.group !== group;
      if (line && !newGroup && used(line) + CHIP_GAP + natural(row) <= inner) {
        line.rows.push(row);
        i += 1;
        continue;
      }
      // A new line: the next group, or this one wrapping.
      const top: number = line ? line.top + CHIP_H + (newGroup ? GROUP_GAP : CHIP_GAP) : CHIPS_TOP;
      if (top + CHIP_H > bodyH && lines.length > 0) {
        full = true;
        break;
      }
      line = {
        group,
        labelled: labels !== 'none' && (newGroup || lines.length === 0),
        rows: [row],
        top,
      };
      lines.push(line);
      i += 1;
    }
    if (full) {
      const countW = (n: number) => GUTTER_GAP + Math.ceil(measure.count(`${n} more`));
      for (;;) {
        const last = lines[lines.length - 1];
        if (inner - used(last) >= countW(queue.length - i)) break;
        if (lines.length === 1 && last.rows.length === 1) break;
        last.rows.pop();
        i -= 1;
        if (last.rows.length === 0) lines.pop();
      }
    }
    pages.push({ lines, more: queue.length - i });
    // A body too short for one line still shows it, and scrolls.
    if (i === start) break;
  }
  return pages;
}

/** The least body height whose first page holds every Recast and
 *  Tracked chip and every harmful one, the rows Timers first holds, and
 *  the count after them when more follow. 0 when nothing needs holding,
 *  and never more than `cap`. */
export function chipsMinBody(
  groups: readonly ChipGroup[],
  width: number,
  hoursOf: (row: AffectRow) => string,
  measure: ChipMeasure,
  labels: LabelMode,
  cap: number,
): number {
  const must = groups.flatMap((g) =>
    g.rows.filter((r) => g.id !== 'other' || r.state === 'harmful'),
  );
  if (must.length === 0) return 0;
  for (let h = CHIPS_TOP + CHIP_H; h <= cap; h += 1) {
    const [first] = chipPages(groups, width, hoursOf, measure, labels, h);
    const shown = new Set(first?.lines.flatMap((l) => l.rows.map((r) => r.key)) ?? []);
    if (must.every((r) => shown.has(r.key))) return h;
  }
  return cap;
}

export type ChipKind = 'missing' | 'harmful' | 'tracked' | 'other';

export function chipKind(row: AffectRow): ChipKind {
  if (row.state === 'missing') return 'missing';
  if (row.state === 'harmful') return 'harmful';
  return isTrackedRow(row) ? 'tracked' : 'other';
}

/** The tone a whole chip takes: a tracked affect running out, yellow at
 *  two hours and red at one or none, as board C's Recast group draws
 *  it. Elsewhere the tone only colors the hours. */
export function chipTone(row: AffectRow): 'warn' | 'danger' | null {
  return row.state === 'expiring' ? hoursTone(row.ticks) : null;
}
