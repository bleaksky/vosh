import { act, createElement } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../../ipc/session';
import { FakeDocument, FakeElement, FakeNode } from '../../test/fakeDom';

// Drives the sessions store through a fake Tauri event bus. Each test
// loads a fresh store module, since the store keeps the list at module
// scope.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const commands = new Map<string, (args: unknown) => unknown>();
const calls: [string, unknown][] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args?: unknown) => {
    calls.push([cmd, args]);
    const answer = commands.get(cmd);
    if (!answer) throw new Error(`no fake for ${cmd}`);
    return answer(args);
  },
}));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const row = (id: number, character: string, port: number, selected = false): SessionRow => ({
  id,
  name: null,
  character,
  host: 'play.theforsakenlands.com',
  port,
  tls: false,
  profile: character,
  connected: true,
  since: null,
  selected,
});

const TOLLIVER = row(1, 'Tolliver', 1848, true);
const ORLA = row(2, 'Orla', 1825);
/** The same two sessions with Orla selected. */
const ORLA_SELECTED = [
  { ...TOLLIVER, selected: false },
  { ...ORLA, selected: true },
];

async function load() {
  const store = await import('./sessionsStore');
  store.startSessionsStore();
  await settle();
  return store;
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  commands.clear();
  calls.length = 0;
  commands.set('sessions_list', () => [TOLLIVER, ORLA]);
  commands.set('session_select', () => null);
});

