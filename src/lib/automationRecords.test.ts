import { describe, expect, it } from 'vitest';
import {
  addDraftItem,
  createDraft,
  removeDraftItem,
  replaceDraftValues,
  saveListThenPinned,
  updateDraftItem,
  isDraftDirty,
  type Draft,
} from './automationDraft';
import {
  activeLoadouts,
  aliasesForSave,
  loadAliases,
  saveAliasDraft,
  automationSaveError,
  blankTimer,
  enabledPresetIds,
  formatInterval,
  importErrorMessage,
  jsonListText,
  loadoutToggles,
  macroSavePlan,
  normalizeAlias,
  normalizeMacro,
  normalizeTick,
  normalizeTimer,
  parseJsonList,
  presetSavePlan,
  presetToggles,
  PRESETS_OFF_MARKER,
  storedPresetIds,
  timerKey,
  timerLabel,
  timerSavePlan,
  validateAliases,
  validateMacros,
  validateTimers,
  type TimerRecord,
} from './automationRecords';
import { defaultEnabledIds, PRESETS } from './presets';

describe('aliases', () => {
  it('normalizes and saves in the shape aliases_import reads', () => {
    const raw = [
      { name: 'kk', expansion: 'kick %1', enabled: true, group: 'combat' },
      { name: 'lua', expansion: '', enabled: false, script: 'mud.send("look")', group: null },
    ];
    const list = raw.map(normalizeAlias);
    expect(list[1]).toEqual({
      name: 'lua',
      expansion: '',
      enabled: false,
      script: 'mud.send("look")',
    });
    const back = (JSON.parse(aliasesForSave(list)) as unknown[]).map(normalizeAlias);
    expect(back).toEqual(list);
  });

  it('keeps an empty Lua body as Lua mode', () => {
    expect(normalizeAlias({ name: 'x', expansion: 'y', script: '' }).script).toBe('');
  });

  it('asks for unique names', () => {
    expect(validateAliases([normalizeAlias({ name: 'kk', expansion: '' })])).toBeNull();
    expect(validateAliases([normalizeAlias({ name: ' ' })])).toContain('name');
    expect(
      validateAliases([normalizeAlias({ name: 'kk' }), normalizeAlias({ name: 'kk' })]),
    ).toContain('Two aliases');
  });
});

describe('saving aliases', () => {
  it('keeps an alias #alias added after the page loaded', async () => {
    let json = JSON.stringify([
      { name: 'k', expansion: 'kick' },
      { name: 'b', expansion: 'bash' },
    ]);
    const api = {
      exportAliases: () => Promise.resolve(json),
      importAliases: (next: string) => {
        json = next;
        return Promise.resolve(0);
      },
    };
    let draft = createDraft(await loadAliases(api));
    // #alias in the main window, after the page loaded.
    json = JSON.stringify([...(JSON.parse(json) as unknown[]), { name: 'r', expansion: 'rescue' }]);
    draft = updateDraftItem(draft, draft.items[0].uid, (a) => ({ ...a, expansion: 'kick %1' }));
    await saveAliasDraft(draft, api);
    const saved = JSON.parse(json) as { name: string; expansion: string }[];
    expect(saved.map((a) => [a.name, a.expansion])).toEqual([
      ['k', 'kick %1'],
      ['b', 'bash'],
      ['r', 'rescue'],
    ]);
  });
});

describe('macros', () => {
  const macros = [
    normalizeMacro({ key: 'F1', command: 'kick' }),
    normalizeMacro({ key: 'F2', command: 'bash', group: 'combat', enabled: false }),
    normalizeMacro({ key: 'F3', command: 'flee' }),
  ];

  it('reads a missing enabled as on', () => {
    expect(macros[0]).toEqual({ key: 'F1', command: 'kick', enabled: true });
    expect(macros[1].enabled).toBe(false);
  });

  it('plans unbinds before binds, including a key that moved', () => {
    let draft = createDraft(macros);
    const [f1, f2, f3] = draft.items;
    draft = updateDraftItem(draft, f1.uid, (m) => ({ ...m, key: 'F5' }));
    draft = updateDraftItem(draft, f2.uid, (m) => ({ ...m, enabled: true }));
    draft = removeDraftItem(draft, f3.uid);
    draft = addDraftItem(draft, { key: 'F1', command: 'rescue', enabled: true });
    const plan = macroSavePlan(draft);
    expect(plan.remove.sort()).toEqual(['F1', 'F3']);
    expect(plan.set.map((m) => m.key).sort()).toEqual(['F1', 'F2', 'F5']);
  });

  it('plans nothing for a clean draft', () => {
    expect(macroSavePlan(createDraft(macros))).toEqual({ remove: [], set: [] });
  });

  it('asks for a key, a command, and unique keys', () => {
    expect(validateMacros(macros)).toBeNull();
    expect(validateMacros([{ key: '', command: 'x', enabled: true }])).toContain('Press a key');
    expect(validateMacros([{ key: 'F1', command: ' ', enabled: true }])).toBe(
      'The macro on F1 needs a command.',
    );
    expect(validateMacros([macros[0], macros[0]])).toContain('Two macros use F1');
  });
});

