import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SnoopSnapshot, SnoopTab } from '../../ipc/snoop';
import type { Snoops } from './snoopStore';

// Drives the snoop store through a fake Tauri event bus with two
// sessions, Staff (1) snooping and Builder (2). Each test loads fresh
// store modules.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const commands = new Map<string, (args: { session?: number }) => unknown>();

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
  invoke: async (cmd: string, args: { session?: number } = {}) => {
    const answer = commands.get(cmd);
    if (!answer) throw new Error(`no fake for ${cmd}`);
    return answer(args);
  },
}));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const STAFF = 1;
const BUILDER = 2;

const select = (selected: number) =>
  fire(
    'vosh://sessions-changed',
    [STAFF, BUILDER].map((id) => ({
      id,
      name: id === STAFF ? 'Staff' : 'Builder',
      character: null,
      host: 'play.theforsakenlands.com',
      port: id === STAFF ? 1848 : 4000,
      tls: false,
      profile: 'Staff',
      connected: true,
      selected: id === selected,
    })),
  );

const live = (name: string, last: number | null = null): SnoopTab => ({
  name,
  live: true,
  ended_at: null,
  last_output_at: last,
});
const ended = (name: string, at: number): SnoopTab => ({
  name,
  live: false,
  ended_at: at,
  last_output_at: null,
});

const list = (session: number, tabs: SnoopTab[], windowed = false) =>
  fire('session://snoop', { session, tabs, windowed });
const output = (session: number, name: string, text: string) =>
  fire('session://snoop-output', { session, name, text });

const EMPTY: SnoopSnapshot = { tabs: [], windowed: false };

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  commands.clear();
  vi.stubGlobal('window', globalThis);
  commands.set('sessions_list', () => []);
  commands.set('snoop_get', () => EMPTY);
});

async function load() {
  const sessions = await import('./sessionsStore');
  sessions.startSessionsStore();
  const store = await import('./snoopStore');
  store.startSnoopStore();
  await settle();
  select(STAFF);
  return store;
}

const names = (store: typeof import('./snoopStore')) =>
  store.getSnoops().tabs.map((tab) => tab.name);

