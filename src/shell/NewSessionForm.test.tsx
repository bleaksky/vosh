import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';
import type { OpenedSession } from '../lib/appMenu';
import type { Connection } from '../stores/session/useConnection';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';

// The New session form. The Profile row follows the address until you
// choose a profile, each pick moves the new session to it, Connect
// waits for that move and then dials, and a form that goes any other
// way closes the session it opened. newSession.test.ts holds how the
// row opens.

/** Every step the form took, in order, the dial among them. */
const steps = vi.hoisted(() => [] as unknown[]);

const sessions = vi.hoisted(() => ({ rows: [] as SessionRow[], selected: 2 }));

/** Holds the next profile switch until the test lets it land. */
const gate = vi.hoisted(() => ({ hold: false, release: (): void => undefined }));

const PLAY = 'play.theforsakenlands.com';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    steps.push([cmd, args]);
    if (cmd === 'profiles_list') {
      return Promise.resolve({
        active: 'default',
        profiles: [
          { name: 'default', auto_match: { host: PLAY, port: 1848, characters: ['Tolliver'] } },
          { name: 'Build', auto_match: { host: PLAY, port: 1825, characters: ['Orla'] } },
          { name: 'Healer', auto_match: { host: PLAY, port: null, characters: ['Maren'] } },
        ],
      });
    }
    if (cmd === 'profile_resolve_match') {
      // Rust's pick before login for the claims above.
      return Promise.resolve(args?.port === 1825 ? 'Build' : 'default');
    }
    if (cmd === 'profile_switch' && gate.hold) {
      gate.hold = false;
      return new Promise<void>((resolve) => {
        gate.release = resolve;
      });
    }
    return Promise.resolve();
  },
}));

vi.mock('@tauri-apps/api/event', () => ({
  emit: () => Promise.resolve(),
  listen: () => Promise.resolve(() => {}),
}));

vi.mock('../stores/session/sessionsStore', () => ({
  useSessions: () => sessions.rows,
  getSessions: () => sessions.rows,
  getSelected: () => sessions.selected,
  select: (id: number) => {
    steps.push(['select', id]);
    sessions.selected = id;
    return Promise.resolve();
  },
}));

function row(id: number, character: string | null, port: number | null): SessionRow {
  return {
    id,
    name: null,
    character,
    host: port === null ? null : PLAY,
    port,
    tls: false,
    profile: 'default',
    connected: port !== null,
    since: null,
    selected: id === sessions.selected,
  };
}

const opened: OpenedSession = { id: 2, previous: 1, front: 'default', profile: 'default' };

type Handler = (e?: unknown) => void;

/** The handlers React keeps on an element. This DOM sends no events. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

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
    // The faked clock, read as each call lands.
    setTimeout: (cb: () => void, ms: number) => setTimeout(cb, ms),
    clearTimeout: (id: number) => clearTimeout(id),
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  vi.stubGlobal('localStorage', {
    getItem: () => JSON.stringify({ host: PLAY, port: 1848, tls: false }),
    setItem() {},
    removeItem() {},
  });
  // The port field puts its caret at the end as it opens.
  (FakeElement.prototype as unknown as Record<string, unknown>).setSelectionRange = () => undefined;
  ({ createRoot } = await import('react-dom/client'));
});

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  steps.length = 0;
  sessions.selected = 2;
  sessions.rows = [row(1, 'Tolliver', 1848), row(2, null, null)];
  gate.hold = false;
});

afterEach(() => {
  vi.useRealTimers();
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const run = (fn: () => void | Promise<void>) =>
  act(async () => {
    await fn();
  });

async function mount() {
  const { NewSessionForm } = await import('./NewSessionForm');
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  const connectNew = vi.fn((target: unknown, session: number) => {
    steps.push(['dial', target, session]);
    return Promise.resolve();
  });
  const connection = { connectNew } as unknown as Connection;
  const onClose = vi.fn();
  await run(() => root.render(createElement(NewSessionForm, { opened, connection, onClose })));
  const port = findAll(container, (el) => el.getAttribute('placeholder') === '4000')[0];
  const form = findAll(container, (el) => el.nodeName === 'FORM')[0];
  const profile = () => findAll(container, (el) => el.nodeName === 'SELECT')[0];
  /** The profile the row shows, as React holds the select's value. */
  const picked = () => (on(profile()) as unknown as { value: string }).value;
  return {
    profile,
    picked,
    onClose,
    connectNew,
    /** Type `text` in Port. */
    editPort: (text: string) => run(() => on(port).onChange({ target: { value: text } })),
    /** Type `text` in Port and let the pick's rest run out. */
    typePort: async (text: string) => {
      await run(() => on(port).onChange({ target: { value: text } }));
      await run(async () => {
        await vi.advanceTimersByTimeAsync(300);
      });
    },
    choose: (name: string) => run(() => on(profile()).onChange({ target: { value: name } })),
    connect: () => run(() => on(form).onSubmit({ preventDefault() {} })),
    unmount: () => run(() => root.unmount()),
  };
}

/** The steps that are the given command, with their arguments. */
const calls = (cmd: string) => named(cmd).map((step) => step[1]);

/** The steps that are the given command, whole. */
const named = (cmd: string) =>
  steps.filter((step): step is unknown[] => Array.isArray(step) && step[0] === cmd);

describe('the Profile row of the New session form', () => {
  it('picks Build once you type the build port', async () => {
    const m = await mount();
    await m.typePort('1825');
    expect(calls('profile_resolve_match')).toContainEqual({
      host: PLAY,
      port: 1825,
      character: null,
      anyCharacter: true,
    });
    expect(calls('profile_switch')).toEqual([{ name: 'Build', session: 2 }]);
    expect(m.picked()).toBe('Build');
    await m.unmount();
  });

  it('keeps the profile you chose as the address changes', async () => {
    const m = await mount();
    await m.choose('Healer');
    expect(calls('profile_switch')).toEqual([{ name: 'Healer', session: 2 }]);
    steps.length = 0;
    await m.typePort('1825');
    expect(calls('profile_resolve_match')).toEqual([]);
    expect(calls('profile_switch')).toEqual([]);
    expect(m.picked()).toBe('Healer');
    await m.unmount();
  });

  it('dials only once the switch to the picked profile has landed', async () => {
    const m = await mount();
    // Connect before the pick's rest runs out, with the switch held.
    await m.editPort('1825');
    gate.hold = true;
    await run(() => {
      void m.connect();
    });
    expect(calls('profile_switch')).toEqual([{ name: 'Build', session: 2 }]);
    expect(calls('dial')).toEqual([]);
    await run(() => gate.release());
    expect(named('dial')).toEqual([['dial', { host: PLAY, port: 1825, tls: false }, 2]]);
    expect(m.onClose).toHaveBeenCalled();
    // A form that dialed closes nothing as it goes.
    steps.length = 0;
    await m.unmount();
    expect(calls('session_close')).toEqual([]);
  });
});

describe('a New session form that goes without dialing', () => {
  it('brings the session you came from back and closes the one it opened', async () => {
    const m = await mount();
    steps.length = 0;
    await m.unmount();
    expect(steps).toEqual([
      ['select', 1],
      ['session_close', { session: 2 }],
    ]);
  });
});
