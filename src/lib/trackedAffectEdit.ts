import { normalizeAffectName } from './affects';
import { sentenceCase } from './affectsView';
import type { TrackedAffect } from './session';

// Editing a profile's tracked affects in Settings > Characters: the
// chips, Add affect, and the labels and order under Advanced. Pure, so
// the page only sends the list each edit returns. Every edit that
// changes nothing hands back the same list, so the page can skip the
// save. Names match the way the Affects pane matches them: case and
// runs of spaces do not count.

/** What a chip and the Affects pane show: your label, else the
 *  server's name in sentence case. */
export function trackedAffectLabel(entry: TrackedAffect): string {
  const label = entry.label?.trim();
  return label || sentenceCase(entry.name);
}

export function isTracked(list: readonly TrackedAffect[], name: string): boolean {
  const key = normalizeAffectName(name);
  return list.some((entry) => normalizeAffectName(entry.name) === key);
}

/** Track `name` last, unless it is blank or already tracked. */
export function addTrackedAffect(list: TrackedAffect[], name: string): TrackedAffect[] {
  const clean = name.replace(/\s+/g, ' ').trim();
  if (clean.length === 0 || isTracked(list, clean)) return list;
  return [...list, { name: clean, label: null }];
}

export function removeTrackedAffect(list: TrackedAffect[], index: number): TrackedAffect[] {
  if (index < 0 || index >= list.length) return list;
  return list.filter((_, i) => i !== index);
}

/** Move the entry at `index` by `delta` places, stopping at either
 *  end. */
export function moveTrackedAffect(
  list: TrackedAffect[],
  index: number,
  delta: number,
): TrackedAffect[] {
  const to = Math.max(0, Math.min(list.length - 1, index + delta));
  if (index < 0 || index >= list.length || to === index) return list;
  const next = list.slice();
  const [moved] = next.splice(index, 1);
  next.splice(to, 0, moved);
  return next;
}

/** Show the entry at `index` as `label`. A blank label shows the
 *  server's name again. */
export function setTrackedAffectLabel(
  list: TrackedAffect[],
  index: number,
  label: string,
): TrackedAffect[] {
  const entry = list[index];
  if (!entry) return list;
  const clean = label.trim();
  const next = clean.length > 0 ? clean : null;
  if ((entry.label ?? null) === next) return list;
  return list.map((e, i) => (i === index ? { ...e, label: next } : e));
}

/** A suggestion for Add affect. `name` is what the server sends, which
 *  is what the profile tracks. `label` is how the chip will read. */
export interface AffectSuggestion {
  name: string;
  label: string;
}

/** Affects on you now that the profile does not track, for Add affect.
 *  With a query, only names that contain it, the ones that start with
 *  it first. Alphabetical otherwise. */
export function affectSuggestions(
  current: readonly { name: string }[] | null,
  tracked: readonly TrackedAffect[],
  query: string,
  limit = 8,
): AffectSuggestion[] {
  if (!current) return [];
  const want = normalizeAffectName(query);
  const seen = new Set(tracked.map((t) => normalizeAffectName(t.name)));
  const found: { suggestion: AffectSuggestion; key: string; starts: boolean }[] = [];
  for (const affect of current) {
    const key = normalizeAffectName(affect.name);
    if (key.length === 0 || seen.has(key)) continue;
    seen.add(key);
    if (want.length > 0 && !key.includes(want)) continue;
    found.push({
      suggestion: {
        name: affect.name.replace(/\s+/g, ' ').trim(),
        label: sentenceCase(affect.name),
      },
      key,
      starts: want.length > 0 && key.startsWith(want),
    });
  }
  found.sort((a, b) => Number(b.starts) - Number(a.starts) || a.key.localeCompare(b.key));
  return found.slice(0, limit).map((f) => f.suggestion);
}
