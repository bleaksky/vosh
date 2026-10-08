import { returnToCommandLine } from '../panel/paneActions';
import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';
import type { SessionRowState } from '../stores/session/sessionRowStore';
import type { Connection } from '../stores/session/useConnection';
import type { SessionLine } from './sessionLine';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';

// The session popover while the sidebar is folded, with the sidebar's
// two line rows. SESSIONS heads a list of every session, the selected
// one with the check, one behind with what waits there and any other
// with its key, and then the session rows. A click brings that session
// to the front.

const store = vi.hoisted(() => ({
  rows: [] as SessionRow[],
  selected: 1,
  goTo: vi.fn(),
  states: new Map<number, Partial<SessionRowState>>(),
}));

vi.mock('../panel/paneActions', () => ({ returnToCommandLine: vi.fn() }));
vi.mock('../stores/session/sessionsStore', async (actual) => ({
  ...(await actual<typeof import('../stores/session/sessionsStore')>()),
  useSessions: () => store.rows,
  useSelected: () => store.selected,
  goTo: store.goTo,
}));

/** The second line of each session, by id. A session it names nothing
 *  for reads its faked row state with no GMCP. */
const lines = vi.hoisted(() => new Map<number, SessionLine>());

vi.mock('./sessionLine', async (actual) => {
  const line = await actual<typeof import('./sessionLine')>();
  const { getSessionRow } = await import('../stores/session/sessionRowStore');
  return {
    ...line,
    useSessionLine: (row: SessionRow) =>
      lines.get(row.id) ??
      line.secondLine(
        row,
        { ...getSessionRow(row.id), ...store.states.get(row.id) },
        null,
        null,
        null,
        0,
      ),
  };
});

vi.mock('../stores/session/sessionRowStore', async (actual) => {
  const rows = await actual<typeof import('../stores/session/sessionRowStore')>();
  return {
    ...rows,
    useSessionRow: (id: number) => ({ ...rows.getSessionRow(id), ...store.states.get(id) }),
  };
});

const row = (id: number, fields: Partial<SessionRow>): SessionRow => ({
  id,
  name: null,
  character: null,
  host: 'play.theforsakenlands.com',
  port: 1848,
  tls: false,
  profile: 'Default',
  connected: true,
  since: null,
  selected: false,
  ...fields,
});

const connection = {
  live: true,
  target: { host: 'play.theforsakenlands.com', port: 1848, tls: false },
  connect: () => Promise.resolve(),
  disconnect: () => Promise.resolve(),
  saveTarget: () => undefined,
} as unknown as Connection;

type Handler = (e?: unknown) => void;

function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
const cleanups: (() => Promise<void>)[] = [];

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    innerWidth: 720,
    innerHeight: 450,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
  });
  vi.stubGlobal('navigator', { userAgent: 'Macintosh', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  // The menu looks for the caret and its first row as it opens.
  const el = FakeElement.prototype as unknown as Record<string, unknown>;
  el.contains = function (this: FakeNode, other: FakeNode | null): boolean {
    for (let n = other; n; n = n.parentNode) if (n === this) return true;
    return false;
  };
  el.querySelector = function (this: FakeElement): FakeElement | null {
    return findAll(this, (child) => child.getAttribute('role') === 'menuitem')[0] ?? null;
  };
  ({ createRoot } = await import('react-dom/client'));
});

afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) await cleanup();
  store.goTo.mockClear();
  store.states.clear();
  lines.clear();
});

afterAll(() => {
  vi.unstubAllGlobals();
});

