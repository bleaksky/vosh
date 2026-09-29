// The Automation list: grouping, ordering, and the filter. Items with
// no group come first under no heading. Named groups follow in
// alphabetical order, each under its name exactly as you typed it.
// Items keep their draft order inside a group, so a new item lands at
// the end of its group. Preset triggers that sit in no group of yours
// close the list under their own heading.

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
}

export interface ListSection {
  /** Stable React key. */
  key: string;
  /** The heading, or null for the ungrouped items at the top. */
  heading: string | null;
  entries: ListEntry[];
}

export const PRESET_SECTION_HEADING = 'From presets';
const UNGROUPED_KEY = '\u0000ungrouped';
const PRESET_KEY = '\u0000presets';

/** The group an item files under: trimmed, empty for none. */
export function groupKeyOf(group: string | null | undefined): string {
  return typeof group === 'string' ? group.trim() : '';
}

function compareGroups(a: string, b: string): number {
  const base = a.localeCompare(b, undefined, { sensitivity: 'base' });
  if (base !== 0) return base;
  return a < b ? -1 : a > b ? 1 : 0;
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
    sections.push({ key: `g:${name}`, heading: name, entries: groups.get(name) ?? [] });
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
