import { describe, expect, it, vi } from 'vitest';
import {
  addDraftItem,
  createDraft,
  draftValues,
  isDraftDirty,
  removeDraftItem,
  replaceDraftValues,
  updateDraftItem,
} from './automationDraft';
import { jsonListText, parseJsonList } from './automationRecords';
import type { TriggerAction, TriggerRecord } from '../ipc/automation';
import {
  blankPattern,
  blankTrigger,
  loadTriggers,
  moveTriggerToPrompts,
  setTriggerGroups,
  saveTriggerDraft,
  effectOf,
  extraEffects,
  highlightOf,
  normalizeTrigger,
  patternSource,
  replaceTemplateOf,
  TRIGGER_STYLE_OPTIONS,
  triggerForSave,
  triggerKey,
  triggerMode,
  triggerStyle,
  validateTriggers,
  withEffect,
  withEffectAt,
  withGroup,
  withHighlight,
  withMainPattern,
  withMainPatternSource,
  withPatternSource,
  withReplaceTemplate,
  withTriggerMode,
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

  it('keeps the room lanes and drops a lane it does not know', () => {
    const t = (target: unknown) =>
      normalizeTrigger({ name: 'r', patterns: [{ pattern: '^.+$', enabled: true }], target })
        .target;
    expect(t('room')).toBe('room');
    expect(t('room_target')).toBe('room_target');
    expect(t('line')).toBeUndefined();
    expect(t('screen')).toBeUndefined();
  });
});

