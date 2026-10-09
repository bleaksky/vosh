// The draft model behind Settings, Automation. Every kind (triggers,
// aliases, macros, timers, presets, loadouts, and the tick) edits a
// draft: a list of items as they stand on the page, next to the same
// items as Vosh last loaded or saved them. Save writes what the draft
// added, changed, and removed through the kind's existing API, over the
// store as it stands at that moment, so an item #trigger, #alias, or a
// script made in the meantime survives. Discard puts the saved items
// back. Each item carries a uid that lives only on the page, so the
// selection and the list keys survive a rename, a regroup, or an edit
// in the JSON view.
//
// Values are treated as immutable. An edit replaces the item's value,
// and every other item keeps its object, which lets the list skip
// rows that did not change and lets the change count reuse a cached
// serialization. That keeps the page quick with 500 triggers.

import { listJoin } from '../lib/text';

/** Why a list cannot save yet: the sentence the save bar shows, and
 *  where in the list the items it names sit, so the list can mark
 *  their rows and select the first. */
export interface SaveProblem {
  message: string;
  at: number[];
}

/** A problem with `message` about the items at `at`. */
export function saveProblem(message: string, at: number[] = []): SaveProblem {
  return { message, at };
}

export interface DraftItem<T> {
  readonly uid: string;
  readonly value: T;
}

export interface Draft<T> {
  /** The items as they stand on the page. */
  readonly items: readonly DraftItem<T>[];
  /** The items as Vosh last loaded or saved them. */
  readonly saved: readonly DraftItem<T>[];
}

let uidSeq = 0;

/** A uid for a new draft item, unique for the life of the page. */
export function nextDraftUid(): string {
  uidSeq += 1;
  return `d${uidSeq}`;
}

/** A clean draft over `values`. */
export function createDraft<T>(values: readonly T[]): Draft<T> {
  const items = values.map((value) => ({ uid: nextDraftUid(), value }));
  return { items, saved: items };
}

export function draftValues<T>(draft: Draft<T>): T[] {
  return draft.items.map((item) => item.value);
}

export function findDraftItem<T>(draft: Draft<T>, uid: string): DraftItem<T> | undefined {
  return draft.items.find((item) => item.uid === uid);
}

/** Replace one item's value. Returns the same draft when nothing
 *  changed, so a no-op edit does not re-render the page. */
export function updateDraftItem<T>(
  draft: Draft<T>,
  uid: string,
  update: (value: T) => T,
): Draft<T> {
  let changed = false;
  const items = draft.items.map((item) => {
    if (item.uid !== uid) return item;
    const value = update(item.value);
    if (value === item.value) return item;
    changed = true;
    return { uid, value };
  });
  return changed ? { ...draft, items } : draft;
}

/** Append a new item. */
export function addDraftItem<T>(draft: Draft<T>, value: T, uid: string = nextDraftUid()): Draft<T> {
  return { ...draft, items: [...draft.items, { uid, value }] };
}

export function removeDraftItem<T>(draft: Draft<T>, uid: string): Draft<T> {
  const items = draft.items.filter((item) => item.uid !== uid);
  return items.length === draft.items.length ? draft : { ...draft, items };
}

/** Put the saved items back. */
export function discardDraft<T>(draft: Draft<T>): Draft<T> {
  return draft.items === draft.saved ? draft : { ...draft, items: draft.saved };
}

/** Replace every value at once, the way the JSON view does. An item
 *  keeps the uid of the current item with the same natural key when
 *  one is free, and otherwise the uid of the item at its position, so
 *  an edit in place reads as a change and not as a removal and an
 *  addition. */
export function replaceDraftValues<T>(
  draft: Draft<T>,
  values: readonly T[],
  keyOf?: (value: T) => string,
): Draft<T> {
  const used = new Set<string>();
  const byKey = new Map<string, string>();
  if (keyOf) {
    for (const item of draft.items) {
      const key = keyOf(item.value);
      if (!byKey.has(key)) byKey.set(key, item.uid);
    }
  }
  const claimed: (string | null)[] = values.map((value) => {
    if (!keyOf) return null;
    const uid = byKey.get(keyOf(value));
    if (uid === undefined || used.has(uid)) return null;
    used.add(uid);
    return uid;
  });
  const items = values.map((value, index) => {
    let uid = claimed[index];
    if (uid === null) {
      const positional = draft.items[index]?.uid;
      uid = positional !== undefined && !used.has(positional) ? positional : nextDraftUid();
      used.add(uid);
    }
    return { uid, value };
  });
  return { ...draft, items };
}

