import { describe, expect, it } from 'vitest';
import { addDraftItem, createDraft, removeDraftItem, updateDraftItem } from './automationDraft';
import type { TriggerAction, TriggerRecord } from './session';
import {
  blankTrigger,
  loadTriggers,
  saveTriggerDraft,
  effectOf,
  extraEffects,
  highlightOf,
  normalizeTrigger,
  replaceTemplateOf,
  TRIGGER_STYLE_OPTIONS,
  triggerForSave,
  triggerStyle,
  validateTriggers,
  withEffect,
  withEffectAt,
  withGroup,
  withHighlight,
  withMainPattern,
  withReplaceTemplate,
  withTriggerStyle,
  type TriggerStyle,
} from './automationTriggers';

const STYLES = TRIGGER_STYLE_OPTIONS.map((o) => o.value);

const starts: [string, TriggerAction[]][] = [
  ['nothing', []],
  ['a send only', [{ kind: 'send', template: 'sleep' }]],
  ['a highlight', [{ kind: 'highlight', style: { fg: 'red', bold: true } }]],
  ['a wash', [{ kind: 'highlight', style: { fg: 'cyan', wash: true } }]],
  ['a replace', [{ kind: 'replace', template: '$1 fled' }]],
  [
    'a gag between effects',
    [{ kind: 'send', template: 'look' }, { kind: 'gag' }, { kind: 'route', pane: 'chat' }],
  ],
];

describe('Style select mapping', () => {
  it('lists the board options in order', () => {
    expect(TRIGGER_STYLE_OPTIONS.map((o) => o.label)).toEqual([
      'None',
      'Highlight',
      'Wash',
      'Replace',
      'Hide',
    ]);
  });

  it('reads each visual as its Style', () => {
    expect(triggerStyle([])).toBe('none');
    expect(triggerStyle([{ kind: 'highlight', style: { fg: 'red' } }])).toBe('highlight');
    expect(triggerStyle([{ kind: 'highlight', style: { fg: 'red', wash: true } }])).toBe('wash');
    expect(triggerStyle([{ kind: 'replace', template: 'x' }])).toBe('replace');
    expect(triggerStyle([{ kind: 'gag' }])).toBe('hide');
  });

  it('writes each Style as the visual it names', () => {
    expect(withTriggerStyle([], 'none')).toEqual([]);
    expect(withTriggerStyle([], 'highlight')).toEqual([
      { kind: 'highlight', style: { fg: 'yellow' } },
    ]);
    expect(withTriggerStyle([], 'wash')).toEqual([
      { kind: 'highlight', style: { fg: 'yellow', wash: true } },
    ]);
    expect(withTriggerStyle([], 'replace')).toEqual([{ kind: 'replace', template: '' }]);
    expect(withTriggerStyle([], 'hide')).toEqual([{ kind: 'gag' }]);
  });

  it('round trips every Style from every start', () => {
    for (const [, actions] of starts) {
      for (const style of STYLES) {
        const next = withTriggerStyle(actions, style);
        expect(triggerStyle(next)).toBe(style);
        // Setting the same Style again changes nothing.
        expect(withTriggerStyle(next, style)).toEqual(next);
        // The effects stay, in their order.
        const effects = (list: TriggerAction[]) =>
          list.filter((a) => a.kind === 'send' || a.kind === 'route' || a.kind === 'script');
        expect(effects(next)).toEqual(effects(actions));
      }
    }
  });

  it('keeps the colors between Highlight and Wash', () => {
    const start: TriggerAction[] = [{ kind: 'highlight', style: { fg: 'red', bold: true } }];
    const washed = withTriggerStyle(start, 'wash');
    expect(highlightOf(washed)).toEqual({ fg: 'red', bold: true, wash: true });
    expect(highlightOf(withTriggerStyle(washed, 'highlight'))).toEqual({ fg: 'red', bold: true });
  });

  it('survives a save and reload', () => {
    for (const style of STYLES as TriggerStyle[]) {
      const t = { ...blankTrigger(), name: 't', actions: withTriggerStyle([], style) };
      const reloaded = normalizeTrigger(JSON.parse(JSON.stringify(triggerForSave(t))));
      expect(triggerStyle(reloaded.actions)).toBe(style);
    }
  });
});