describe('match modes', () => {
  // The rows Rust writes, the main pattern and More patterns, as the
  // store sends them. A Text or Starts with row holds its regex in
  // pattern and what you typed in text. A Regex row has no mode on the
  // wire. The last row is a Text row as builds before text saved it.
  const rows = [
    {
      pattern: '^\\s*You are thirsty\\.\\s*$',
      enabled: true,
      mode: 'text',
      text: 'You are thirsty.',
    },
    {
      pattern: '^\\s*You are hungry.*',
      enabled: false,
      mode: 'starts_with',
      text: 'You are hungry',
    },
    { pattern: '^You are hungry\\.$', enabled: true },
    { pattern: 'x', enabled: true, mode: 'regex', text: 'z' },
    { pattern: 'y', enabled: true, mode: 'glob', text: 'z' },
    { pattern: '*** Too Dark ***', enabled: true, mode: 'text' },
  ];

  /** triggers_export and triggers_import over one list. */
  function fakeStore() {
    const store = {
      json: JSON.stringify([{ name: 'needs', patterns: rows, actions: [] }]),
      exportTriggers: () => Promise.resolve(store.json),
      importTriggers: (next: string) => {
        store.json = next;
        return Promise.resolve();
      },
      saved: () => (JSON.parse(store.json) as TriggerRecord[])[0],
    };
    return store;
  }

  it('keeps Text and Starts with on every row with its text and leaves Regex out', () => {
    const t = normalizeTrigger({ name: 'needs', patterns: rows, actions: [] });
    expect(t.patterns).toEqual([
      {
        pattern: '^\\s*You are thirsty\\.\\s*$',
        enabled: true,
        mode: 'text',
        text: 'You are thirsty.',
      },
      {
        pattern: '^\\s*You are hungry.*',
        enabled: false,
        mode: 'starts_with',
        text: 'You are hungry',
      },
      { pattern: '^You are hungry\\.$', enabled: true },
      { pattern: 'x', enabled: true },
      { pattern: 'y', enabled: true },
      // The row with no text takes its pattern as the text.
      { pattern: '*** Too Dark ***', enabled: true, mode: 'text', text: '*** Too Dark ***' },
    ]);
    // The Pattern fields show what you typed.
    expect(t.patterns.map(patternSource)).toEqual([
      'You are thirsty.',
      'You are hungry',
      '^You are hungry\\.$',
      'x',
      'y',
      '*** Too Dark ***',
    ]);
  });

  it('survives an edit and a save', async () => {
    const api = fakeStore();
    let draft = createDraft(await loadTriggers(api));
    draft = updateDraftItem(draft, draft.items[0].uid, (t) =>
      withMainPattern(withGroup(t, 'needs'), { enabled: false }),
    );
    await saveTriggerDraft(draft, api);
    const saved = api.saved();
    expect(saved.patterns.map((p) => p.mode ?? 'regex')).toEqual([
      'text',
      'starts_with',
      'regex',
      'regex',
      'regex',
      'text',
    ]);
    expect(saved.patterns[0]).toEqual({
      pattern: '^\\s*You are thirsty\\.\\s*$',
      enabled: false,
      mode: 'text',
      text: 'You are thirsty.',
    });
    expect(saved.patterns[1].text).toBe('You are hungry');
    expect(saved.patterns[5]).toEqual({
      pattern: '*** Too Dark ***',
      enabled: true,
      mode: 'text',
      text: '*** Too Dark ***',
    });
  });

  it('edits what you typed in a Text or Starts with row', async () => {
    const api = fakeStore();
    let draft = createDraft(await loadTriggers(api));
    draft = updateDraftItem(draft, draft.items[0].uid, (t) => {
      const next = withMainPatternSource(t, 'You are hungry.');
      return {
        ...next,
        patterns: next.patterns.map((p, i) =>
          i === 1 || i === 2 || i === 5 ? withPatternSource(p, `${patternSource(p)} `) : p,
        ),
      };
    });
    await saveTriggerDraft(draft, api);
    // An edit changes only the text of a Text or Starts with row, so Save
    // sends the old regex beside the new text. The store reads text, and
    // the regex it writes is the one the new text compiles to.
    expect(api.saved().patterns).toEqual([
      {
        pattern: '^\\s*You are thirsty\\.\\s*$',
        enabled: true,
        mode: 'text',
        text: 'You are hungry.',
      },
      {
        pattern: '^\\s*You are hungry.*',
        enabled: false,
        mode: 'starts_with',
        text: 'You are hungry ',
      },
      { pattern: '^You are hungry\\.$ ', enabled: true },
      { pattern: 'x', enabled: true },
      { pattern: 'y', enabled: true },
      { pattern: '*** Too Dark ***', enabled: true, mode: 'text', text: '*** Too Dark *** ' },
    ]);
    expect(api.saved().patterns.map(patternSource)).toEqual([
      'You are hungry.',
      'You are hungry ',
      '^You are hungry\\.$ ',
      'x',
      'y',
      '*** Too Dark *** ',
    ]);
  });

  it('reads as saved again once you type a row back as it was', async () => {
    const loaded = createDraft(await loadTriggers(fakeStore()));
    const uid = loaded.items[0].uid;
    const typed = draftValues(loaded)[0].patterns.map(patternSource);
    // The main pattern, a Text row as the store sends it.
    let draft = updateDraftItem(loaded, uid, (t) => withMainPatternSource(t, `${typed[0]}x`));
    expect(isDraftDirty(draft)).toBe(true);
    draft = updateDraftItem(draft, uid, (t) => withMainPatternSource(t, typed[0]));
    expect(isDraftDirty(draft)).toBe(false);
    // Each row in More patterns, the Text row with no text among them.
    for (let i = 1; i < typed.length; i++) {
      const typeIn = (value: string) => (t: TriggerRecord) => ({
        ...t,
        patterns: t.patterns.map((p, j) => (j === i ? withPatternSource(p, value) : p)),
      });
      draft = updateDraftItem(loaded, uid, typeIn(`${typed[i]}x`));
      expect(isDraftDirty(draft), `row ${i + 1}`).toBe(true);
      draft = updateDraftItem(draft, uid, typeIn(typed[i]));
      expect(isDraftDirty(draft), `row ${i + 1}`).toBe(false);
    }
  });

  it('round trips through Edit all as JSON', async () => {
    const api = fakeStore();
    let draft = createDraft(await loadTriggers(api));
    const shown = jsonListText(draftValues(draft));
    expect(parseJsonList(shown, normalizeTrigger)).toEqual(draftValues(draft));
    // Change the text of the Starts with row and leave its regex alone,
    // as you might in the JSON.
    const edited = shown.replace('"text": "You are hungry"', '"text": "You are hungry."');
    expect(edited).not.toBe(shown);
    draft = replaceDraftValues(draft, parseJsonList(edited, normalizeTrigger) ?? [], triggerKey);
    await saveTriggerDraft(draft, api);
    const saved = api.saved();
    expect(saved.patterns[0].text).toBe('You are thirsty.');
    expect(saved.patterns[1]).toEqual({
      pattern: '^\\s*You are hungry.*',
      enabled: false,
      mode: 'starts_with',
      text: 'You are hungry.',
    });
    expect(patternSource(saved.patterns[1])).toBe('You are hungry.');
  });

  it('starts a new trigger as Text', () => {
    expect(blankTrigger().patterns).toEqual([
      { pattern: '', enabled: true, mode: 'text', text: '' },
    ]);
    expect(triggerMode(blankTrigger())).toBe('text');
  });

  it('reads a trigger in the main row mode, and Regex when it has none', async () => {
    const [needs] = await loadTriggers(fakeStore());
    expect(triggerMode(needs)).toBe('text');
    expect(triggerMode({ ...needs, patterns: needs.patterns.slice(2) })).toBe('regex');
    expect(triggerMode({ ...needs, patterns: [] })).toBe('regex');
  });

  it('sets every row to the mode you pick and keeps what each one holds', async () => {
    const [needs] = await loadTriggers(fakeStore());
    const typed = needs.patterns.map(patternSource);
    for (const mode of ['text', 'starts_with', 'regex'] as const) {
      const next = withTriggerMode(needs, mode);
      expect(next.patterns.map((p) => p.mode ?? 'regex')).toEqual(typed.map(() => mode));
      expect(next.patterns.map(patternSource)).toEqual(typed);
      expect(next.patterns.map((p) => p.enabled)).toEqual(needs.patterns.map((p) => p.enabled));
    }
    // A Regex row carries no mode or text, as the store writes one.
    expect(withTriggerMode(needs, 'regex').patterns[0]).toEqual({
      pattern: 'You are thirsty.',
      enabled: true,
    });
    // A Text row moved to Starts with keeps the regex the store sent,
    // and the store writes the new one from the text on Save.
    expect(withTriggerMode(needs, 'starts_with').patterns[0]).toEqual({
      pattern: '^\\s*You are thirsty\\.\\s*$',
      enabled: true,
      mode: 'starts_with',
      text: 'You are thirsty.',
    });
  });

  it('reads as saved after Regex to Text and back', () => {
    // The Regex rows, as the store sends them.
    const regex = normalizeTrigger({ name: 'needs', patterns: rows.slice(2, 5), actions: [] });
    const loaded = createDraft([regex]);
    const uid = loaded.items[0].uid;
    let draft = updateDraftItem(loaded, uid, (t) => withTriggerMode(t, 'text'));
    expect(isDraftDirty(draft)).toBe(true);
    draft = updateDraftItem(draft, uid, (t) => withTriggerMode(t, 'starts_with'));
    expect(isDraftDirty(draft)).toBe(true);
    draft = updateDraftItem(draft, uid, (t) => withTriggerMode(t, 'regex'));
    expect(isDraftDirty(draft)).toBe(false);
  });

  it('saves a new Text trigger with its mode and text', async () => {
    const api = fakeStore();
    let draft = createDraft(await loadTriggers(api));
    draft = addDraftItem(draft, {
      ...withMainPatternSource(blankTrigger(), 'A thick fog rolls in'),
      name: 'fog',
    });
    await saveTriggerDraft(draft, api);
    const fog = (JSON.parse(await api.exportTriggers()) as TriggerRecord[]).find(
      (t) => t.name === 'fog',
    );
    expect(fog?.patterns).toEqual([
      { pattern: '', enabled: true, mode: 'text', text: 'A thick fog rolls in' },
    ]);
  });

  it('adds a pattern in the mode the trigger reads', () => {
    expect(blankPattern('text')).toEqual({ pattern: '', enabled: true, mode: 'text', text: '' });
    expect(blankPattern('starts_with')).toEqual({
      pattern: '',
      enabled: true,
      mode: 'starts_with',
      text: '',
    });
    expect(blankPattern('regex')).toEqual({ pattern: '', enabled: true });
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
    expect(validateTriggers([t('')])?.message).toBe('Give every trigger a name before you save.');
    expect(validateTriggers([t('a'), t('b'), t(' a ')])).toEqual({
      message: 'You have two triggers named “a” with no group. Rename one or give it a group.',
      at: [0, 2],
    });
    expect(validateTriggers([t('a', ' ')])?.message).toContain('needs a pattern');
  });

  it('lets two groups each hold a trigger of one name, and not one group two', () => {
    const inGroup = (name: string, group: string) => ({ ...t(name), group });
    expect(
      validateTriggers([inGroup('greet', 'Tolliver'), inGroup('greet', 'Maren'), t('greet')]),
    ).toBeNull();
    expect(
      validateTriggers([
        inGroup('greet', 'Tolliver'),
        inGroup('greet', 'Maren'),
        inGroup('greet ', 'Maren'),
      ]),
    ).toEqual({
      message: 'Maren has two triggers named “greet”. Rename one or move it to another group.',
      at: [1, 2],
    });
    // A name may hold spaces, as it always could.
    expect(validateTriggers([t('Sleep when mana is low')])).toBeNull();
  });

  it('keys a trigger by its group and its trimmed name', () => {
    expect(triggerKey({ ...t(' greet '), group: 'Maren' })).toBe('Maren\u001fgreet');
    expect(triggerKey(t('greet'))).toBe('\u001fgreet');
    expect(triggerKey({ ...t('greet'), group: 'Maren' })).not.toBe(triggerKey(t('greet')));
  });

  it('keeps your trigger off the name of a preset trigger, its preset on or off', () => {
    const guard =
      'Disarms and fading buffs uses the name disarm.secondary. Give your trigger its own name.';
    expect(validateTriggers([t('disarm.secondary')])?.message).toBe(guard);
    // Ahead of the clash with the preset's own copy, so the message
    // says why.
    const installed = { ...t('disarm.secondary'), preset: 'disarm_buff_fade' };
    expect(validateTriggers([installed, t('disarm.secondary')])).toEqual({
      message: guard,
      at: [1],
    });
    expect(validateTriggers([installed])).toBeNull();
  });

  it('reads what you typed in a Text row, not its regex', () => {
    const row = { pattern: '^\\s*\\s*$', enabled: true, mode: 'text' as const, text: ' ' };
    expect(validateTriggers([{ ...t('a'), patterns: [row] }])?.message).toContain(
      'needs a pattern',
    );
    expect(validateTriggers([{ ...t('a'), patterns: [{ ...row, text: 'x' }] }])).toBeNull();
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

  it('keeps the alert table of every trigger through a save', async () => {
    // Every key, as triggers_export sends the table.
    const alert = {
      banner: true,
      sound: 'chime',
      attention: 'once',
      background: true,
      words: false,
    } as const;
    const store = fakeStore([{ ...trigger('rest', 'sleep'), alert }, trigger('flee', 'flee')]);
    let draft = createDraft(await loadTriggers(store.api));
    expect(draft.items[0].value.alert).toEqual(alert);
    draft = updateDraftItem(draft, draft.items[1].uid, (t) => ({ ...t, enabled: false }));
    await saveTriggerDraft(draft, store.api);
    expect(store.list()[0].alert).toEqual(alert);
    expect(store.list()[1].alert).toBeUndefined();
  });

  it('saves nothing when the store answer does not read', async () => {
    const store = fakeStore([trigger('rest', 'sleep')]);
    const draft = addDraftItem(createDraft(await loadTriggers(store.api)), trigger('wake', 'x'));
    const broken = { ...store.api, exportTriggers: () => Promise.resolve('not json') };
    await expect(saveTriggerDraft(draft, broken)).rejects.toThrow('saved nothing');
    expect(store.names()).toEqual(['rest']);
  });

  it('moves one trigger to Prompts and leaves the rest as the store wrote them', async () => {
    const store = fakeStore([trigger('rest', 'sleep'), trigger('flee', 'flee')]);
    await moveTriggerToPrompts('flee', null, store.api);
    expect(store.list().map((t) => [t.name, t.target ?? 'line'])).toEqual([
      ['rest', 'line'],
      ['flee', 'prompt'],
    ]);
    expect(store.list()[1].actions).toEqual([{ kind: 'send', template: 'flee' }]);
    await expect(moveTriggerToPrompts('gone', null, store.api)).rejects.toThrow(
      'Vosh no longer has a trigger named “gone”.',
    );
    const broken = { ...store.api, exportTriggers: () => Promise.resolve('not json') };
    await expect(moveTriggerToPrompts('rest', null, broken)).rejects.toThrow('changed nothing');
    expect(store.list()[0].target).toBeUndefined();
  });

  it('moves only the trigger of the group it names to Prompts', async () => {
    const store = fakeStore([
      { ...trigger('greet', 'bow'), group: 'Tolliver' },
      { ...trigger('greet', 'wave'), group: 'Maren' },
    ]);
    await moveTriggerToPrompts('greet', 'Maren', store.api);
    expect(store.list().map((t) => [t.group, t.target ?? 'line'])).toEqual([
      ['Tolliver', 'line'],
      ['Maren', 'prompt'],
    ]);
    await expect(moveTriggerToPrompts('greet', null, store.api)).rejects.toThrow(
      'Vosh no longer has a trigger named “greet”.',
    );
  });

  it('saves two triggers of one name in two groups, each where it was', async () => {
    const store = fakeStore([{ ...trigger('greet', 'bow'), group: 'Tolliver' }]);
    let draft = createDraft(await loadTriggers(store.api));
    draft = addDraftItem(draft, { ...trigger(' greet ', 'wave'), group: 'Maren' });
    await saveTriggerDraft(draft, store.api);
    expect(store.list().map((t) => [t.name, t.group])).toEqual([
      ['greet', 'Tolliver'],
      ['greet', 'Maren'],
    ]);
  });

  it('moves only the copies of the preset it names', async () => {
    const store = fakeStore([
      { ...trigger('flee', 'flee'), group: 'mine' },
      { ...trigger('flee', 'flee'), group: 'Maren', preset: 'terror_events' },
    ]);
    await setTriggerGroups(new Map([['flee', '']]), store.api, 'terror_events');
    expect(store.list().map((t) => t.group)).toEqual(['mine', undefined]);
  });

  it('puts named triggers in their group and writes nothing when none moves', async () => {
    const store = fakeStore([
      { ...trigger('rest', 'sleep'), group: 'mine' },
      { ...trigger('flee', 'flee'), group: 'mine' },
    ]);
    await setTriggerGroups(
      new Map([
        ['rest', ''],
        ['gone', 'x'],
      ]),
      store.api,
    );
    expect(store.list().map((t) => [t.name, t.group])).toEqual([
      ['rest', undefined],
      ['flee', 'mine'],
    ]);
    const writes = vi.fn(store.api.importTriggers);
    await setTriggerGroups(new Map([['flee', 'mine']]), { ...store.api, importTriggers: writes });
    expect(writes).not.toHaveBeenCalled();
  });
});