describe('the snoop store', () => {
  it('puts a snoop that starts in front', async () => {
    const store = await load();
    list(STAFF, [live('Tolliver')]);
    expect(store.getSnoops().selected).toBe('Tolliver');
    list(STAFF, [live('Tolliver'), live('Orla')]);
    expect(names(store)).toEqual(['Tolliver', 'Orla']);
    expect(store.getSnoops().selected).toBe('Orla');
  });

  it('picks the tab back up when you snoop the same player again', async () => {
    const store = await load();
    list(STAFF, [live('Tolliver'), live('Orla')]);
    list(STAFF, [ended('Tolliver', 5), live('Orla')]);
    expect(store.getSnoops().selected).toBe('Orla');
    list(STAFF, [live('Tolliver'), live('Orla')]);
    expect(names(store)).toEqual(['Tolliver', 'Orla']);
    expect(store.getSnoops().selected).toBe('Tolliver');
  });

  it('hands the front to the next tab once the game confirms your Stop', async () => {
    const store = await load();
    list(STAFF, [live('Tolliver'), live('Maren'), live('Orla')]);
    store.selectSnoop('Maren');
    // Stop waits for the game, so the tab reads live meanwhile.
    list(STAFF, [live('Tolliver'), live('Maren'), live('Orla')]);
    expect(store.getSnoops().selected).toBe('Maren');
    list(STAFF, [live('Tolliver'), live('Orla')]);
    expect(store.getSnoops().selected).toBe('Orla');
    list(STAFF, [live('Tolliver')]);
    expect(store.getSnoops().selected).toBe('Tolliver');
    list(STAFF, []);
    expect(store.getSnoops().selected).toBeNull();
  });

  it('keeps a snoop that ended as ended, in front where it was', async () => {
    const store = await load();
    list(STAFF, [live('Tolliver'), live('Maren')]);
    list(STAFF, [live('Tolliver'), ended('Maren', 1_000)]);
    const { tabs, selected } = store.getSnoops();
    expect(selected).toBe('Maren');
    expect(tabs[1]).toEqual(ended('Maren', 1_000));
  });

  it('marks lines for a tab behind, or any tab while folded, as unread', async () => {
    const store = await load();
    list(STAFF, [live('Tolliver'), live('Maren')]);
    output(STAFF, 'Maren', 'The day has begun.\n\r');
    expect([...store.getSnoops().unread]).toEqual([]);
    output(STAFF, 'Tolliver', 'The day has begun.\n\r');
    expect([...store.getSnoops().unread]).toEqual(['Tolliver']);
    store.selectSnoop('Tolliver');
    expect([...store.getSnoops().unread]).toEqual([]);

    store.setSnoopsFolded(true);
    output(STAFF, 'Tolliver', '<788hp 315m 540mv> ');
    expect([...store.getSnoops().unread]).toEqual(['Tolliver']);
    list(STAFF, [live('Tolliver', 7), live('Maren')]);
    expect([...store.getSnoops().unread]).toEqual(['Tolliver']);
    store.setSnoopsFolded(false);
    expect([...store.getSnoops().unread]).toEqual([]);

    output(STAFF, 'Maren', '<1020hp 800m 930mv> ');
    list(STAFF, [live('Tolliver')]);
    expect([...store.getSnoops().unread]).toEqual([]);
  });

  it('hands the text to its subscribers and keeps it out of the state', async () => {
    const store = await load();
    const heard: unknown[] = [];
    store.subscribeSnoopOutput((...args) => heard.push(args));
    list(STAFF, [live('Tolliver')]);
    const before = store.getSnoops();
    output(STAFF, 'Tolliver', 'A rocky mountain path\n\r');
    output(STAFF, 'Tolliver', '[Exits: north west]\n\r');
    expect(store.getSnoops()).toBe(before);
    expect(heard).toEqual([
      [STAFF, 'Tolliver', 'A rocky mountain path\n\r', false],
      [STAFF, 'Tolliver', '[Exits: north west]\n\r', false],
    ]);
  });

  it('keeps the snoops of each session apart and counts the live ones', async () => {
    const store = await load();
    list(STAFF, [live('Tolliver'), live('Orla'), ended('Maren', 5)]);
    list(BUILDER, [live('Maren')]);
    expect(names(store)).toEqual(['Tolliver', 'Orla', 'Maren']);
    select(BUILDER);
    expect(names(store)).toEqual(['Maren']);
    expect(store.getSnoops().selected).toBe('Maren');
    expect(store.liveSnoops(STAFF)).toBe(2);
    expect(store.liveSnoops(BUILDER)).toBe(1);
  });

  it('fills a session it shows first from the snapshot, text and all', async () => {
    commands.set('snoop_get', ({ session }) =>
      session === BUILDER
        ? {
            tabs: [{ ...live('Orla', 9), text: 'The Central Square of Val Miran\n\r' }],
            windowed: true,
          }
        : EMPTY,
    );
    const store = await load();
    const heard: unknown[] = [];
    store.subscribeSnoopOutput((...args) => heard.push(args));
    select(BUILDER);
    await settle();
    expect(store.getSnoops()).toMatchObject({
      tabs: [live('Orla', 9)],
      windowed: true,
      selected: 'Orla',
    });
    expect(heard).toEqual([[BUILDER, 'Orla', 'The Central Square of Val Miran\n\r', true]]);
  });

  it('keeps the text of each tab for a terminal that mounts later', async () => {
    const store = await load();
    list(STAFF, [live('Tolliver'), live('Orla')]);
    output(STAFF, 'Tolliver', 'A rocky mountain path\n\r');
    output(STAFF, 'Tolliver', '<788hp 315m 540mv> ');
    output(STAFF, 'Orla', 'The day has begun.\n\r');
    expect(store.snoopText(STAFF, 'Tolliver')).toBe('A rocky mountain path\n\r<788hp 315m 540mv> ');
    expect(store.snoopText(STAFF, 'Orla')).toBe('The day has begun.\n\r');
    expect(store.snoopText(BUILDER, 'Tolliver')).toBe('');

    // A tab that goes takes its text along.
    list(STAFF, [live('Tolliver')]);
    expect(store.snoopText(STAFF, 'Orla')).toBe('');
    expect(store.snoopText(STAFF, 'Tolliver')).not.toBe('');
  });

  it('keeps the newest 5,000 lines of a tab, as the backend does', async () => {
    const store = await load();
    list(STAFF, [live('Tolliver')]);
    const lines = (from: number, to: number) =>
      Array.from({ length: to - from }, (_, i) => `${from + i}\n\r`).join('');
    output(STAFF, 'Tolliver', lines(0, store.SNOOP_LINES));
    output(STAFF, 'Tolliver', lines(store.SNOOP_LINES, store.SNOOP_LINES + 600));
    const kept = store.snoopText(STAFF, 'Tolliver');
    expect(kept.match(/\n/g)?.length).toBe(store.SNOOP_LINES);
    expect(kept.startsWith('600\n\r')).toBe(true);
    expect(kept.endsWith(`${store.SNOOP_LINES + 599}\n\r`)).toBe(true);
  });

  it('takes a snapshot in place of the text it kept', async () => {
    commands.set('snoop_get', ({ session }) =>
      session === BUILDER
        ? { tabs: [{ ...live('Orla', 9), text: 'A rocky mountain path\n\r' }], windowed: false }
        : EMPTY,
    );
    const store = await load();
    select(BUILDER);
    await settle();
    expect(store.snoopText(BUILDER, 'Orla')).toBe('A rocky mountain path\n\r');
  });

  it('reads a session the snoop window shows whatever is selected', async () => {
    commands.set('snoop_get', ({ session }) =>
      session === BUILDER
        ? {
            tabs: [
              { ...live('Maren', 9), text: 'A rocky mountain path\n\r' },
              { ...live('Orla', 9), text: '' },
            ],
            windowed: true,
          }
        : EMPTY,
    );
    const store = await load();
    const { FakeDocument, FakeElement, FakeNode } = await import('../../test/fakeDom');
    const doc = new FakeDocument();
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    vi.stubGlobal('HTMLIFrameElement', class {});
    // Node 20 has no navigator, and React DOM reads it.
    vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
    const { act, createElement } = await import('react');
    const { createRoot } = await import('react-dom/client');
    let seen: Snoops | null = null;
    const Reader = () => {
      seen = store.useSnoopsOf(BUILDER);
      return null;
    };
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () => root.render(createElement(Reader)));
    await act(settle);
    expect(seen).toMatchObject({ windowed: true, selected: 'Orla' });
    expect(store.snoopText(BUILDER, 'Maren')).toBe('A rocky mountain path\n\r');
    // The session in front keeps its own.
    expect(store.getSnoops().tabs).toEqual([]);

    await act(async () => store.selectSnoop('Maren', BUILDER));
    expect(seen).toMatchObject({ selected: 'Maren' });
    act(() => root.unmount());
    vi.unstubAllGlobals();
  });
});