async function mount(listSessions: boolean, live: Partial<Connection> = {}) {
  const { SessionMenu } = await import('./SessionMenu');
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const onClose = vi.fn();
  const onCloseSession = vi.fn();
  await act(async () => {
    root.render(
      createElement(SessionMenu, {
        connection: { ...connection, ...live },
        anchor: null,
        listSessions,
        onCloseSession,
        onClose,
      }),
    );
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const menu = findAll(doc.body, (el) => el.getAttribute('role') === 'menu')[0];
  const items = findAll(menu, (el) => el.getAttribute('role') === 'menuitem');
  return { menu, items, onClose, onCloseSession };
}

const hasClass = (name: string) => (el: FakeElement) =>
  (el.getAttribute('class') ?? '').split(' ').includes(name);

describe('the session popover with the sidebar folded', () => {
  store.rows = [
    row(1, { character: 'Tolliver', selected: true }),
    row(2, { character: 'Orla', port: 1825 }),
    row(3, { port: 1825, connected: false }),
  ];

  it('lists every session in the sidebar rows under SESSIONS before the board 4 rows, as frame 05 draws it', async () => {
    // Orla fights behind, where a tell and the fight wait.
    store.states.set(2, { link: 'live', waiting: ['preset:alert_tells', 'preset:alert_attacked'] });
    lines.set(1, { who: null, text: 'Thickening Woods', health: 100, low: false });
    lines.set(2, { who: null, text: 'Fighting a Blackwatch guard', health: 18, low: true });
    const { menu, items } = await mount(true);
    expect(findAll(menu, hasClass('shell-menu-head'))[0]?.textContent).toBe('Sessions');
    expect(items.map((el) => el.textContent)).toEqual([
      'TolliverThickening Woods100%',
      'Orla18252Fighting a Blackwatch guard18%',
      'The Forsaken Lands1825⌘3The Forsaken Lands',
      'Edit connection…',
      'Rename session…',
      'New session…⌘T',
      'Disconnect',
    ]);
    // No line between Rename session… and New session… while the list shows,
    // so five rows fit whole at 720 by 450.
    const actions = findAll(
      menu,
      (el) => hasClass('shell-menu-item')(el) || hasClass('shell-menu-sep')(el),
    ).map((el) => (hasClass('shell-menu-sep')(el) ? '|' : el.textContent));
    expect(actions).toEqual([
      '|',
      'Edit connection…',
      'Rename session…',
      'New session…⌘T',
      '|',
      'Disconnect',
    ]);
    // Every row wears its mark. The one in front wears the check, Orla
    // the count of what waits there in place of her key.
    const marks = items
      .slice(0, 3)
      .map((el) => findAll(el, hasClass('shell-sessions-mark'))[0]?.getAttribute('aria-label'));
    expect(marks).toEqual(['Playing', 'Playing', 'Not connected']);
    expect(items[0].getAttribute('aria-current')).toBe('true');
    expect(findAll(items[0], hasClass('pane-menu-check'))).toHaveLength(1);
    expect(findAll(items[1], hasClass('shell-sessions-port'))[0]?.textContent).toBe('1825');
    const count = findAll(items[1], hasClass('shell-sessions-count'))[0];
    expect(count?.getAttribute('aria-label')).toBe('2 waiting');
    expect(findAll(items[1], hasClass('shell-sessions-health'))[0]?.getAttribute('class')).toBe(
      'shell-sessions-health is-low',
    );
    expect(findAll(items[2], hasClass('shell-sessions-count'))).toHaveLength(0);
  });

  it('brings the session you pick to the front and closes', async () => {
    const { items, onClose } = await mount(true);
    await act(async () => on(items[1]).onClick());
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(store.goTo).toHaveBeenCalledWith(2);
    await act(async () => new Promise((done) => setTimeout(done, 0)));
    expect(returnToCommandLine).toHaveBeenCalledTimes(1);
  });

  // The list takes the sidebar's place while it is folded, so each row
  // closes its session as the sidebar's does, and nothing else is the
  // only way to close a session behind without bringing the sidebar back.
  it('gives every session a close button that closes it and not the one in front', async () => {
    const { menu, onClose, onCloseSession } = await mount(true);
    const closers = findAll(menu, hasClass('shell-menu-session-close'));
    expect(closers.map((el) => el.getAttribute('aria-label'))).toEqual([
      'Close session',
      'Close session',
      'Close session',
    ]);
    await act(async () => on(closers[2]).onClick());
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onCloseSession).toHaveBeenCalledWith(3);
    expect(store.goTo).not.toHaveBeenCalled();
  });

  // A long list in a short window scrolls inside the popover, which
  // stops 8 above the window's foot, so every session stays in reach.
  it('keeps the list inside the window and lets it scroll', async () => {
    const { menu } = await mount(true);
    expect(menu.style.maxHeight).toBe('calc(100vh - 16px)');
    expect(menu.getAttribute('class')).toBe('shell-menu is-listed');
    const list = findAll(menu, hasClass('shell-menu-sessions'))[0];
    expect(findAll(list, (el) => el.getAttribute('role') === 'menuitem')).toHaveLength(3);
  });

  it('lists no session while the sidebar shows', async () => {
    const { menu, items } = await mount(false);
    expect(findAll(menu, hasClass('shell-menu-head'))).toHaveLength(0);
    expect(items[0].textContent).toBe('Edit connection…');
  });
});

describe('the session popover while a redial waits', () => {
  it('offers Connect to dial now and Disconnect to end the tries', async () => {
    const { items } = await mount(false, { live: false, redialing: true });
    expect(items.map((el) => el.textContent)).toEqual([
      'Connect to The Forsaken Lands⌘R',
      'Edit connection…',
      'Rename session…',
      'New session…⌘T',
      'Disconnect',
    ]);
  });
});
