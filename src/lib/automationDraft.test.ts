import { describe, expect, it } from 'vitest';
import {
  addDraftItem,
  countPhrase,
  createDraft,
  discardDraft,
  discardTitle,
  draftChangeCount,
  draftChanges,
  draftValues,
  isDraftDirty,
  removeDraftItem,
  replaceDraftValues,
  serializeValue,
  updateDraftItem,
} from './automationDraft';

interface Item {
  name: string;
  group?: string | undefined;
  enabled: boolean;
}

const items: Item[] = [
  { name: 'Sleep when mana is low', group: 'idle', enabled: true },
  { name: 'Flee below 20 percent', group: 'combat', enabled: false },
  { name: 'Loot every kill', group: 'combat', enabled: true },
];

describe('draft model', () => {
  it('starts clean with every item in load order', () => {
    const draft = createDraft(items);
    expect(draftValues(draft)).toEqual(items);
    expect(isDraftDirty(draft)).toBe(false);
    expect(new Set(draft.items.map((i) => i.uid)).size).toBe(3);
  });

  it('counts an edit, an addition, and a removal', () => {
    let draft = createDraft(items);
    const [first, second] = draft.items;
    draft = updateDraftItem(draft, first.uid, (v) => ({ ...v, group: 'rest' }));
    draft = addDraftItem(draft, { name: 'New', enabled: true });
    draft = removeDraftItem(draft, second.uid);
    expect(draftChangeCount(draft)).toBe(3);
    const changes = draftChanges(draft);
    expect(changes.changed.map((c) => c.after.group)).toEqual(['rest']);
    expect(changes.added.map((a) => a.value.name)).toEqual(['New']);
    expect(changes.removed.map((r) => r.value.name)).toEqual(['Flee below 20 percent']);
  });

  it('reads an edit that is undone as clean', () => {
    let draft = createDraft(items);
    const uid = draft.items[0].uid;
    draft = updateDraftItem(draft, uid, (v) => ({ ...v, enabled: false }));
    expect(isDraftDirty(draft)).toBe(true);
    draft = updateDraftItem(draft, uid, (v) => ({ ...v, enabled: true }));
    expect(isDraftDirty(draft)).toBe(false);
  });

  it('keeps the same draft for a no-op edit', () => {
    const draft = createDraft(items);
    expect(updateDraftItem(draft, draft.items[0].uid, (v) => v)).toBe(draft);
  });

  it('discard restores exactly what was saved and keeps the uids', () => {
    const clean = createDraft(items);
    let draft = updateDraftItem(clean, clean.items[1].uid, (v) => ({ ...v, name: 'Run' }));
    draft = addDraftItem(draft, { name: 'Extra', enabled: true });
    const restored = discardDraft(draft);
    expect(draftValues(restored)).toEqual(items);
    expect(restored.items.map((i) => i.uid)).toEqual(clean.items.map((i) => i.uid));
    expect(isDraftDirty(restored)).toBe(false);
  });

  it('ignores field order and undefined fields when comparing', () => {
    expect(serializeValue({ a: 1, b: undefined, c: [1, { y: 2, x: 1 }] })).toBe(
      serializeValue({ c: [1, { x: 1, y: 2 }], a: 1 }),
    );
    let draft = createDraft(items);
    draft = updateDraftItem(draft, draft.items[0].uid, (v) => ({
      enabled: v.enabled,
      group: v.group,
      name: v.name,
    }));
    expect(isDraftDirty(draft)).toBe(false);
  });

  it('does not count a new order as a change', () => {
    const draft = createDraft(items);
    const reordered = { ...draft, items: [...draft.items].reverse() };
    expect(isDraftDirty(reordered)).toBe(false);
  });

  it('maps JSON edits onto the items they came from', () => {
    const draft = createDraft(items);
    const edited = [
      { ...items[0], enabled: false },
      items[2],
      { name: 'Brand new', enabled: true },
    ];
    const next = replaceDraftValues(draft, edited, (v) => v.name);
    expect(next.items[0].uid).toBe(draft.items[0].uid);
    expect(next.items[1].uid).toBe(draft.items[2].uid);
    const changes = draftChanges(next);
    expect(changes.changed).toHaveLength(1);
    expect(changes.added.map((a) => a.value.name)).toEqual(['Brand new']);
    expect(changes.removed.map((r) => r.value.name)).toEqual(['Flee below 20 percent']);
  });

  it('keeps positions for a JSON rename', () => {
    const draft = createDraft(items);
    const renamed = [{ ...items[0], name: 'Sleep at low mana' }, items[1], items[2]];
    const next = replaceDraftValues(draft, renamed, (v) => v.name);
    expect(next.items.map((i) => i.uid)).toEqual(draft.items.map((i) => i.uid));
    expect(draftChangeCount(next)).toBe(1);
  });

  it('round trips a save: a draft built from the saved values is clean', () => {
    let draft = createDraft(items);
    draft = updateDraftItem(draft, draft.items[2].uid, (v) => ({ ...v, name: 'Loot it all' }));
    draft = addDraftItem(draft, { name: 'Wake', enabled: true });
    const written = JSON.parse(JSON.stringify(draftValues(draft))) as Item[];
    const reloaded = createDraft(written);
    expect(isDraftDirty(reloaded)).toBe(false);
    expect(draftValues(reloaded)).toEqual(draftValues(draft));
  });

  it('stays quick with 500 items', () => {
    const many = Array.from({ length: 500 }, (_, i) => ({
      name: `Trigger ${i}`,
      group: `g${i % 12}`,
      enabled: i % 3 !== 0,
    }));
    let draft = createDraft(many);
    draftChangeCount(draft);
    const started = performance.now();
    for (let i = 0; i < 50; i += 1) {
      draft = updateDraftItem(draft, draft.items[i].uid, (v) => ({ ...v, name: `${v.name}!` }));
      draftChangeCount(draft);
    }
    expect(draftChangeCount(draft)).toBe(50);
    expect(performance.now() - started).toBeLessThan(500);
  });
});

describe('discard copy', () => {
  it('names the count and the kind', () => {
    expect(countPhrase(1, { one: 'trigger', many: 'triggers' })).toBe('1 trigger');
    expect(discardTitle([countPhrase(3, { one: 'trigger', many: 'triggers' })])).toBe(
      'Discard changes to 3 triggers?',
    );
    expect(discardTitle([countPhrase(2, { one: 'timer', many: 'timers' }), 'the tick'])).toBe(
      'Discard changes to 2 timers and the tick?',
    );
    expect(discardTitle(['the tick'])).toBe('Discard changes to the tick?');
  });
});
