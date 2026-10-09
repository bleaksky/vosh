import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Macro } from '../../ipc/automation';

// The macro list under Presets and Macros speaks of the profile Settings
// shows. Editing Build while Default is in front, the kept key note on
// the Numpad movement card names Build's macros, not Default's.

const fake = vi.hoisted(() => ({
  shown: 'Build' as string | undefined,
  held: true,
  lists: new Map<string, Macro[]>(),
  reads: [] as (string | null | undefined)[],
  changed: null as ((m: Macro[]) => void) | null,
  moved: null as (() => void) | null,
  groups: null as (() => void) | null,
}));

vi.mock('../../ipc/automation', () => ({
  listMacros: async (profile?: string | null) => {
    fake.reads.push(profile);
    return fake.lists.get(profile ?? 'Default') ?? [];
  },
  subscribeMacrosChanged: async (cb: (m: Macro[]) => void) => {
    fake.changed = cb;
    return () => undefined;
  },
  subscribeMacroGroupsChanged: async (cb: () => void) => {
    fake.groups = cb;
    return () => undefined;
  },
}));
vi.mock('../../ipc/profiles', () => ({
  subscribeProfileSwitched: async () => () => undefined,
}));
vi.mock('../shownProfile', () => ({
  getShownProfile: () => fake.shown,
  isShownHeld: () => fake.held,
  subscribeShownMoves: (cb: () => void) => {
    fake.moved = cb;
    return () => undefined;
  },
}));

const macro = (key: string, command: string): Macro => ({
  key,
  command,
  group: null,
  enabled: true,
  preset: null,
});
const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  fake.lists.set('Build', [macro('Numpad3', 'rec')]);
  fake.lists.set('Default', [macro('F1', 'score')]);
});

describe('the macro list Settings reads', () => {
  it('reads the profile Settings shows, follows it as it moves, and skips the front list while held', async () => {
    const { store } = await import('./macroListStore');
    const stop = store.subscribe(() => undefined);
    await settle();
    expect(fake.reads).toEqual(['Build']);
    expect(store.get()).toEqual([macro('Numpad3', 'rec')]);

    // A change to Default, the profile in front, is not the list Settings shows.
    fake.changed?.([macro('F2', 'flee')]);
    expect(store.get()).toEqual([macro('Numpad3', 'rec')]);

    // Save lets go of Build, and Settings moves to Default.
    fake.shown = 'Default';
    fake.held = false;
    fake.moved?.();
    await settle();
    expect(fake.reads).toEqual(['Build', 'Default']);
    expect(store.get()).toEqual([macro('F1', 'score')]);

    // Now the front list is the one it shows.
    fake.changed?.([macro('F2', 'flee')]);
    expect(store.get()).toEqual([macro('F2', 'flee')]);

    // A macro group that turns can hand a key to a preset macro in
    // loadout mode, so the list comes again.
    fake.lists.set('Default', [macro('Numpad3', 'd')]);
    fake.groups?.();
    await settle();
    expect(fake.reads).toEqual(['Build', 'Default', 'Default']);
    expect(store.get()).toEqual([macro('Numpad3', 'd')]);
    stop();
  });
});