describe('timers', () => {
  const timers = [
    normalizeTimer({ id: 1, name: 'Hydrate', interval_secs: 300, command: 'drink', enabled: true }),
    normalizeTimer({
      id: 2,
      name: '',
      interval_secs: 60,
      command: 'save\n#echo saved',
      enabled: 0,
    }),
  ];

  it('normalizes the backend shape', () => {
    expect(timers[1]).toEqual({
      id: 2,
      name: '',
      interval_secs: 60,
      command: 'save\n#echo saved',
      enabled: true,
    });
    expect(normalizeTimer({ interval_secs: 0, command: 'x' }).interval_secs).toBe(1);
  });

  it('plans updates, creates, and deletes by id', () => {
    let draft = createDraft(timers);
    draft = updateDraftItem(draft, draft.items[0].uid, (t) => ({ ...t, interval_secs: 120 }));
    draft = removeDraftItem(draft, draft.items[1].uid);
    draft = addDraftItem(draft, { ...blankTimer(), command: 'look' });
    const plan = timerSavePlan(draft);
    expect(plan.remove).toEqual([2]);
    expect(plan.set.map((t) => [t.id, t.command])).toEqual([
      [1, 'drink'],
      [null, 'look'],
    ]);
  });

  it('creates a timer copied in the JSON view instead of overwriting its original', () => {
    const draft = createDraft(timers);
    // You copy the first entry, id and all, and paste it at the end.
    const text = jsonListText([...timers, { ...timers[0] }]);
    const next = replaceDraftValues(draft, parseJsonList(text, normalizeTimer) ?? [], timerKey);
    const plan = timerSavePlan(next);
    expect(plan.remove).toEqual([]);
    expect(plan.set).toEqual([{ ...timers[0], id: null }]);
  });

  it('updates the id a timer loaded with, whatever id the JSON view gives it', () => {
    const draft = createDraft(timers);
    const edited = [{ ...timers[0], id: 2, interval_secs: 600 }, timers[1]];
    const plan = timerSavePlan(replaceDraftValues(draft, edited, timerKey));
    expect(plan.set.map((t) => [t.id, t.interval_secs])).toEqual([[1, 600]]);
  });

  it('creates a new timer once when the tick fails after it', async () => {
    // timers_set and timers_list over one list.
    const store: TimerRecord[] = timers.map((t) => ({ ...t }));
    const write = async (d: Draft<TimerRecord>) => {
      const plan = timerSavePlan(d);
      for (const t of plan.set) {
        if (t.id === null) store.push({ ...t, id: Math.max(...store.map((s) => s.id ?? 0)) + 1 });
        else
          store.splice(
            store.findIndex((s) => s.id === t.id),
            1,
            { ...t },
          );
      }
    };
    let draft = addDraftItem(createDraft(store.map((t) => ({ ...t }))), {
      ...blankTimer(),
      command: 'look',
    });
    const save = () =>
      saveListThenPinned({
        list: isDraftDirty(draft) ? () => write(draft) : null,
        // An invalid Reset on pattern.
        pinned: () => Promise.reject(new Error('invalid reset pattern')),
        reload: async () => {
          draft = createDraft(store.map((t) => ({ ...t })));
        },
      });
    await expect(save()).rejects.toThrow('invalid reset pattern');
    await expect(save()).rejects.toThrow('invalid reset pattern');
    expect(store.filter((t) => t.command === 'look')).toHaveLength(1);
    expect(isDraftDirty(draft)).toBe(false);
  });

  it('shows intervals in the largest whole unit', () => {
    expect(formatInterval(45)).toBe('45 s');
    expect(formatInterval(90)).toBe('90 s');
    expect(formatInterval(300)).toBe('5 min');
    expect(formatInterval(7200)).toBe('2 h');
  });

  it('names a timer by its name or its first command line', () => {
    expect(timerLabel(timers[0])).toBe('Hydrate');
    expect(timerLabel(timers[1])).toBe('save');
  });

  it('asks for a command', () => {
    expect(validateTimers(timers)).toBeNull();
    expect(validateTimers([{ ...blankTimer(), name: 'Rest' }])).toContain('Rest');
  });
});