const serialCache = new WeakMap<object, string>();

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(value).sort()) {
      const field = (value as Record<string, unknown>)[key];
      if (field !== undefined) out[key] = canonical(field);
    }
    return out;
  }
  return value;
}

/** A stable string for a value: keys sorted, undefined fields left
 *  out. Two values that save the same way serialize the same way. */
export function serializeValue(value: unknown): string {
  if (value && typeof value === 'object') {
    const hit = serialCache.get(value);
    if (hit !== undefined) return hit;
    const text = JSON.stringify(canonical(value));
    serialCache.set(value, text);
    return text;
  }
  return JSON.stringify(value) ?? 'undefined';
}

export interface DraftChanges<T> {
  added: DraftItem<T>[];
  removed: DraftItem<T>[];
  changed: { uid: string; before: T; after: T }[];
}

/** What Save has to write: the items added, removed, and changed
 *  since the last load or save. Order alone is not a change, since the
 *  stores keep their own order. */
export function draftChanges<T>(draft: Draft<T>): DraftChanges<T> {
  const out: DraftChanges<T> = { added: [], removed: [], changed: [] };
  if (draft.items === draft.saved) return out;
  const saved = new Map(draft.saved.map((item) => [item.uid, item.value]));
  const seen = new Set<string>();
  for (const item of draft.items) {
    if (!saved.has(item.uid)) {
      out.added.push(item);
      continue;
    }
    seen.add(item.uid);
    const before = saved.get(item.uid) as T;
    if (before !== item.value && serializeValue(before) !== serializeValue(item.value)) {
      out.changed.push({ uid: item.uid, before, after: item.value });
    }
  }
  for (const item of draft.saved) {
    if (!seen.has(item.uid)) out.removed.push(item);
  }
  return out;
}

/** The list a store should hold after Save: `current`, the store as it
 *  stands now, with the draft's own additions, changes, and removals
 *  applied by natural key. Everything the draft did not touch keeps the
 *  store's version, so an item another window or a script added, edited,
 *  or removed since the page loaded stays that way. Where both sides
 *  touched the same key, the draft wins. A changed item keeps its place,
 *  and one the store no longer holds comes back at the end. */
export function mergeDraftChanges<T>(
  current: readonly T[],
  draft: Draft<T>,
  keyOf: (value: T) => string,
): T[] {
  const { added, removed, changed } = draftChanges(draft);
  const removedKeys = new Set(removed.map((item) => keyOf(item.value)));
  const changedByKey = new Map<string, T>();
  for (const { before, after } of changed) changedByKey.set(keyOf(before), after);
  // Keys the draft writes. The store's own item under one of them gives
  // way to the draft's.
  const written = new Set([
    ...changed.map((c) => keyOf(c.after)),
    ...added.map((item) => keyOf(item.value)),
  ]);
  const placed = new Set<string>();
  const out: T[] = [];
  for (const value of current) {
    const key = keyOf(value);
    const after = changedByKey.get(key);
    if (after !== undefined && !placed.has(key)) {
      placed.add(key);
      out.push(after);
    } else if (!removedKeys.has(key) && !written.has(key)) {
      out.push(value);
    }
  }
  for (const [key, after] of changedByKey) if (!placed.has(key)) out.push(after);
  for (const item of added) out.push(item.value);
  return out;
}

/** Save a draft over the store as it stands now: read it, apply the
 *  draft's changes with mergeDraftChanges, and write the result. For
 *  kinds whose API replaces the whole store in one call. */
export async function saveDraftOnto<T>(
  draft: Draft<T>,
  store: { read: () => Promise<T[]>; write: (values: T[]) => Promise<void> },
  keyOf: (value: T) => string,
): Promise<void> {
  const current = await store.read();
  await store.write(mergeDraftChanges(current, draft, keyOf));
}

