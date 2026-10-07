// The Automation list: grouping, ordering, and the filter. Items with
// no group come first under no heading. Named groups follow in
// alphabetical order, each under its name exactly as you typed it.
// Items keep their draft order inside a group, so a new item lands at
// the end of its group. Preset triggers that sit in no group of yours
// close the list under their own heading.
//
// Each heading folds its group away. A folded group shows its heading
// alone, and its rows leave the arrow key order. Each list remembers
// its folds in localStorage by section key, which for a group is `g:`
// and the group's name. While the filter has text, every group with a
// match shows open, and clearing the filter brings the folds back.

/** One row in the list. */
export interface ListEntry {
  uid: string;
  /** The row text. */
  name: string;
  /** Quieter text after the name, like a macro's command. */
  meta?: string;
  /** The group as typed and trimmed. Empty means no group. */
  group: string;
  enabled: boolean;
  /** Everything the filter searches, in lower case. */
  text: string;
  /** Sits in the trailing preset section when it has no group. */
  preset?: boolean;
  /** How the dot draws while the item is off. `suggested` wears the
   *  accent ring of a preset suggested for your world. */
  dot?: 'suggested';
  /** Deep link and search anchor on the row, like `presets:herb_labels`. */
  anchor?: string;
}

export interface ListSection {
  /** Stable React key. */
  key: string;
  /** The heading, or null for the ungrouped items at the top. */
  heading: string | null;
  entries: ListEntry[];
}

export const PRESET_SECTION_HEADING = 'From presets';
// Section keys. A group's is `g:` and its name, so the three never meet.
// They stay plain text, since the list finds a heading by its key.
const UNGROUPED_KEY = 'u:';
const PRESET_KEY = 'p:';

/** The group an item files under: trimmed, empty for none. */
export function groupKeyOf(group: string | null | undefined): string {
  return typeof group === 'string' ? group.trim() : '';
}

function compareGroups(a: string, b: string): number {
  const base = a.localeCompare(b, undefined, { sensitivity: 'base' });
  if (base !== 0) return base;
  return a < b ? -1 : a > b ? 1 : 0;
}

/** The key of the section an entry files under. */
export function sectionKeyOf(entry: Pick<ListEntry, 'group' | 'preset'>): string {
  if (entry.group !== '') return `g:${entry.group}`;
  return entry.preset ? PRESET_KEY : UNGROUPED_KEY;
}

/** The group a section stands for, by its key, or null for the
 *  ungrouped items and the presets, which no switch turns. */
export function groupOfSectionKey(key: string): string | null {
  return key.startsWith('g:') ? key.slice(2) : null;
}

/** Sort entries into sections: ungrouped first, named groups in
 *  alphabetical order, ungrouped presets last. */
export function buildSections(entries: readonly ListEntry[]): ListSection[] {
  const ungrouped: ListEntry[] = [];
  const presets: ListEntry[] = [];
  const groups = new Map<string, ListEntry[]>();
  for (const entry of entries) {
    if (entry.group === '') {
      (entry.preset ? presets : ungrouped).push(entry);
      continue;
    }
    const list = groups.get(entry.group);
    if (list) list.push(entry);
    else groups.set(entry.group, [entry]);
  }
  const sections: ListSection[] = [];
  if (ungrouped.length > 0)
    sections.push({ key: UNGROUPED_KEY, heading: null, entries: ungrouped });
  for (const name of [...groups.keys()].sort(compareGroups)) {
    sections.push({
      key: sectionKeyOf({ group: name }),
      heading: name,
      entries: groups.get(name) ?? [],
    });
  }
  if (presets.length > 0) {
    sections.push({ key: PRESET_KEY, heading: PRESET_SECTION_HEADING, entries: presets });
  }
  return sections;
}

/** Keep the sections whose entries match `query`. Every word must
 *  appear in the entry's text. An empty query keeps everything. */
export function filterSections(sections: readonly ListSection[], query: string): ListSection[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return sections.slice();
  const out: ListSection[] = [];
  for (const section of sections) {
    const entries = section.entries.filter((entry) =>
      words.every((word) => entry.text.includes(word)),
    );
    if (entries.length > 0) out.push({ ...section, entries });
  }
  return out;
}

/** The uids in the order the list shows them. */
export function sectionOrder(sections: readonly ListSection[]): string[] {
  const out: string[] = [];
  for (const section of sections) for (const entry of section.entries) out.push(entry.uid);
  return out;
}

/** The key a section folds by, or null for the ungrouped items at the
 *  top, which have no heading and never fold. */
export function foldKeyOf(section: ListSection): string | null {
  return section.heading === null ? null : section.key;
}

/** Whether a section shows its rows. */
export function isSectionOpen(section: ListSection, folded: ReadonlySet<string>): boolean {
  const key = foldKeyOf(section);
  return key === null || !folded.has(key);
}

/** A place the arrow keys stop: a group heading or a row. */
export type ListStop = { kind: 'heading'; key: string } | { kind: 'row'; uid: string };

/** A stop as one string, to compare and to store. */
export function stopId(stop: ListStop): string {
  return stop.kind === 'heading' ? `heading:${stop.key}` : `row:${stop.uid}`;
}

/** The stops in the order the list shows them: each heading, then its
 *  rows while its group is open. */
export function listStops(
  sections: readonly ListSection[],
  folded: ReadonlySet<string>,
): ListStop[] {
  const out: ListStop[] = [];
  for (const section of sections) {
    const key = foldKeyOf(section);
    if (key !== null) out.push({ kind: 'heading', key });
    if (key !== null && folded.has(key)) continue;
    for (const entry of section.entries) out.push({ kind: 'row', uid: entry.uid });
  }
  return out;
}