describe('trigger fields', () => {
  it('reads and writes the first send as Then send', () => {
    let actions = withEffect([{ kind: 'gag' }], 'send', 'sleep');
    expect(effectOf(actions, 'send')).toBe('sleep');
    actions = withEffect(actions, 'send', 'wake');
    expect(actions).toEqual([{ kind: 'gag' }, { kind: 'send', template: 'wake' }]);
    expect(withEffect(actions, 'send', '')).toEqual([{ kind: 'gag' }]);
  });

  it('keeps extra effects in reach and in place', () => {
    const actions: TriggerAction[] = [
      { kind: 'send', template: 'one' },
      { kind: 'route', pane: 'chat' },
      { kind: 'send', template: 'two' },
    ];
    expect(extraEffects(actions)).toEqual([{ index: 2, kind: 'send', value: 'two' }]);
    expect(withEffectAt(actions, 2, 'three')[2]).toEqual({ kind: 'send', template: 'three' });
    expect(withEffectAt(actions, 2, null)).toHaveLength(2);
    expect(effectOf(actions, 'route')).toBe('chat');
  });

  it('edits highlight flags only while the Style is a highlight', () => {
    const actions = withTriggerStyle([], 'highlight');
    expect(highlightOf(withHighlight(actions, { bg: 'blue', underline: true }))).toEqual({
      fg: 'yellow',
      bg: 'blue',
      underline: true,
    });
    expect(highlightOf(withHighlight(actions, { fg: undefined }))).toEqual({});
    expect(withHighlight([{ kind: 'gag' }], { bold: true })).toEqual([{ kind: 'gag' }]);
  });

  it('edits the Replace template in place', () => {
    const actions = withTriggerStyle([{ kind: 'send', template: 'x' }], 'replace');
    const next = withReplaceTemplate(actions, '{red}$1');
    expect(replaceTemplateOf(next)).toBe('{red}$1');
    expect(next[1]).toEqual({ kind: 'send', template: 'x' });
  });

  it('edits the main pattern and keeps the rest', () => {
    const t: TriggerRecord = {
      ...blankTrigger(),
      patterns: [
        { pattern: 'a', enabled: true },
        { pattern: 'b', enabled: false },
      ],
    };
    const next = withMainPattern(t, { pattern: 'c' });
    expect(next.patterns).toEqual([
      { pattern: 'c', enabled: true },
      { pattern: 'b', enabled: false },
    ]);
  });

  it('drops a blank group', () => {
    const t = { ...blankTrigger(), group: 'idle' };
    expect(withGroup(t, '  combat ').group).toBe('combat');
    expect('group' in withGroup(t, '  ')).toBe(false);
  });
});

describe('normalizeTrigger', () => {
  it('reads the legacy single pattern and action shape', () => {
    const t = normalizeTrigger({
      name: 'old',
      pattern: '^You are hungry',
      priority: 3,
      action: { kind: 'send', template: 'eat bread' },
      group: '  ',
    });
    expect(t).toEqual({
      name: 'old',
      patterns: [{ pattern: '^You are hungry', enabled: true }],
      priority: 3,
      enabled: true,
      actions: [{ kind: 'send', template: 'eat bread' }],
    });
  });

  it('keeps the preset, the group, and the prompt lane', () => {
    const t = normalizeTrigger({
      name: 'p',
      patterns: [{ pattern: 'x', enabled: true }],
      actions: [],
      preset: 'healing_basics',
      group: 'idle',
      target: 'prompt',
    });
    expect(t.preset).toBe('healing_basics');
    expect(t.group).toBe('idle');
    expect(t.target).toBe('prompt');
  });
});

describe('validateTriggers', () => {
  const t = (name: string, pattern = 'x') => ({
    ...blankTrigger(),
    name,
    patterns: [{ pattern, enabled: true }],
  });

  it('passes a clean list', () => {
    expect(validateTriggers([t('a'), t('b')])).toBeNull();
  });

  it('asks for names, unique names, and a pattern', () => {
    expect(validateTriggers([t('')])).toBe('Give every trigger a name before you save.');
    expect(validateTriggers([t('a'), t('a')])).toContain('Two triggers are named');
    expect(validateTriggers([t('a', ' ')])).toContain('needs a pattern');
  });
});

describe('saving triggers', () => {
  const trigger = (name: string, send: string): TriggerRecord => ({
    ...blankTrigger(),
    name,
    patterns: [{ pattern: `^${name}$`, enabled: true }],
    actions: [{ kind: 'send', template: send }],
  });

  /** triggers_export and triggers_import over one list. */
  function fakeStore(initial: TriggerRecord[]) {
    let json = JSON.stringify(initial);
    const names = () => (JSON.parse(json) as TriggerRecord[]).map((t) => t.name);
    return {
      api: {
        exportTriggers: () => Promise.resolve(json),
        importTriggers: (next: string) => {
          json = next;
          return Promise.resolve();
        },
      },
      /** #trigger in the main window. */
      add: (t: TriggerRecord) => {
        json = JSON.stringify([...(JSON.parse(json) as TriggerRecord[]), t]);
      },
      names,
      list: () => JSON.parse(json) as TriggerRecord[],
    };
  }

  it('keeps a trigger #trigger added after the page loaded', async () => {
    const store = fakeStore([trigger('rest', 'sleep'), trigger('flee', 'flee')]);
    let draft = createDraft(await loadTriggers(store.api));
    store.add(trigger('bash', 'bash door'));
    draft = updateDraftItem(draft, draft.items[0].uid, (t) => ({ ...t, enabled: false }));
    draft = removeDraftItem(draft, draft.items[1].uid);
    draft = addDraftItem(draft, trigger('wake', 'wake'));
    await saveTriggerDraft(draft, store.api);
    expect(store.names()).toEqual(['rest', 'bash', 'wake']);
    expect(store.list()[0].enabled).toBe(false);
  });

  it('saves nothing when the store answer does not read', async () => {
    const store = fakeStore([trigger('rest', 'sleep')]);
    const draft = addDraftItem(createDraft(await loadTriggers(store.api)), trigger('wake', 'x'));
    const broken = { ...store.api, exportTriggers: () => Promise.resolve('not json') };
    await expect(saveTriggerDraft(draft, broken)).rejects.toThrow('saved nothing');
    expect(store.names()).toEqual(['rest']);
  });
});
