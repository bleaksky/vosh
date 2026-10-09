import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../../ipc/session';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../../test/fakeDom';

// Reconnect when the link drops, under General, then Connection. The
// row reads and saves the profile Settings shows, which follows the
// session list through a fake Tauri event bus. Each test loads fresh
// modules, since the stores keep the list at module scope.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
let rows: SessionRow[] = [];
const saved = new Map<string, boolean>();
let failSet = false;
const calls: { cmd: string; args: Record<string, unknown> | undefined }[] = [];

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
  invoke: async (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args });
    if (cmd === 'sessions_list') return rows;
    if (cmd === 'reconnect_get') return saved.get(String(args?.profile)) ?? true;
    if (cmd === 'reconnect_set') {
      if (failSet) throw new Error('the profile file is read only');
      saved.set(String(args?.profile), Boolean(args?.on));
      return null;
    }
    return null;
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
  since: 1_000,
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
  saved.clear();
  calls.length = 0;
  failSet = false;
});

/** Send every window the rows, as the app does after a step. */
async function send(list: SessionRow[]): Promise<void> {
  await act(async () => {
    for (const cb of handlers.get('vosh://sessions-changed') ?? []) cb({ payload: list });
    await settle();
  });
}

/** Draw the row over `list` and hand back how to read and flip it. */
async function draw(list: SessionRow[]) {
  rows = list;
  const { ReconnectRow } = await import('./ReconnectRow');
  const { ShownSession } = await import('../ShownSession');
  const errors: (string | null)[] = [];
  const onError = (message: string | null) => errors.push(message);
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    // The header starts Settings following the session list.
    root.render(
      createElement(
        'div',
        null,
        createElement(ShownSession),
        createElement(ReconnectRow, { onError }),
      ),
    );
    await settle();
    await settle();
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
  });
  const input = () =>
    findAll(container, (el) => el.getAttribute('role') === 'switch')[0] as FakeElement & {
      checked?: boolean;
    };
  return {
    errors,
    label: () =>
      findAll(container, (el) => el.getAttribute('class') === 'st-row-label')[0]?.textContent,
    on: () => {
      const el = input();
      const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
      return (el as unknown as Record<string, { checked: boolean }>)[key].checked;
    },
    flip: async (next: boolean) => {
      const el = input();
      const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
      const props = (el as unknown as Record<string, { onChange: (e: unknown) => void }>)[key];
      await act(async () => {
        props.onChange({ target: { checked: next } });
        await settle();
      });
    },
  };
}

const named = (cmd: string) => calls.filter((c) => c.cmd === cmd).map((c) => c.args);

describe('Reconnect when the link drops', () => {
  it('reads the profile Settings shows', async () => {
    saved.set('Build', false);
    const page = await draw([TOLLIVER, { ...ORLA, selected: true }]);
    expect(page.label()).toBe('Reconnect when the link drops');
    expect(named('reconnect_get').at(-1)).toEqual({ profile: 'Build' });
    expect(page.on()).toBe(false);
  });

  it('reads again when Settings moves to another profile', async () => {
    saved.set('Build', false);
    const page = await draw([{ ...TOLLIVER, selected: true }, ORLA]);
    expect(page.on()).toBe(true);
    await send([TOLLIVER, { ...ORLA, selected: true }]);
    expect(named('reconnect_get').at(-1)).toEqual({ profile: 'Build' });
    expect(page.on()).toBe(false);
  });

  it('saves a flip at once on the profile it shows', async () => {
    const page = await draw([{ ...TOLLIVER, selected: true }]);
    await page.flip(false);
    expect(named('reconnect_set')).toEqual([{ on: false, profile: 'default' }]);
    expect(page.on()).toBe(false);
    expect(page.errors.at(-1)).toBeNull();
  });

  it('goes back and says why when the save fails', async () => {
    const page = await draw([{ ...TOLLIVER, selected: true }]);
    failSet = true;
    await page.flip(false);
    expect(page.on()).toBe(true);
    expect(page.errors.at(-1)).toBe('Error: the profile file is read only');
  });
});
