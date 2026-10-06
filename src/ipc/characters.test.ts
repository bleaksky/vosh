import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultLayout } from '../panel/paneLayout';

// The Characters wrappers in src/ipc. Tauri takes camelCase argument
// keys from JS, and an optional profile must reach the backend as null
// rather than go missing, so these pin the exact calls.
const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: tauri.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn((event: string, handler: (event: { payload: unknown }) => void) => {
    tauri.handlers.set(event, handler);
    return Promise.resolve(() => undefined);
  }),
}));

const { trackedAffectsSet } = await import('./affects');
const {
  profileDetailGet,
  profileExportFile,
  profileSetLogin,
  profileSetWorld,
  sessionIdentityGet,
  subscribeProfileChanged,
  subscribeSessionIdentity,
} = await import('./characters');
const { profileCreate } = await import('./profiles');

const fire = (event: string, payload: unknown) => tauri.handlers.get(event)?.({ payload });

beforeEach(() => {
  tauri.invoke.mockReset();
  tauri.invoke.mockResolvedValue(undefined);
  tauri.handlers.clear();
});

describe('profile wrappers', () => {
  it('creates a profile with camelCase keys and nulls for what you leave out', async () => {
    await profileCreate('Corvanne');
    expect(tauri.invoke).toHaveBeenLastCalledWith('profile_create', {
      name: 'Corvanne',
      copyFrom: null,
      autoMatch: null,
    });
    const autoMatch = { host: 'play.theforsakenlands.com', port: 1848, characters: ['Corvanne'] };
    await profileCreate('Corvanne', 'default', autoMatch);
    expect(tauri.invoke).toHaveBeenLastCalledWith('profile_create', {
      name: 'Corvanne',
      copyFrom: 'default',
      autoMatch,
    });
  });

  it('sends the login toggle and the world as the backend names them', async () => {
    await profileSetLogin('Test-Prompt', 'Ilsabet', true);
    expect(tauri.invoke).toHaveBeenLastCalledWith('profile_set_login', {
      name: 'Test-Prompt',
      character: 'Ilsabet',
      on: true,
    });
    await profileSetWorld('Test-Prompt', null, null);
    expect(tauri.invoke).toHaveBeenLastCalledWith('profile_set_world', {
      name: 'Test-Prompt',
      host: null,
      port: null,
    });
  });

  it('exports with the characters you ticked, and none unless you tick some', async () => {
    await profileExportFile('Healer');
    expect(tauri.invoke).toHaveBeenLastCalledWith('profile_export_file', {
      name: 'Healer',
      characters: [],
    });
    await profileExportFile('Healer', ['Orla']);
    expect(tauri.invoke).toHaveBeenLastCalledWith('profile_export_file', {
      name: 'Healer',
      characters: ['Orla'],
    });
  });

  it('reads a profile with its tracked affects and panes cleaned up', async () => {
    tauri.invoke.mockResolvedValueOnce({
      name: 'default',
      display_name: 'Default',
      active: false,
      auto_match: null,
      world_name: null,
      tracked_affects: ['Haste', { name: ' Fly ', label: '' }],
      panes: { panel_open: false, root: null },
      generation: null,
      login_on: false,
    });
    const detail = await profileDetailGet('default');
    expect(tauri.invoke).toHaveBeenLastCalledWith('profile_detail_get', { name: 'default' });
    expect(detail.tracked_affects).toEqual([
      { name: 'Haste', label: null },
      { name: 'Fly', label: null },
    ]);
    expect(detail.panes).toEqual({ ...defaultLayout(), panel_open: false });
  });
});

describe('per profile edits', () => {
  it('sends a null profile for the live one and the name for another', async () => {
    tauri.invoke.mockResolvedValue([{ name: 'Haste' }]);
    const list = [{ name: 'Haste', label: null }];
    expect(await trackedAffectsSet(list)).toEqual(list);
    expect(tauri.invoke).toHaveBeenLastCalledWith('tracked_affects_set', { list, profile: null });
    await trackedAffectsSet(list, 'Healer');
    expect(tauri.invoke).toHaveBeenLastCalledWith('tracked_affects_set', {
      list,
      profile: 'Healer',
    });
  });
});

describe('events', () => {
  it('hands on the name of the profile that changed', async () => {
    const seen: string[] = [];
    await subscribeProfileChanged((name) => seen.push(name));
    fire('vosh://profile-changed', { name: 'Healer' });
    fire('vosh://profile-changed', 'Healer');
    fire('vosh://profile-changed', null);
    expect(seen).toEqual(['Healer']);
  });

  it('hands on the session identity, or null once you disconnect', async () => {
    const identity = {
      host: 'play.theforsakenlands.com',
      port: 1848,
      character: 'Ilsabet',
      profile: 'default',
      claimed_by: 'default',
    };
    const seen: unknown[] = [];
    await subscribeSessionIdentity((next) => seen.push(next));
    fire('vosh://session-identity-changed', identity);
    fire('vosh://session-identity-changed', null);
    expect(seen).toEqual([identity, null]);

    tauri.invoke.mockResolvedValueOnce(null);
    expect(await sessionIdentityGet(2)).toBeNull();
    expect(tauri.invoke).toHaveBeenLastCalledWith('session_identity_get', { session: 2 });
  });
});
