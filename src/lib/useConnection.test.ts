import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { pushToast } from './toasts';
import {
  CONNECTION_TARGET_EVENT,
  connectTo,
  KNOWN_WORLDS,
  knownWorld,
  loadTarget,
  parseTarget,
  profileSwitchErrorMessage,
  saveConnectionTarget,
  subscribeConnectionTarget,
  worldName,
} from './useConnection';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('./toasts', () => ({ pushToast: vi.fn() }));

describe('the saved target', () => {
  const store = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
  });
  afterEach(() => {
    store.clear();
    vi.mocked(emit).mockClear();
    vi.mocked(listen).mockClear();
  });

  it('saves for the next launch and tells every window', () => {
    const target = { host: 'mud.example.org', port: 4000, tls: true };
    saveConnectionTarget(target);
    expect(loadTarget()).toEqual(target);
    expect(emit).toHaveBeenCalledWith(CONNECTION_TARGET_EVENT, target);
  });

  it('follows a target another window saves and ignores a bad one', async () => {
    let handler: ((event: { payload: unknown }) => void) | undefined;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementationOnce((_name, fn) => {
      handler = fn as typeof handler;
      return Promise.resolve(unlisten);
    });
    const seen: unknown[] = [];
    const stop = subscribeConnectionTarget((t) => seen.push(t));
    expect(listen).toHaveBeenCalledWith(CONNECTION_TARGET_EVENT, expect.any(Function));
    handler?.({ payload: { host: 'mud.example.org', port: 23, tls: false } });
    handler?.({ payload: { host: '', port: 23 } });
    expect(seen).toEqual([{ host: 'mud.example.org', port: 23, tls: false }]);
    await Promise.resolve();
    stop();
    expect(unlisten).toHaveBeenCalled();
    handler?.({ payload: { host: 'late.example.org', port: 23 } });
    expect(seen).toHaveLength(1);
  });
});

describe('knownWorld', () => {
  it('finds the world a host plays', () => {
    expect(knownWorld('play.theforsakenlands.com')?.name).toBe('The Forsaken Lands');
    expect(knownWorld('mud.example.org')).toBeUndefined();
  });
});

describe('KNOWN_WORLDS', () => {
  it('knows where to connect to The Forsaken Lands', () => {
    expect(KNOWN_WORLDS).toContainEqual({
      domain: 'theforsakenlands.com',
      name: 'The Forsaken Lands',
      host: 'play.theforsakenlands.com',
      port: 1848,
    });
  });

  it('names every known world by its own host', () => {
    for (const world of KNOWN_WORLDS) expect(worldName(world.host)).toBe(world.name);
  });
});

describe('worldName', () => {
  it('names a known world by its host or any subdomain', () => {
    expect(worldName('play.theforsakenlands.com')).toBe('The Forsaken Lands');
    expect(worldName('theforsakenlands.com')).toBe('The Forsaken Lands');
    expect(worldName(' Play.TheForsakenLands.com. ')).toBe('The Forsaken Lands');
  });

  it('shows any other host as typed', () => {
    expect(worldName('mud.example.org')).toBe('mud.example.org');
    expect(worldName('nottheforsakenlands.com')).toBe('nottheforsakenlands.com');
  });
});

describe('parseTarget', () => {
  it('keeps a host, a TCP port, and the TLS flag', () => {
    expect(parseTarget({ host: ' mud.example.org ', port: 4000, tls: true })).toEqual({
      host: 'mud.example.org',
      port: 4000,
      tls: true,
    });
    expect(parseTarget({ host: 'mud.example.org', port: '23' })).toEqual({
      host: 'mud.example.org',
      port: 23,
      tls: false,
    });
  });

  it('rejects a blank host or a port out of range', () => {
    expect(parseTarget({ host: '  ', port: 23 })).toBeNull();
    expect(parseTarget({ host: 'mud.example.org', port: 0 })).toBeNull();
    expect(parseTarget({ host: 'mud.example.org', port: 70000 })).toBeNull();
    expect(parseTarget({ host: 'mud.example.org', port: 23.5 })).toBeNull();
    expect(parseTarget(null)).toBeNull();
  });
});

describe('connectTo', () => {
  const HEALER_UNREADABLE =
    'Vosh could not open the Healer profile because it could not read the profile file. You are still using the Default profile.';
  const target = { host: 'play.theforsakenlands.com', port: 1848, tls: false };

  /** A backend whose match for the target is Healer while Default is
   *  live, and whose switch answers with `switched`. */
  function backend(switched: () => Promise<unknown>) {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'profile_resolve_match') return Promise.resolve('Healer');
      if (cmd === 'profiles_list') return Promise.resolve({ active: 'default', profiles: [] });
      if (cmd === 'profile_switch') return switched();
      return Promise.resolve();
    });
  }

  afterEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(pushToast).mockClear();
  });

  it('shows a switch that failed as a toast and connects under the live profile', async () => {
    backend(() => Promise.reject(HEALER_UNREADABLE));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    await connectTo(target);
    warn.mockRestore();
    expect(pushToast).toHaveBeenCalledWith({ kind: 'error', message: HEALER_UNREADABLE });
    expect(invoke).toHaveBeenLastCalledWith('session_connect', target);
  });

  it('switches quietly when the switch works', async () => {
    backend(() => Promise.resolve());
    await connectTo(target);
    expect(invoke).toHaveBeenCalledWith('profile_switch', { name: 'Healer' });
    expect(pushToast).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenLastCalledWith('session_connect', target);
  });
});

describe('profileSwitchErrorMessage', () => {
  it('passes the backend sentence through', () => {
    expect(profileSwitchErrorMessage(' Vosh cannot find a profile named Healer. ')).toBe(
      'Vosh cannot find a profile named Healer.',
    );
    expect(profileSwitchErrorMessage(new Error('Vosh cannot find a profile named Healer.'))).toBe(
      'Vosh cannot find a profile named Healer.',
    );
  });

  it('says what happened when the backend gives no sentence', () => {
    expect(profileSwitchErrorMessage('')).toBe(
      'Vosh could not switch profiles, so you connect with the profile you were using.',
    );
  });
});