/** Save a kind's list and then the block pinned above it, like the Tick
 *  above Timers. The list loads again as soon as it is written, so when
 *  the pinned block fails after it, the page already matches the store
 *  and the next Save does not write the list a second time. For Timers
 *  that second write would create every new timer again. Pass null for
 *  a part with nothing to save. The list loads at the end either way. */
export async function saveListThenPinned(steps: {
  list: (() => Promise<void>) | null;
  pinned: (() => Promise<void>) | null;
  reload: () => Promise<void>;
}): Promise<void> {
  if (steps.list) {
    await steps.list();
    await steps.reload();
  }
  if (steps.pinned) await steps.pinned();
  if (!steps.list) await steps.reload();
}

/** One write the store took during a Save that sends one call per item,
 *  like Macros and Timers. `stored` is what the store now holds for the
 *  draft item `uid`, or null once the store no longer holds it. */
export interface SavedWrite<T> {
  uid: string;
  stored: T | null;
  /** The item's value when Save planned the write. Give it when the
   *  store keeps something other than what the page sent, like a new
   *  timer that gets its id from the store. An item that still holds
   *  this value takes the stored one, so it reads as saved. */
  sent?: T;
}

/** Move one write the store took into the saved items. A Save that
 *  fails partway keeps its unsaved changes, and without this the next
 *  Save would send every write again, creating each new timer twice.
 *  A write for an item the draft no longer knows changes nothing. */
export function markWritten<T>(draft: Draft<T>, write: SavedWrite<T>): Draft<T> {
  const { uid, stored, sent } = write;
  if (stored === null) {
    const saved = draft.saved.filter((item) => item.uid !== uid);
    return saved.length === draft.saved.length ? draft : { ...draft, saved };
  }
  const current = draft.items.find((item) => item.uid === uid);
  let items = draft.items;
  if (current && sent !== undefined && serializeValue(current.value) === serializeValue(sent)) {
    items = draft.items.map((item) => (item.uid === uid ? { uid, value: stored } : item));
  }
  let saved = draft.saved;
  if (draft.saved.some((item) => item.uid === uid)) {
    saved = draft.saved.map((item) => (item.uid === uid ? { uid, value: stored } : item));
  } else if (current) {
    saved = [...draft.saved, { uid, value: stored }];
  }
  return items === draft.items && saved === draft.saved ? draft : { items, saved };
}

/** markWritten for each write, in the order the store took them. */
export function markAllWritten<T>(draft: Draft<T>, writes: readonly SavedWrite<T>[]): Draft<T> {
  return writes.reduce((next, write) => markWritten(next, write), draft);
}

/** How many items Save would add, remove, or change. */
export function draftChangeCount<T>(draft: Draft<T>): number {
  const { added, removed, changed } = draftChanges(draft);
  return added.length + removed.length + changed.length;
}

export function isDraftDirty<T>(draft: Draft<T>): boolean {
  return draftChangeCount(draft) > 0;
}

/** What the page does when the store changes outside it: follow the
 *  store while the draft is clean, keep your unsaved edits and say the
 *  list changed while it is dirty, and wait out its own save, which
 *  loads the list again when it ends. */
export function storeChangeAction(state: {
  dirty: boolean;
  saving: boolean;
}): 'reload' | 'warn' | 'ignore' {
  if (state.saving) return 'ignore';
  return state.dirty ? 'warn' : 'reload';
}

/** The singular and plural name of what a kind holds. */
export interface KindNoun {
  one: string;
  many: string;
}

/** `1 trigger`, `3 triggers`. */
export function countPhrase(count: number, noun: KindNoun): string {
  return `${count} ${count === 1 ? noun.one : noun.many}`;
}

/** The title the discard dialog asks with, like `Discard changes to 3
 *  triggers?` or `Discard changes to 2 timers and the tick?`. Pass the
 *  phrases for each part that changed. */
export function discardTitle(phrases: readonly string[]): string {
  const parts = phrases.filter((p) => p.length > 0);
  if (parts.length === 0) return 'Discard your changes?';
  return `Discard changes to ${listJoin(parts)}?`;
}

/** The note the page shows when the store changed while you had unsaved
 *  edits. Save applies your edits over the new list, so both survive. */
export function listChangedNote(noun: KindNoun): string {
  return `Your ${noun.many} changed outside Settings while you edited them. Save keeps those changes and adds yours.`;
}