describe('sessionsStore', () => {
  it('reads the list as it starts, with the selection the list marks', async () => {
    commands.set('sessions_list', () => ORLA_SELECTED);
    const store = await load();
    expect(store.getSessions()).toEqual(ORLA_SELECTED);
    expect(store.getSelected()).toBe(2);
  });

  it('keeps the first session selected until a list comes', async () => {
    commands.delete('sessions_list');
    const store = await load();
    expect(store.getSessions()).toEqual([]);
    expect(store.getSelected()).toBe(1);
  });

  it('takes the rows of each list the app sends', async () => {
    const store = await load();
    const renamed = [{ ...TOLLIVER, name: 'Hunting' }, ORLA];
    fire('vosh://sessions-changed', renamed);
    expect(store.getSessions()).toEqual(renamed);
    expect(store.getSelected()).toBe(1);
    fire('vosh://sessions-changed', ORLA_SELECTED);
    expect(store.getSelected()).toBe(2);
  });

  it('keeps a list that lands before the first read answers', async () => {
    let answer: (rows: SessionRow[]) => void = () => undefined;
    commands.set(
      'sessions_list',
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );
    const store = await load();
    fire('vosh://sessions-changed', ORLA_SELECTED);
    answer([TOLLIVER, ORLA]);
    await settle();
    expect(store.getSelected()).toBe(2);
  });

  it('reads the list again when a banner click selects a session', async () => {
    const store = await load();
    commands.set('sessions_list', () => ORLA_SELECTED);
    fire('vosh://session-selected', { session: 2 });
    await settle();
    expect(calls.filter(([cmd]) => cmd === 'sessions_list')).toHaveLength(2);
    expect(store.getSelected()).toBe(2);
  });

  it('selects a session at once and then tells the app', async () => {
    const store = await load();
    const moved = vi.fn();
    store.subscribeSelected(moved);
    store.select(2);
    expect(store.getSelected()).toBe(2);
    expect(moved).toHaveBeenCalledTimes(1);
    expect(calls.at(-1)).toEqual(['session_select', { session: 2 }]);
    // The list the app sends after the selection moves nothing.
    fire('vosh://sessions-changed', ORLA_SELECTED);
    expect(moved).toHaveBeenCalledTimes(1);
  });

  it('reads the list again when the app refuses a selection', async () => {
    const store = await load();
    commands.set('session_select', () => {
      throw new Error('No such session.');
    });
    store.select(3);
    expect(store.getSelected()).toBe(3);
    await settle();
    expect(store.getSelected()).toBe(1);
  });

  it('renames a session at once and then tells the app', async () => {
    const store = await load();
    commands.set('session_rename', () => null);
    const heard = vi.fn();
    store.subscribeSessions(heard);
    void store.rename(2, 'Builder');
    expect(store.getSessions()[1].name).toBe('Builder');
    expect(heard).toHaveBeenCalledTimes(1);
    expect(calls.at(-1)).toEqual(['session_rename', { session: 2, name: 'Builder' }]);
  });

  it('keeps a rename over a list read before it', async () => {
    const store = await load();
    commands.set('session_rename', () => null);
    let answer: (rows: SessionRow[]) => void = () => undefined;
    commands.set(
      'sessions_list',
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );
    fire('vosh://session-selected', { session: 1 });
    await settle();
    void store.rename(2, 'Builder');
    answer([TOLLIVER, ORLA]);
    await settle();
    expect(store.getSessions()[1].name).toBe('Builder');
  });

  it('reads the list again and says so when the app refuses a rename', async () => {
    const store = await load();
    const { getToasts } = await import('../toasts');
    // The notice leaves on a window timer.
    vi.stubGlobal('window', { setTimeout: () => 0 });
    try {
      commands.set('session_rename', () => {
        throw new Error('No session 2.');
      });
      await store.rename(2, 'Builder');
      await settle();
      expect(store.getSessions()[1].name).toBeNull();
      expect(getToasts().at(-1)).toMatchObject({ kind: 'error', message: 'No session 2.' });
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('moves a session at once and then tells the app', async () => {
    commands.set('sessions_list', () => [TOLLIVER, ORLA, row(3, 'Maren', 1848)]);
    const store = await load();
    commands.set('session_move', () => null);
    void store.move(3, 1);
    expect(store.getSessions().map((r) => r.id)).toEqual([1, 3, 2]);
    // ⌘2 reaches the row in its new place.
    expect(store.sessionAt(2)).toBe(3);
    expect(calls.at(-1)).toEqual(['session_move', { session: 3, to: 1 }]);
  });

  it('reads the list again and says so when the app refuses a move', async () => {
    const store = await load();
    const { getToasts } = await import('../toasts');
    vi.stubGlobal('window', { setTimeout: () => 0 });
    try {
      commands.set('session_move', () => {
        throw new Error('No session 2.');
      });
      await store.move(2, 0);
      await settle();
      expect(store.getSessions().map((r) => r.id)).toEqual([1, 2]);
      expect(getToasts().at(-1)).toMatchObject({ kind: 'error', message: 'No session 2.' });
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('opens the session the first list selects, and each selection the app finished', async () => {
    let finish: (value: null) => void = () => undefined;
    commands.set(
      'session_select',
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const store = await load();
    expect(store.getOpened()).toEqual([1]);
    // A selection shows at once, and the window opens the session only
    // once the app read what it kept.
    store.select(2);
    expect(store.getOpened()).toEqual([1]);
    finish(null);
    await settle();
    expect(store.getOpened()).toEqual([1, 2]);
    // A later list that selects the first session again opens nothing
    // new.
    fire('vosh://sessions-changed', [TOLLIVER, ORLA]);
    expect(store.getOpened()).toEqual([1, 2]);
  });

  it('opens the session a banner click selects', async () => {
    const store = await load();
    commands.set('sessions_list', () => ORLA_SELECTED);
    fire('vosh://session-selected', { session: 2 });
    await settle();
    expect(store.getOpened()).toEqual([1, 2]);
  });

  it('opens no session the app refuses, and lets one that closes go', async () => {
    const store = await load();
    commands.set('session_select', () => {
      throw new Error('No such session.');
    });
    store.select(3);
    await settle();
    expect(store.getOpened()).toEqual([1]);
    commands.set('session_select', () => null);
    store.select(2);
    await settle();
    expect(store.getOpened()).toEqual([1, 2]);
    fire('vosh://sessions-changed', [{ ...ORLA, selected: true }]);
    expect(store.getOpened()).toEqual([2]);
  });

  it('names the other sessions on the profile a session plays', async () => {
    const maren = { ...row(3, 'Maren', 1848), profile: 'Tolliver' };
    commands.set('sessions_list', () => [TOLLIVER, ORLA, maren]);
    const store = await load();
    expect(store.othersOnProfile(1)).toEqual([3]);
    expect(store.othersOnProfile(3)).toEqual([1]);
    expect(store.othersOnProfile(2)).toEqual([]);
    expect(store.othersOnProfile(9)).toEqual([]);
  });

  it('steps round the ends of the list, as otty does', async () => {
    const maren = row(3, 'Maren', 1848);
    commands.set('sessions_list', () => [TOLLIVER, ORLA, maren]);
    const store = await load();
    expect(store.sessionStep(1)).toBe(2);
    expect(store.sessionStep(-1)).toBe(3);
    fire('vosh://sessions-changed', [
      { ...TOLLIVER, selected: false },
      ORLA,
      { ...maren, selected: true },
    ]);
    expect(store.sessionStep(1)).toBe(1);
    expect(store.sessionStep(-1)).toBe(2);
  });

  it('steps nowhere with one session', async () => {
    commands.set('sessions_list', () => [TOLLIVER]);
    const store = await load();
    expect(store.sessionStep(1)).toBeNull();
    expect(store.sessionStep(-1)).toBeNull();
  });

  it('finds a session by its place in the list, from 1', async () => {
    const store = await load();
    expect(store.sessionAt(1)).toBe(1);
    expect(store.sessionAt(2)).toBe(2);
    expect(store.sessionAt(3)).toBeNull();
  });

  it('goes to a session only when it is another one', async () => {
    const store = await load();
    const asked = () => calls.filter(([cmd]) => cmd === 'session_select').length;
    store.goTo(null);
    store.goTo(1);
    expect(asked()).toBe(0);
    store.goTo(2);
    expect(store.getSelected()).toBe(2);
    expect(calls.at(-1)).toEqual(['session_select', { session: 2 }]);
  });

  it('tells a selection listener only when the selection moves', async () => {
    const store = await load();
    const moved = vi.fn();
    store.subscribeSelected(moved);
    fire('vosh://sessions-changed', [{ ...TOLLIVER, character: 'Maren' }, ORLA]);
    expect(moved).not.toHaveBeenCalled();
    fire('vosh://sessions-changed', ORLA_SELECTED);
    expect(moved).toHaveBeenCalledTimes(1);
  });
});

describe('the session hooks', () => {
  const doc = new FakeDocument();
  let createRoot: typeof import('react-dom/client').createRoot;

  beforeAll(async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener() {},
      removeEventListener() {},
    });
    vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    // React DOM checks for a DOM once, when it loads.
    ({ createRoot } = await import('react-dom/client'));
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  it('hand a view the rows and the selected row, and follow each change', async () => {
    const store = await load();
    const seen: string[] = [];
    function Rows() {
      const rows = store.useSessions();
      const selected = store.useSelectedRow();
      seen.push(`${rows.map((r) => r.character).join(' ')} / ${selected?.character ?? 'none'}`);
      return null;
    }
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () => root.render(createElement(Rows)));
    // The row the store selects wins over the mark of a list from before.
    await act(async () => store.select(2));
    await act(async () => fire('vosh://sessions-changed', [ORLA]));
    expect(seen.at(0)).toBe('Tolliver Orla / Tolliver');
    expect(seen.slice(-2)).toEqual(['Tolliver Orla / Orla', 'Orla / Orla']);
    await act(async () => root.unmount());
  });
});