/** The uids of the rows you can see, in list order. */
export function visibleOrder(
  sections: readonly ListSection[],
  folded: ReadonlySet<string>,
): string[] {
  const out: string[] = [];
  for (const section of sections) {
    if (!isSectionOpen(section, folded)) continue;
    for (const entry of section.entries) out.push(entry.uid);
  }
  return out;
}

/** The fold key of the folded group that hides `uid`, or null when the
 *  row shows or is not in the list. */
export function foldedGroupOf(
  sections: readonly ListSection[],
  folded: ReadonlySet<string>,
  uid: string,
): string | null {
  for (const section of sections) {
    if (!section.entries.some((entry) => entry.uid === uid)) continue;
    return isSectionOpen(section, folded) ? null : foldKeyOf(section);
  }
  return null;
}

/** The stop that takes Tab. A heading you moved to keeps it until the
 *  selection changes. Otherwise the selected row takes it, or the
 *  heading of the folded group that hides it. With the selection
 *  outside the list, as while the filter leaves it out, the first row
 *  takes it, so Tab and Enter from the filter pick the first match and
 *  never fold it away. A heading takes it only when no row shows. */
export function tabStopId(
  stops: readonly ListStop[],
  sections: readonly ListSection[],
  folded: ReadonlySet<string>,
  selected: string | null,
  heading: string | null,
): string | null {
  const ids = stops.map(stopId);
  if (heading !== null && ids.includes(heading)) return heading;
  if (selected !== null) {
    const row = stopId({ kind: 'row', uid: selected });
    if (ids.includes(row)) return row;
    const hiding = foldedGroupOf(sections, folded, selected);
    if (hiding !== null) return stopId({ kind: 'heading', key: hiding });
  }
  const first = stops.find((stop) => stop.kind === 'row') ?? stops[0];
  return first ? stopId(first) : null;
}

/** What a key press on the list does. */
export type ListKeyAction =
  | { type: 'move'; to: ListStop }
  | { type: 'fold'; key: string; fold: boolean }
  | null;

/** What `key` does with focus on the stop `at`. Up and Down step
 *  through headings and rows as one list, Home and End jump to its
 *  ends, and Left folds a heading while Right opens it. Null leaves the
 *  key to the page. */
export function listKeyAction(
  stops: readonly ListStop[],
  at: string | null,
  key: string,
): ListKeyAction {
  if (stops.length === 0) return null;
  const index = at === null ? -1 : stops.findIndex((stop) => stopId(stop) === at);
  if (key === 'ArrowLeft' || key === 'ArrowRight') {
    const stop = stops[index];
    if (!stop || stop.kind !== 'heading') return null;
    return { type: 'fold', key: stop.key, fold: key === 'ArrowLeft' };
  }
  let next: number;
  if (key === 'ArrowDown') next = Math.min(stops.length - 1, index + 1);
  else if (key === 'ArrowUp') next = Math.max(0, index - 1);
  else if (key === 'Home') next = 0;
  else if (key === 'End') next = stops.length - 1;
  else return null;
  return { type: 'move', to: stops[next] };
}

/** Where a list keeps its folds, like `vosh.automation.folded.triggers`. */
export function foldedStorageKey(list: string): string {
  return `vosh.automation.folded.${list}`;
}

/** The folds a list remembers. Storage that refuses to read, or holds
 *  something else, folds nothing. */
export function loadFolded(storage: Pick<Storage, 'getItem'> | null, list: string): Set<string> {
  try {
    const raw = storage?.getItem(foldedStorageKey(list));
    if (!raw) return new Set();
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((key): key is string => typeof key === 'string'));
  } catch {
    return new Set();
  }
}

/** Remember a list's folds. A list with none folded leaves no key. */
export function saveFolded(
  storage: Pick<Storage, 'setItem' | 'removeItem'> | null,
  list: string,
  folded: ReadonlySet<string>,
): void {
  try {
    const key = foldedStorageKey(list);
    if (folded.size === 0) storage?.removeItem(key);
    else storage?.setItem(key, JSON.stringify([...folded].sort()));
  } catch {
    // Storage can refuse to write. The folds then last while the list stays open.
  }
}

/** `folded` with `key` folded or open. */
export function withFold(
  folded: ReadonlySet<string>,
  key: string,
  fold: boolean,
): ReadonlySet<string> {
  if (folded.has(key) === fold) return folded;
  const next = new Set(folded);
  if (fold) next.add(key);
  else next.delete(key);
  return next;
}

/** Folds made while the filter has text, for that text alone. */
export interface SearchFolds {
  filter: string;
  folded: ReadonlySet<string>;
}

const NONE: ReadonlySet<string> = new Set();

/** Whether the filter has text to match. */
export function isFiltering(filter: string): boolean {
  return filter.trim() !== '';
}

/** The folds the list shows. With no filter, the ones you keep. While
 *  the filter has text, every group with a match opens, and only what
 *  you fold for that same text stays folded. */
export function foldsInView(
  kept: ReadonlySet<string>,
  search: SearchFolds | null,
  filter: string,
): ReadonlySet<string> {
  if (!isFiltering(filter)) return kept;
  return search && search.filter === filter ? search.folded : NONE;
}

/** The uid to select after `uid` leaves the list: the next one in
 *  `order`, else the one before it, else none. */
export function neighborUid(order: readonly string[], uid: string): string | null {
  const index = order.indexOf(uid);
  if (index === -1) return order[0] ?? null;
  return order[index + 1] ?? order[index - 1] ?? null;
}

/** The text the filter searches, built from the parts of an item. */
export function searchText(...parts: (string | null | undefined)[]): string {
  return parts
    .filter((p): p is string => typeof p === 'string' && p.length > 0)
    .join('\n')
    .toLowerCase();
}
