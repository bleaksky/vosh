import { describe, expect, it } from 'vitest';
import type { TriggerRecord } from '../ipc/automation';
import { addDraftItem, insertDraftItemAfter, createDraft } from './automationDraft';
import { blankTrigger } from './automationTriggers';
import { duplicateAlias, duplicateMacro, duplicateTrigger } from './duplicate';

const trigger = (name: string, more: Partial<TriggerRecord> = {}): TriggerRecord => ({
  ...blankTrigger(),
  name,
  ...more,
});

describe('duplicate', () => {
  it('names a trigger copy so it saves beside the original as it is', () => {
    const t = trigger('kick ready', { group: 'combat' });
    expect(duplicateTrigger(t, [t]).name).toBe('kick ready copy');
    expect(duplicateTrigger(t, [t, trigger('kick ready copy')]).name).toBe('kick ready copy 2');
    expect(duplicateTrigger(t, [t]).group).toBe('combat');
  });

  it('makes a preset trigger copy yours, with patterns of its own', () => {
    const t = trigger('lit', { preset: 'highlights' });
    const copy = duplicateTrigger(t, [t]);
    expect(copy.preset).toBeUndefined();
    copy.patterns[0].pattern = 'changed';
    expect(t.patterns[0].pattern).not.toBe('changed');
  });

  it('keeps an alias copy to one word', () => {
    const a = { name: 'kk', expansion: 'kick', enabled: true };
    expect(duplicateAlias(a, [a]).name).toBe('kk-copy');
    expect(duplicateAlias(a, [a, { ...a, name: 'kk-copy' }]).name).toBe('kk-copy2');
  });

  it('leaves a macro copy with no key, since one key runs one macro', () => {
    const m = { key: 'F1', command: 'kick', enabled: true, preset: 'p' };
    expect(duplicateMacro(m)).toEqual({ key: '', command: 'kick', enabled: true });
  });

  it('puts the copy right after its original', () => {
    let d = createDraft(['a', 'b']);
    const first = d.items[0].uid;
    d = insertDraftItemAfter(d, first, 'a2', 'new');
    expect(d.items.map((i) => i.value)).toEqual(['a', 'a2', 'b']);
    expect(insertDraftItemAfter(addDraftItem(d, 'z'), 'gone', 'end').items.at(-1)?.value).toBe(
      'end',
    );
  });
});
