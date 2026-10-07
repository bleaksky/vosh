import { describe, expect, it } from 'vitest';
import aliasesExport from '../../fixtures/ipc/aliases_export.json?raw';
import keptKeys from '../../fixtures/macros/kept-keys.json';
import type { Macro } from '../ipc/automation';
import { PRESET_ALERT_DEFAULT } from './alertPresets';
import {
  addDraftItem,
  createDraft,
  draftChangeCount,
  draftValues,
  markAllWritten,
  removeDraftItem,
  replaceDraftValues,
  saveListThenPinned,
  updateDraftItem,
  isDraftDirty,
  type Draft,
  type SavedWrite,
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
  keysYourMacrosKeep,
  loadoutToggles,
  macroSavePlan,
  normalizeAlias,
  normalizeMacro,
  normalizeTick,
  normalizeTimer,
  parseJsonList,
  presetLaunchPlan,
  presetSavePlan,
  presetToggles,
  PRESETS_OFF_MARKER,
  saveMacroDraft,
  saveTimerDraft,
  storedPresetIds,
  timerEntry,
  timerKey,
  timerLabel,
  timerSavePlan,
  validateAliases,
  validateMacros,
  validateTimers,
  type MacroRecord,
  type MacroStoreApi,
  type TimerRecord,
  type TimerStoreApi,
} from './automationRecords';
import { buildSections, foldedStorageKey } from './automationList';
import { defaultEnabledIds, presetById, PRESETS } from './presets';

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

  it('reads every field aliases_export sends', async () => {
    // A Rust test in ipc/automation.rs holds this file to aliases_export.
    const list = await loadAliases({
      exportAliases: () => Promise.resolve(aliasesExport.trimEnd()),
      importAliases: () => Promise.resolve(0),
    });
    expect(list).toEqual([
      { name: 'cs', expansion: 'cast %1', enabled: true, group: 'magic' },
      { name: 'k', expansion: 'kill %1', enabled: true },
      { name: 'lk', expansion: 'kill %1', enabled: true, script: 'mud.send("look")' },
      {
        name: 'lt',
        expansion: 'look',
        enabled: true,
        script: 'mud.send("look " .. captures[1])\nmud.echo("looked")',
      },
      { name: 'off', expansion: 'say off', enabled: false },
      { name: 'rec', expansion: 'recall', enabled: true },
    ]);
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
    expect(plan.remove.map((r) => r.key).sort()).toEqual(['F1', 'F3']);
    expect(plan.remove.find((r) => r.key === 'F1')?.uids).toEqual([f1.uid]);
    expect(plan.remove.find((r) => r.key === 'F3')?.uids).toEqual([f3.uid]);
    expect(plan.set.map((s) => s.macro.key).sort()).toEqual(['F1', 'F2', 'F5']);
  });

  /** macros_set and macros_delete over one list. `failOn` makes the
   *  bind of that key throw once. */
  function macroStore(start: MacroRecord[]) {
    const store = start.map((m) => ({ ...m }));
    const calls: string[] = [];
    let failOn: string | null = null;
    // As set_macro and delete_macro in src-tauri/src/ipc/automation.rs
    // do: a preset macro takes only its group, and an unbind removes
    // only yours.
    const api: MacroStoreApi = {
      deleteMacro: async (key) => {
        calls.push(`delete ${key}`);
        store.splice(0, store.length, ...store.filter((m) => m.preset || m.key !== key));
      },
      setMacro: async (key, command, group, enabled, preset) => {
        if (key === failOn) {
          failOn = null;
          throw new Error('disk full');
        }
        calls.push(preset ? `group ${preset} ${key} ${group}` : `set ${key}`);
        const at = store.findIndex((m) => m.key === key && m.preset === preset);
        if (preset) {
          const { group: _, ...rest } = store[at];
          store[at] = { ...rest, ...(group ? { group } : {}) };
          return;
        }
        const next = { key, command, enabled, ...(group ? { group } : {}) };
        if (at >= 0) store[at] = next;
        else store.push(next);
      },
    };
    return { store, calls, api, failNext: (key: string) => (failOn = key) };
  }

  /** Save the way the page does: on a failure, keep the draft and mark
   *  what the store took as saved. */
  async function saveMacros(draft: Draft<MacroRecord>, api: MacroStoreApi) {
    const writes: SavedWrite<MacroRecord>[] = [];
    try {
      await saveMacroDraft(draft, (w) => writes.push(w), api);
      return { draft: createDraft(draft.items.map((i) => i.value)), error: null };
    } catch (e) {
      return { draft: markAllWritten(draft, writes), error: e };
    }
  }

  it('binds only the rest after a failure after the first of three new macros', async () => {
    const { store, calls, api, failNext } = macroStore(macros);
    let draft = createDraft(macros.map((m) => ({ ...m })));
    for (const key of ['F6', 'F7', 'F8']) {
      draft = addDraftItem(draft, { key, command: `cast ${key}`, enabled: true });
    }
    failNext('F7');
    const first = await saveMacros(draft, api);
    expect(first.error).toBeInstanceOf(Error);
    // F6 is saved. F7 and F8 stay as your unsaved changes.
    expect(macroSavePlan(first.draft).set.map((s) => s.macro.key)).toEqual(['F7', 'F8']);
    expect(draftValues(first.draft).map((m) => m.key)).toEqual([
      'F1',
      'F2',
      'F3',
      'F6',
      'F7',
      'F8',
    ]);

    const second = await saveMacros(first.draft, api);
    expect(second.error).toBeNull();
    expect(calls).toEqual(['set F6', 'set F7', 'set F8']);
    expect(store.map((m) => m.key)).toEqual(['F1', 'F2', 'F3', 'F6', 'F7', 'F8']);
  });

  it('binds a moved key after the old key unbound and the bind failed', async () => {
    const { store, calls, api, failNext } = macroStore(macros);
    let draft = createDraft(macros.map((m) => ({ ...m })));
    draft = updateDraftItem(draft, draft.items[0].uid, (m) => ({ ...m, key: 'F5' }));
    failNext('F5');
    const first = await saveMacros(draft, api);
    expect(first.error).toBeInstanceOf(Error);
    // F1 is gone from the store, so the macro now reads as new on F5.
    expect(macroSavePlan(first.draft)).toEqual({
      remove: [],
      set: [{ uid: draft.items[0].uid, macro: { key: 'F5', command: 'kick', enabled: true } }],
    });
    await saveMacros(first.draft, api);
    expect(calls).toEqual(['delete F1', 'set F5']);
    expect(store.map((m) => m.key).sort()).toEqual(['F2', 'F3', 'F5']);
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

  describe('a preset macro', () => {
    const rec = normalizeMacro({ key: 'Numpad3', command: 'rec' });
    const north = normalizeMacro({ key: 'Numpad8', command: 'n', preset: 'numpad_movement' });
    const down = normalizeMacro({
      key: 'Numpad3',
      command: 'd',
      enabled: false,
      preset: 'numpad_movement',
    });
    const stored = [rec, north, down];

    it('keeps the preset that added it', () => {
      expect(down).toEqual({
        key: 'Numpad3',
        command: 'd',
        enabled: false,
        preset: 'numpad_movement',
      });
      expect('preset' in normalizeMacro({ key: 'F1', command: 'x', preset: '' })).toBe(false);
    });

    it('lets your macro use its key, and needs nothing of it', () => {
      expect(validateMacros(stored)).toBeNull();
      expect(validateMacros([...stored, { ...north, command: ' ' }])).toBeNull();
      // Two of yours on one key still clash.
      expect(validateMacros([...stored, { ...rec, command: 'rest' }])).toBe(
        'Two macros use Numpad3. Give each one its own key.',
      );
    });

    it('saves only a new group, under the key and preset it loaded with', () => {
      let draft = createDraft(stored);
      const [, n, d] = draft.items;
      draft = updateDraftItem(draft, n.uid, (m) => ({ ...m, group: 'travel', command: 'north' }));
      // A command or a switch the JSON view changed sends nothing.
      draft = updateDraftItem(draft, d.uid, (m) => ({ ...m, enabled: true }));
      expect(macroSavePlan(draft)).toEqual({
        remove: [],
        set: [{ uid: n.uid, macro: { ...north, group: 'travel' } }],
      });
      // A key the JSON view changed saves the group under the old key.
      draft = updateDraftItem(draft, n.uid, (m) => ({ ...m, key: 'F9' }));
      expect(macroSavePlan(draft).set).toEqual([
        { uid: n.uid, macro: { ...north, group: 'travel' } },
      ]);
    });

    it('never unbinds your key for a preset macro the JSON view removed or added', async () => {
      const { store, calls, api } = macroStore(stored);
      let draft = createDraft(stored.map((m) => ({ ...m })));
      draft = removeDraftItem(draft, draft.items[2].uid);
      draft = addDraftItem(draft, { ...north, key: 'Numpad6', command: 'e' });
      expect(macroSavePlan(draft)).toEqual({ remove: [], set: [] });
      await saveMacroDraft(draft, () => {}, api);
      expect(calls).toEqual([]);
      expect(store).toEqual(stored);
    });

    it('takes a row the JSON view moved from yours to a preset as yours removed', () => {
      let draft = createDraft(stored);
      const [r] = draft.items;
      draft = updateDraftItem(draft, r.uid, (m) => ({ ...m, preset: 'numpad_movement' }));
      expect(macroSavePlan(draft)).toEqual({
        remove: [{ key: 'Numpad3', uids: [r.uid] }],
        set: [],
      });
      // And one moved from a preset to yours as yours added.
      draft = createDraft(stored);
      const [, n] = draft.items;
      draft = updateDraftItem(draft, n.uid, ({ preset: _, ...m }) => m);
      expect(macroSavePlan(draft)).toEqual({
        remove: [],
        set: [{ uid: n.uid, macro: { key: 'Numpad8', command: 'n', enabled: true } }],
      });
    });

    it('sends the group of a preset macro through macros_set with its preset', async () => {
      const { store, calls, api } = macroStore(stored);
      let draft = createDraft(stored.map((m) => ({ ...m })));
      const [r, , d] = draft.items;
      draft = updateDraftItem(draft, d.uid, (m) => ({ ...m, group: 'travel' }));
      draft = updateDraftItem(draft, r.uid, (m) => ({ ...m, command: 'recite' }));
      await saveMacroDraft(draft, () => {}, api);
      expect(calls).toEqual(['set Numpad3', 'group numpad_movement Numpad3 travel']);
      expect(store).toEqual([{ ...rec, command: 'recite' }, north, { ...down, group: 'travel' }]);
    });
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

  it('keeps a timer group trimmed and leaves a blank one out', () => {
    expect(normalizeTimer({ id: 3, command: 'drink', group: ' upkeep ' }).group).toBe('upkeep');
    expect('group' in normalizeTimer({ id: 3, command: 'drink', group: '  ' })).toBe(false);
    expect('group' in normalizeTimer({ id: 3, command: 'drink', group: null })).toBe(false);
  });

  it('lists timers under a heading for each group, folded under the timers list id', () => {
    const rows = [
      { ...timers[0], group: 'upkeep' },
      timers[1],
      normalizeTimer({ id: 3, command: 'rescue', group: 'combat' }),
    ].map((t, i) => ({ uid: String(i), ...timerEntry(t) }));
    const sections = buildSections(rows);
    expect(sections.map((s) => [s.heading, s.entries.map((e) => e.name)])).toEqual([
      [null, ['save']],
      ['combat', ['rescue']],
      ['upkeep', ['Hydrate']],
    ]);
    expect(sections[2].key).toBe('g:upkeep');
    expect(rows[0].meta).toBe('Every 5 min');
    // The filter finds a timer by its group.
    expect(rows[0].text).toContain('upkeep');
    expect(foldedStorageKey('timers')).toBe('vosh.automation.folded.timers');
  });

  it('sends each timer with its group, and none as null', async () => {
    const sent: unknown[][] = [];
    const api: TimerStoreApi = {
      timersDelete: () => Promise.resolve([]),
      timersSet: (...args) => {
        sent.push(args);
        return Promise.resolve([]);
      },
    };
    let draft = createDraft(timers);
    draft = updateDraftItem(draft, draft.items[0].uid, (t) => ({ ...t, group: 'upkeep' }));
    draft = addDraftItem(draft, { ...blankTimer(), command: 'look' });
    await saveTimerDraft(draft, () => {}, api);
    expect(sent).toEqual([
      [1, 'Hydrate', 300, 'drink', true, 'upkeep'],
      [null, '', 30, 'look', true, null],
    ]);
  });

  it('plans updates, creates, and deletes by id', () => {
    let draft = createDraft(timers);
    draft = updateDraftItem(draft, draft.items[0].uid, (t) => ({ ...t, interval_secs: 120 }));
    draft = removeDraftItem(draft, draft.items[1].uid);
    draft = addDraftItem(draft, { ...blankTimer(), command: 'look' });
    const plan = timerSavePlan(draft);
    expect(plan.remove.map((r) => r.id)).toEqual([2]);
    expect(plan.set.map((s) => [s.timer.id, s.timer.command])).toEqual([
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
    expect(plan.set.map((s) => s.timer)).toEqual([{ ...timers[0], id: null }]);
  });

  it('updates the id a timer loaded with, whatever id the JSON view gives it', () => {
    const draft = createDraft(timers);
    const edited = [{ ...timers[0], id: 2, interval_secs: 600 }, timers[1]];
    const plan = timerSavePlan(replaceDraftValues(draft, edited, timerKey));
    expect(plan.set.map((s) => [s.timer.id, s.timer.interval_secs])).toEqual([[1, 600]]);
  });

  /** timers_set and timers_delete over one list, the way the backend
   *  keeps it: a new timer takes the next id at the end. `failNext`
   *  makes the next create throw once. */
  function timerStore(start: TimerRecord[]) {
    const store = start.map((t) => ({ ...t }));
    let creates = 0;
    let failAt: number | null = null;
    const api: TimerStoreApi = {
      timersDelete: async (id) => {
        store.splice(0, store.length, ...store.filter((t) => t.id !== id));
        return store.map((t) => ({ ...t }));
      },
      timersSet: async (id, name, interval_secs, command, enabled) => {
        const at = store.findIndex((t) => t.id === id);
        if (at >= 0) {
          store[at] = { id, name, interval_secs, command, enabled };
        } else {
          creates += 1;
          if (creates === failAt) throw new Error('disk full');
          const next = Math.max(0, ...store.map((t) => t.id ?? 0)) + 1;
          store.push({ id: next, name, interval_secs, command, enabled });
        }
        return store.map((t) => ({ ...t }));
      },
    };
    return { store, api, failCreate: (n: number) => (failAt = creates + n) };
  }

  async function saveTimers(draft: Draft<TimerRecord>, api: TimerStoreApi) {
    const writes: SavedWrite<TimerRecord>[] = [];
    try {
      await saveTimerDraft(draft, (w) => writes.push(w), api);
      return { draft: null, error: null };
    } catch (e) {
      return { draft: markAllWritten(draft, writes), error: e };
    }
  }

  it('creates each new timer once after a failure after the first of three creates', async () => {
    const { store, api, failCreate } = timerStore(timers);
    let draft = createDraft(timers.map((t) => ({ ...t })));
    for (const command of ['look', 'score', 'inventory']) {
      draft = addDraftItem(draft, { ...blankTimer(), command });
    }
    failCreate(2);
    const first = await saveTimers(draft, api);
    expect(first.error).toBeInstanceOf(Error);
    expect(store.map((t) => t.command)).toEqual(['drink', 'save\n#echo saved', 'look']);
    const rebased = first.draft as Draft<TimerRecord>;
    // look is saved under the id the store gave it. score and inventory
    // are still yours to save, and nothing else is.
    expect(draftValues(rebased).find((t) => t.command === 'look')?.id).toBe(3);
    expect(draftChangeCount(rebased)).toBe(2);
    expect(timerSavePlan(rebased).set.map((s) => [s.timer.id, s.timer.command])).toEqual([
      [null, 'score'],
      [null, 'inventory'],
    ]);

    const second = await saveTimers(rebased, api);
    expect(second.error).toBeNull();
    expect(store.map((t) => [t.id, t.command])).toEqual([
      [1, 'drink'],
      [2, 'save\n#echo saved'],
      [3, 'look'],
      [4, 'score'],
      [5, 'inventory'],
    ]);
  });

  it('keeps a saved timer editable under its new id after a failed Save', async () => {
    const { store, api, failCreate } = timerStore(timers);
    let draft = addDraftItem(createDraft(timers.map((t) => ({ ...t }))), {
      ...blankTimer(),
      command: 'look',
    });
    draft = addDraftItem(draft, { ...blankTimer(), command: 'score' });
    failCreate(2);
    const rebased = (await saveTimers(draft, api)).draft as Draft<TimerRecord>;
    const look = rebased.items.find((i) => i.value.command === 'look');
    const edited = updateDraftItem(rebased, look?.uid ?? '', (t) => ({ ...t, interval_secs: 90 }));
    await saveTimers(edited, api);
    expect(store.filter((t) => t.command === 'look')).toEqual([
      { id: 3, name: '', interval_secs: 90, command: 'look', enabled: true },
    ]);
  });

  it('creates a new timer once when the tick fails after it', async () => {
    // timers_set and timers_list over one list.
    const store: TimerRecord[] = timers.map((t) => ({ ...t }));
    const write = async (d: Draft<TimerRecord>) => {
      const plan = timerSavePlan(d);
      for (const { timer: t } of plan.set) {
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

  it('keeps the ids no preset of this build knows', () => {
    const stored = [PRESETS[0].id, 'later_preset'];
    const off = presetToggles(stored).map((t) => ({ ...t, enabled: false }));
    expect(storedPresetIds(off, stored)).toEqual(['later_preset']);
    const on = presetToggles(stored);
    expect(storedPresetIds(on, stored)).toEqual([PRESETS[0].id, 'later_preset']);
    expect(storedPresetIds(off, [PRESETS_OFF_MARKER])).toEqual([PRESETS_OFF_MARKER]);
  });

  it('keeps or drops an alert preset by its toggle', () => {
    const stored = [PRESETS[0].id, 'alert_tells', 'alert_name'];
    const library = presetToggles(stored);
    const alerts = [
      { id: 'alert_tells', enabled: true, alert: PRESET_ALERT_DEFAULT },
      { id: 'alert_name', enabled: false, alert: PRESET_ALERT_DEFAULT },
      { id: 'alert_attacked', enabled: true, alert: PRESET_ALERT_DEFAULT },
    ];
    expect(storedPresetIds([...library, ...alerts], stored)).toEqual([
      PRESETS[0].id,
      'alert_tells',
      'alert_attacked',
    ]);
  });

  it('drops the marker when an alert preset is all that is on', () => {
    const library = presetToggles([PRESETS_OFF_MARKER]);
    const tells = { id: 'alert_tells', enabled: true, alert: PRESET_ALERT_DEFAULT };
    const stored = storedPresetIds([...library, tells], [PRESETS_OFF_MARKER]);
    expect(stored).toEqual(['alert_tells']);
    expect(enabledPresetIds(stored)).toEqual([]);
    const off = { ...tells, enabled: false };
    expect(storedPresetIds([...library, off], stored)).toEqual([PRESETS_OFF_MARKER]);
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

  it('leaves the alert presets out of installs and removals', () => {
    let draft = createDraft([
      ...presetToggles([PRESETS[0].id]),
      { id: 'alert_tells', enabled: false, alert: PRESET_ALERT_DEFAULT },
      { id: 'alert_name', enabled: true, alert: PRESET_ALERT_DEFAULT },
    ]);
    const uid = (id: string) => draft.items.find((i) => i.value.id === id)?.uid ?? '';
    draft = updateDraftItem(draft, uid('alert_tells'), (t) => ({ ...t, enabled: true }));
    draft = updateDraftItem(draft, uid('alert_name'), (t) => ({ ...t, enabled: false }));
    draft = updateDraftItem(draft, uid(PRESETS[1].id), (t) => ({ ...t, enabled: true }));
    expect(presetSavePlan(draft)).toEqual({ install: [PRESETS[1].id], remove: [] });
  });

  it('installs the presets that are on at launch, in library order', () => {
    const pick = [PRESETS[2].id, PRESETS[0].id];
    expect(presetLaunchPlan(pick, []).install).toEqual([PRESETS[0].id, PRESETS[2].id]);
    expect(presetLaunchPlan([], []).install).toEqual(defaultEnabledIds());
    expect(presetLaunchPlan([PRESETS_OFF_MARKER], []).install).toEqual([]);
  });

  it('takes out at launch a preset you turned off that another profile put back', () => {
    // Loadout mode: the store holds the triggers every profile shares.
    // You turned the first preset off, and a launch as another character
    // installed it again from that character's list.
    const off = PRESETS[0].id;
    const on = PRESETS[1].id;
    const installed = [off, off, on, null, undefined];
    const plan = presetLaunchPlan([on], installed);
    expect(plan.install).toEqual([on]);
    expect(plan.remove).toEqual([off]);
  });

  it('takes out at launch a preset this build no longer has', () => {
    const plan = presetLaunchPlan([], ['renamed_long_ago', ...defaultEnabledIds()]);
    expect(plan.remove).toEqual(['renamed_long_ago']);
  });

  it('turns presets on and off over the stored list first', () => {
    const [first, second] = [PRESETS[0].id, PRESETS[1].id];
    const plan = presetLaunchPlan(
      [first],
      [first],
      [
        { id: first, on: false },
        { id: second, on: true },
      ],
    );
    expect(plan).toEqual({ install: [second], remove: [first] });
  });

  it('removes nothing at launch while the store matches the list', () => {
    expect(presetLaunchPlan([], defaultEnabledIds()).remove).toEqual([]);
    expect(presetLaunchPlan([PRESETS_OFF_MARKER], [null, undefined]).remove).toEqual([]);
  });

  it('reads the preset tags of the macros beside those of the triggers at launch', () => {
    // The trigger tags, then the macro tags. Your F1 has none, and Numpad
    // movement tags the six keys it binds.
    const installed = [
      ...defaultEnabledIds(),
      undefined,
      ...Array<string>(6).fill('numpad_movement'),
    ];
    // Off, so a launch takes its macros out, even after another profile
    // or an older build put them back.
    expect(presetLaunchPlan([], installed)).toEqual({
      install: defaultEnabledIds(),
      remove: ['numpad_movement'],
    });
    // On, so a launch installs it again and takes nothing out.
    const on = [...defaultEnabledIds(), 'numpad_movement'];
    expect(presetLaunchPlan(on, installed)).toEqual({ install: on, remove: [] });
  });

  it('names the keys of a preset your macros keep, in the preset order', () => {
    const numpad = presetById('numpad_movement');
    if (!numpad) throw new Error('no numpad_movement preset');
    const theirs = { key: 'Numpad8', command: 'n', preset: 'numpad_movement' };
    expect(keysYourMacrosKeep(numpad, [theirs, { key: 'F1', command: 'score' }])).toEqual([]);
    // Yours keeps a key while it is on, off or in a group.
    expect(
      keysYourMacrosKeep(numpad, [
        { key: 'Numpad3', command: 'rec', enabled: false },
        { key: 'Numpad9', command: 'gate', group: 'travel' },
        theirs,
      ]),
    ).toEqual(['Numpad9', 'Numpad3']);
    // A preset with no macros wants no key.
    const heals = presetById('healing_basics');
    if (!heals) throw new Error('no healing_basics preset');
    expect(keysYourMacrosKeep(heals, [{ key: 'Numpad3', command: 'rec' }])).toEqual([]);
  });

  it('names the keys the Rust hold holds off, over the cases both sides read', () => {
    // A test in src-tauri/src/loadouts/presets.rs holds hold_taken_keys to
    // the same cases, so the Presets card and Rust agree on every key.
    const preset = presetById(keptKeys.preset);
    if (!preset) throw new Error(`no ${keptKeys.preset} preset`);
    for (const c of keptKeys.cases) {
      const macros: readonly Macro[] = c.macros;
      expect(keysYourMacrosKeep(preset, macros), c.about).toEqual(c.kept);
      // The rows carry the library's own macros, in its order.
      const theirs = macros.filter((m) => m.preset === preset.id);
      expect(
        theirs.map(({ key, command }) => ({ key, command })),
        c.about,
      ).toEqual(preset.macros);
    }
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
    // tick_set_config answers with this sentence, which passes through.
    const sentence = 'Vosh could not read the Reset on pattern. Check it and save again.';
    expect(automationSaveError(sentence)).toBe(sentence);
    expect(automationSaveError(new Error(sentence))).toBe(sentence);
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
