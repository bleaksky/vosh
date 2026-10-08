import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';

// The session and profile Settings names at the right of its header,
// board 7 and board 9 of the Sessions review. The sessions come through
// a fake Tauri event bus, and each test loads fresh modules, since the
// stores keep the list at module scope.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
let rows: SessionRow[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

/** The profile each profile_hold_edits call named. */
const holdCalls: (string | null)[] = [];

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args?: { profile?: string | null }) => {
    if (cmd === 'profile_hold_edits') holdCalls.push(args?.profile ?? null);
    return cmd === 'sessions_list' ? rows : null;
  },
}));

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const row = (id: number, patch: Partial<SessionRow>): SessionRow => ({
  id,
  name: null,
  character: 'Tolliver',
  host: 'play.theforsakenlands.com',
  port: 1848,
  tls: false,
  profile: 'default',
  connected: true,
  since: null,
  selected: false,
  ...patch,
});

const TOLLIVER = row(1, {});
const ORLA = row(2, { character: 'Orla', port: 1825, profile: 'Build' });

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
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
});

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  holdCalls.length = 0;
});

/** Send every window the rows, as the app does after a step. */
async function send(list: SessionRow[]): Promise<void> {
  await act(async () => {
    for (const cb of handlers.get('vosh://sessions-changed') ?? []) cb({ payload: list });
  });
}

/** Draw the header over `list`, beside a page that holds unsaved edits
 *  while `dirty` says so, and hand back how to read it. */
async function header(list: SessionRow[], dirty = false) {
  rows = list;
  const { ShownSession } = await import('./ShownSession');
  const { useProfileHold } = await import('./shownProfile');
  function Page({ held }: { held: boolean }) {
    useProfileHold(held);
    return null;
  }
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  const draw = (held: boolean) =>
    act(async () => {
      root.render(
        createElement('div', null, createElement(ShownSession), createElement(Page, { held })),
      );
      await settle();
    });
  await draw(dirty);
  cleanups.push(async () => {
    await act(async () => root.unmount());
  });
  const classOf = (el: FakeElement) => el.getAttribute('class') ?? '';
  return {
    /** Each part as its class and its words. */
    parts: () =>
      findAll(container, (el) => classOf(el).startsWith('st-who-')).map((el) => [
        classOf(el),
        el.textContent,
      ]),
    dot: () => findAll(container, (el) => classOf(el).startsWith('dot')).map(classOf)[0],
    /** Save or discard, which leaves the page clean. */
    letGo: () => draw(false),
  };
}

describe('the Settings header', () => {
  it('stays away with one session', async () => {
    const shown = await header([{ ...TOLLIVER, selected: true }]);
    expect(shown.parts()).toEqual([]);
    expect(shown.dot()).toBeUndefined();
  });

  it('names the selected session and its profile', async () => {
    const shown = await header([TOLLIVER, { ...ORLA, selected: true }]);
    expect(shown.dot()).toBe('dot is-success');
    expect(shown.parts()).toEqual([
      ['st-who-name', 'Orla'],
      ['st-who-profile', 'Build'],
    ]);
  });

  it('shows Also in while another session plays the same profile', async () => {
    const builder = row(2, { name: 'Builder', port: 1825, selected: true });
    const shown = await header([TOLLIVER, builder]);
    expect(shown.parts()).toEqual([
      ['st-who-name', 'Builder'],
      ['st-who-profile', 'Default'],
      ['st-who-also', 'Also in Tolliver'],
    ]);
  });

  it('names a session before login by its world and port', async () => {
    const login = { ...ORLA, character: null, connected: false, selected: true };
    const shown = await header([TOLLIVER, login]);
    expect(shown.dot()).toBe('dot is-off');
    expect(shown.parts()[0]).toEqual(['st-who-name', 'The Forsaken Lands 1825']);
  });

  it('follows the selection to another session', async () => {
    const shown = await header([{ ...TOLLIVER, selected: true }, ORLA]);
    await send([TOLLIVER, { ...ORLA, selected: true }]);
    expect(shown.parts()[0]).toEqual(['st-who-name', 'Orla']);
  });
});

describe('a page with unsaved edits', () => {
  it('holds its profile and says so until you save or discard', async () => {
    const shown = await header([{ ...TOLLIVER, selected: true }, ORLA], true);
    await send([TOLLIVER, { ...ORLA, selected: true }]);
    expect(shown.dot()).toBe('dot is-warn');
    expect(shown.parts()).toEqual([
      ['st-who-name', 'Tolliver'],
      ['st-who-profile', 'Default'],
      ['st-who-note', 'Save or discard to follow Orla'],
    ]);
    await shown.letGo();
    expect(shown.dot()).toBe('dot is-success');
    expect(shown.parts()).toEqual([
      ['st-who-name', 'Orla'],
      ['st-who-profile', 'Build'],
    ]);
  });

  it('asks Rust to keep its profile open until you save or discard', async () => {
    const shown = await header([{ ...TOLLIVER, selected: true }, ORLA], true);
    await send([TOLLIVER, { ...ORLA, selected: true }]);
    expect(holdCalls).toEqual(['default']);
    await shown.letGo();
    expect(holdCalls).toEqual(['default', null]);
  });

  it('only renames the header for a session on the same profile', async () => {
    const builder = row(2, { name: 'Builder', port: 1825 });
    const shown = await header([{ ...TOLLIVER, selected: true }, builder], true);
    await send([TOLLIVER, { ...builder, selected: true }]);
    expect(shown.parts()).toEqual([
      ['st-who-name', 'Builder'],
      ['st-who-profile', 'Default'],
      ['st-who-also', 'Also in Tolliver'],
    ]);
  });

  it('holds through a login that switches its session to another profile', async () => {
    const shown = await header([{ ...TOLLIVER, selected: true }, ORLA], true);
    await send([{ ...TOLLIVER, profile: 'Healer', selected: true }, ORLA]);
    expect(shown.parts()).toEqual([
      ['st-who-name', 'Tolliver'],
      ['st-who-profile', 'Default'],
      ['st-who-note', 'Save or discard to follow Tolliver'],
    ]);
  });
});
