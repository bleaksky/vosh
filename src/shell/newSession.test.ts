import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import type { SessionRow } from '../ipc/session';
import { SESSION_MENU_EVENT, type OpenedSession } from '../lib/appMenu';
import { cancelNewSession, openNewSession } from './newSession';

// New session… adds a row on the profile the saved world picks, selects
// it and opens its form. Cancel closes that row and writes nothing,
// with the session you came from selected again first.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

/** The sessions store as the app last listed it, faked. */
const sessions = vi.hoisted(() => ({ rows: [] as SessionRow[], selected: 1 }));

vi.mock('../stores/session/sessionsStore', () => ({
  getSessions: () => sessions.rows,
  getSelected: () => sessions.selected,
  select: (id: number) => {
    sessions.selected = id;
    return invoke('session_select', { session: id }).then(() => undefined);
  },
}));

const PLAY = 'play.theforsakenlands.com';
const store = new Map<string, string>();
const requests: unknown[] = [];

function row(id: number, profile: string): SessionRow {
  return {
    id,
    name: null,
    character: 'Tolliver',
    host: PLAY,
    port: 1848,
    tls: false,
    profile,
    connected: true,
    since: null,
    selected: id === 1,
  };
}

beforeEach(() => {
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
  });
  vi.stubGlobal('window', {
    dispatchEvent: (e: CustomEvent) => {
      if (e.type === SESSION_MENU_EVENT) requests.push(e.detail);
      return true;
    },
  });
  sessions.rows = [row(1, 'default')];
  sessions.selected = 1;
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === 'profiles_list') {
      return Promise.resolve({
        active: 'default',
        profiles: [
          { name: 'default', auto_match: { host: PLAY, port: 1848, characters: ['Tolliver'] } },
          { name: 'Build', auto_match: { host: PLAY, port: 1825, characters: ['Orla'] } },
        ],
      });
    }
    if (cmd === 'profile_resolve_match') {
      // Rust's pick before login, for the claims the list above holds.
      const { host, port } = args as { host: string; port: number };
      if (host !== PLAY) return Promise.resolve(null);
      return Promise.resolve(port === 1825 ? 'Build' : 'default');
    }
    if (cmd === 'session_open') {
      sessions.rows = [...sessions.rows, { ...row(2, 'Build'), character: null, host: null }];
      return Promise.resolve(2);
    }
    return Promise.resolve();
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.mocked(invoke).mockReset();
  vi.mocked(emit).mockClear();
  store.clear();
  requests.length = 0;
});

describe('openNewSession', () => {
  it('opens a session on the profile the saved world picks, selects it and opens its form', async () => {
    store.set('vosh.connection.target', JSON.stringify({ host: PLAY, port: 1825, tls: false }));
    await openNewSession();
    const calls = vi.mocked(invoke).mock.calls.map(([cmd, args]) => [cmd, args]);
    expect(calls).toEqual([
      ['profiles_list', undefined],
      ['profile_resolve_match', { host: PLAY, port: 1825, character: null, anyCharacter: true }],
      ['session_open', { profile: 'Build' }],
      ['session_select', { session: 2 }],
    ]);
    const opened: OpenedSession = { id: 2, previous: 1, front: 'default', profile: 'Build' };
    expect(requests).toEqual([{ mode: 'new', opened }]);
  });

  it('keeps the profile in front when no claim names the saved world', async () => {
    store.set('vosh.connection.target', JSON.stringify({ host: 'mud.example.org', port: 4000 }));
    await openNewSession();
    expect(invoke).toHaveBeenCalledWith('session_open', { profile: 'default' });
  });
});

describe('cancelNewSession', () => {
  const opened: OpenedSession = { id: 2, previous: 1, front: 'default', profile: 'Build' };

  it('selects the session you came from, then closes the new row', async () => {
    sessions.rows = [row(1, 'default'), row(2, 'Build')];
    sessions.selected = 2;
    await cancelNewSession(opened);
    expect(vi.mocked(invoke).mock.calls).toEqual([
      ['session_select', { session: 1 }],
      ['session_close', { session: 2 }],
    ]);
  });

  it('writes nothing, neither the saved world nor an address', async () => {
    sessions.rows = [row(1, 'default'), row(2, 'Build')];
    sessions.selected = 2;
    await cancelNewSession(opened);
    expect(store.size).toBe(0);
    expect(emit).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith('session_set_address', expect.anything());
  });

  it('leaves the selection alone once another session holds it', async () => {
    sessions.rows = [row(1, 'default'), row(2, 'Build'), row(3, 'default')];
    sessions.selected = 3;
    await cancelNewSession(opened);
    expect(vi.mocked(invoke).mock.calls).toEqual([['session_close', { session: 2 }]]);
  });
});
