import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { CONNECTION_TARGET_CHANGED } from '../../ipc/events';
import { pushToast } from '../toasts';
import {
  connectOpened,
  connectOrRedial,
  connectTo,
  keepTarget,
  loadTarget,
  parseTarget,
  profileSwitchErrorMessage,
  saveConnectionTarget,
  subscribeConnectionTarget,
  targetOf,
} from './useConnection';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('../toasts', () => ({ pushToast: vi.fn() }));
const waiting = vi.hoisted(() => new Map<number, { host: string; port: number; tls: boolean }>());
vi.mock('./reconnectStore', () => ({
  waitingTarget: (session: number) => waiting.get(session) ?? null,
}));

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
    expect(emit).toHaveBeenCalledWith(CONNECTION_TARGET_CHANGED, target);
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
    expect(listen).toHaveBeenCalledWith(CONNECTION_TARGET_CHANGED, expect.any(Function));
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

  it('shows a switch that failed as a toast and connects the session under its profile', async () => {
    backend(() => Promise.reject(HEALER_UNREADABLE));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    await connectTo(target, 2);
    warn.mockRestore();
    expect(pushToast).toHaveBeenCalledWith({ kind: 'error', message: HEALER_UNREADABLE });
    expect(invoke).toHaveBeenLastCalledWith('session_connect', { ...target, session: 2 });
  });

  it('switches the session it connects quietly when the switch works', async () => {
    backend(() => Promise.resolve());
    await connectTo(target, 2);
    expect(invoke).toHaveBeenCalledWith('profile_switch', { name: 'Healer', session: 2 });
    expect(pushToast).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenLastCalledWith('session_connect', { ...target, session: 2 });
  });
});

describe('connectOrRedial', () => {
  const target = { host: 'play.theforsakenlands.com', port: 1848, tls: false };

  afterEach(() => {
    waiting.clear();
    vi.mocked(invoke).mockReset();
  });

  it('dials the waiting try now, as Reconnect now does', async () => {
    waiting.set(2, target);
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
    await connectOrRedial(target, 2);
    expect(vi.mocked(invoke).mock.calls).toEqual([['session_reconnect_now', { session: 2 }]]);
  });

  it('connects as Connect does while no redial waits', async () => {
    waiting.set(1, target);
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
    await connectOrRedial(target, 2);
    expect(invoke).not.toHaveBeenCalledWith('session_reconnect_now', expect.anything());
    expect(invoke).toHaveBeenLastCalledWith('session_connect', { ...target, session: 2 });
  });

  it.each([
    { host: 'localhost', port: 1848, tls: false },
    { host: 'play.theforsakenlands.com', port: 1825, tls: false },
    { host: 'play.theforsakenlands.com', port: 1848, tls: true },
  ])('connects to another world while a redial waits, which ends it', async (other) => {
    waiting.set(2, target);
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
    await connectOrRedial(other, 2);
    expect(invoke).not.toHaveBeenCalledWith('session_reconnect_now', expect.anything());
    expect(invoke).toHaveBeenLastCalledWith('session_connect', { ...other, session: 2 });
  });
});

describe('connectOpened', () => {
  const target = { host: 'play.theforsakenlands.com', port: 1825, tls: false };
  const store = new Map<string, string>();

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.mocked(invoke).mockReset();
    vi.mocked(emit).mockClear();
    store.clear();
  });

  it('dials the new session on the profile its form chose, with no match first', async () => {
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => void store.set(key, value),
    });
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
    await connectOpened(target, 2);
    expect(vi.mocked(invoke).mock.calls).toEqual([['session_connect', { ...target, session: 2 }]]);
    expect(loadTarget()).toEqual(target);
    expect(emit).toHaveBeenCalledWith(CONNECTION_TARGET_CHANGED, target);
  });
});

describe('the target of a session', () => {
  const saved = { host: 'play.theforsakenlands.com', port: 1848, tls: false };
  const store = new Map<string, string>();

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.mocked(invoke).mockReset();
    vi.mocked(emit).mockClear();
    store.clear();
  });

  it('is the target the session keeps, else the saved world', () => {
    const own = { host: 'play.theforsakenlands.com', port: 1825, tls: true };
    expect(targetOf(own, saved)).toEqual(own);
    expect(targetOf({ host: null, port: null, tls: false }, saved)).toBe(saved);
    expect(targetOf(null, saved)).toBe(saved);
  });

  it('keeps a new target for the session and as the saved world', async () => {
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => void store.set(key, value),
    });
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
    const target = { host: 'play.theforsakenlands.com', port: 1825, tls: false };
    await keepTarget(target, 2);
    expect(vi.mocked(invoke).mock.calls).toEqual([
      ['session_set_address', { session: 2, ...target }],
    ]);
    expect(loadTarget()).toEqual(target);
    expect(emit).toHaveBeenCalledWith(CONNECTION_TARGET_CHANGED, target);
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