describe('tick', () => {
  it('stores blank text as null and whole seconds', () => {
    expect(
      normalizeTick({
        enabled: true,
        interval_secs: 30.7,
        auto_fire: '',
        sound: false,
        reset_pattern: 'You are hungry',
        warn_at_secs: 0,
        warn_message: '',
        warn_color: null,
      }),
    ).toEqual({
      enabled: true,
      interval_secs: 30,
      auto_fire: null,
      sound: false,
      reset_pattern: 'You are hungry',
      warn_at_secs: null,
      warn_message: null,
      warn_color: null,
    });
  });
});

describe('presets', () => {
  it('reads an empty stored list as the defaults', () => {
    expect(enabledPresetIds([])).toEqual(defaultEnabledIds());
  });

  it('round trips every preset turned off', () => {
    const off = presetToggles([]).map((t) => ({ ...t, enabled: false }));
    const stored = storedPresetIds(off);
    expect(stored).toEqual([PRESETS_OFF_MARKER]);
    expect(enabledPresetIds(stored)).toEqual([]);
    expect(presetToggles(stored).every((t) => !t.enabled)).toBe(true);
  });

  it('round trips a partial pick in library order', () => {
    const pick = [PRESETS[2].id, PRESETS[0].id];
    const toggles = presetToggles(pick);
    expect(storedPresetIds(toggles)).toEqual([PRESETS[0].id, PRESETS[2].id]);
  });

  it('plans installs and removals from the toggles that changed', () => {
    let draft = createDraft(presetToggles([PRESETS[0].id]));
    draft = updateDraftItem(draft, draft.items[0].uid, (t) => ({ ...t, enabled: false }));
    draft = updateDraftItem(draft, draft.items[1].uid, (t) => ({ ...t, enabled: true }));
    expect(presetSavePlan(draft)).toEqual({ install: [PRESETS[1].id], remove: [PRESETS[0].id] });
    draft = updateDraftItem(draft, draft.items[1].uid, (t) => ({ ...t, enabled: false }));
    draft = updateDraftItem(draft, draft.items[0].uid, (t) => ({ ...t, enabled: true }));
    expect(isDraftDirty(draft)).toBe(false);
  });
});

describe('loadouts', () => {
  it('round trips the active list', () => {
    const toggles = loadoutToggles([{ name: 'warrior' }, { name: 'crafter' }], ['crafter']);
    expect(toggles).toEqual([
      { name: 'warrior', active: false },
      { name: 'crafter', active: true },
    ]);
    expect(activeLoadouts(toggles)).toEqual(['crafter']);
  });
});

describe('JSON view', () => {
  it('round trips a list through its text', () => {
    const list = [normalizeMacro({ key: 'F1', command: 'kick', group: 'combat' })];
    expect(parseJsonList(jsonListText(list), normalizeMacro)).toEqual(list);
  });

  it('reads anything but a list as null', () => {
    expect(parseJsonList('{"key":"F1"}', normalizeMacro)).toBeNull();
    expect(parseJsonList('[{"key":', normalizeMacro)).toBeNull();
  });
});

describe('automationSaveError', () => {
  it('turns store errors into sentences', () => {
    expect(automationSaveError('invalid regex `(`: unclosed group')).toBe(
      'Vosh could not read the pattern (. Fix it and save again.',
    );
    expect(automationSaveError('command cannot be empty')).toBe(
      'Every item needs a command before you save.',
    );
    expect(automationSaveError(new Error('Disk full.'))).toBe('Disk full.');
  });

  it('names the Tick field when its Reset on pattern does not read', () => {
    const raw =
      'invalid reset pattern: regex parse error:\n    [bad\n    ^\nerror: unclosed character class';
    expect(automationSaveError(raw)).toBe(
      'Vosh could not read the Reset on pattern. Fix it and save again.',
    );
  });
});

describe('importErrorMessage', () => {
  it('turns import_apply errors into sentences', () => {
    expect(importErrorMessage('could not detect import format', 'import')).toBe(
      'Vosh could not tell which client made this file. Choose its format and import again.',
    );
    expect(importErrorMessage('unknown import format: tintin', 'import')).toBe(
      'Vosh does not read that format. Choose one from the list and import again.',
    );
  });

  it('keeps a sentence and replaces anything else', () => {
    expect(importErrorMessage(new Error('Vosh is busy.'), 'import')).toBe('Vosh is busy.');
    expect(importErrorMessage(new Error('NotReadableError: read failed'), 'read')).toBe(
      'Vosh could not read that file. Choose it again or paste its contents.',
    );
    expect(importErrorMessage('Error: disk full.', 'import')).toBe(
      'Vosh could not import that file.',
    );
    expect(importErrorMessage('ipc: channel closed', 'import')).toBe(
      'Vosh could not import that file.',
    );
  });
});
