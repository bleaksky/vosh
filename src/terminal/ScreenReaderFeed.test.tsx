import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import looks from '../../fixtures/room-colors/looks.json';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';

// The hidden game log and the pulse announcer (R21 and R25 review,
// board 13, Q19 to Q21), mounted with React DOM over a fake DOM. The
// reader's choices, the sessions and each read come through a fake
// Tauri event bus, and each test loads fresh modules, since the stores
// keep their state at module scope. Two sessions, Tolliver's (1) in
// front and Orla's (2) behind.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
let config: Record<string, unknown> = {};

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
  invoke: async (cmd: string) => {
    if (cmd === 'ui_get_config') return { tracked_affects: [], ...config };
    if (cmd === 'sessions_list') return [];
    throw new Error(`no fake for ${cmd}`);
  },
}));

const TOLLIVER = 1;
const ORLA = 2;

/** The lines of the room looks fixture as they show, plain text with
 *  the blank lines left out, as the session sends them. */
const SHOWN = (looks.cases as { events: { line?: unknown }[] }[])
  .flatMap((c) => c.events)
  .flatMap((e) => (typeof e.line === 'string' ? [e.line] : []))
  // eslint-disable-next-line no-control-regex
  .map((line) => line.replace(/\x1b\[[0-9;]*m/g, '').trimEnd())
  .filter((line) => line.trim().length > 0);

const PROMPT = '<1020hp 800m 930mv>';

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
  vi.useRealTimers();
});

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  config = {};
});

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Bring `selected` to the front, as the app sends the list. */
const select = (selected: number) =>
  act(async () => {
    fire(
      'vosh://sessions-changed',
      [TOLLIVER, ORLA].map((id) => ({
        id,
        name: null,
        character: id === TOLLIVER ? 'Tolliver' : 'Orla',
        host: 'play.theforsakenlands.com',
        port: 1848,
        tls: false,
        profile: 'Default',
        connected: true,
        since: null,
        selected: id === selected,
      })),
    );
  });

/** Mount the feed with the reader choices `choices`, Tolliver in front,
 *  and hand back how to drive it and read it. Timers are fake from
 *  here on. */
async function mount(choices: Record<string, unknown>) {
  config = choices;
  const stores = {
    sessions: await import('../stores/session/sessionsStore'),
    reader: await import('../stores/session/readerStore'),
    choices: await import('../stores/config/screenReaderStore'),
  };
  stores.sessions.startSessionsStore();
  stores.reader.startReaderStore();
  stores.choices.startScreenReaderStore();
  const { ScreenReaderFeed } = await import('./ScreenReaderFeed');
  const { readPrompt } = await import('./readerVoice');
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(createElement(ScreenReaderFeed));
    await settle();
  });
  await select(TOLLIVER);
  cleanups.push(async () => {
    await act(async () => root.unmount());
  });
  vi.useFakeTimers();
  const region = () => findAll(container, (el) => el.getAttribute('aria-live') === 'polite')[0];
  return {
    container,
    /** One read of `session`, as the session sends it. */
    read: (
      session: number,
      lines: string[],
      { prompt = null, away = false }: { prompt?: string | null; away?: boolean } = {},
    ) =>
      act(async () => {
        fire('session://screen-reader', { session, lines, count: lines.length, prompt, away });
      }),
    /** Let `ms` pass. */
    wait: (ms: number) =>
      act(async () => {
        vi.advanceTimersByTime(ms);
      }),
    /** The announcement node in the live region, or undefined. */
    said: () => region()?.childNodes[0] as FakeElement | undefined,
    /** Each part of the announcement. */
    parts: () =>
      findAll(region() ?? container, (el) => el.tagName === 'P').map((p) => p.textContent),
    /** The hidden log, or undefined. */
    log: () => findAll(container, (el) => el.getAttribute('role') === 'log')[0],
    /** The lines the log holds. */
    lines: () => findAll(container, (el) => el.tagName === 'LI').map((li) => li.textContent),
    select,
    readPrompt: () =>
      act(async () => {
        readPrompt();
      }),
  };
}

const ON = { screen_reader: true };

describe('the hidden game log', () => {
  it('renders nothing while the reader is off', async () => {
    const feed = await mount({});
    await feed.read(TOLLIVER, SHOWN.slice(0, 2));
    await feed.wait(250);
    expect(feed.container.childNodes).toEqual([]);
  });

  it('is a log named Game lines that never speaks by itself', async () => {
    const feed = await mount(ON);
    const log = feed.log();
    expect(log?.tagName).toBe('OL');
    expect(log?.getAttribute('aria-label')).toBe('Game lines');
    expect(log?.getAttribute('aria-live')).toBe('off');
    expect(log?.getAttribute('class')).toBe('visually-hidden');
  });

  it('keeps the last 500 lines of the selected session', async () => {
    const feed = await mount(ON);
    const many = Array.from({ length: 600 }, (_, i) => SHOWN[i % SHOWN.length]);
    await feed.read(TOLLIVER, many.slice(0, 300));
    await feed.read(TOLLIVER, many.slice(300));
    expect(feed.lines()).toHaveLength(500);
    expect(feed.lines()).toEqual(many.slice(100));
  });
});

describe('the announcer', () => {
  it('joins the lines of one pulse into one announcement 250 ms from the first', async () => {
    const feed = await mount(ON);
    await feed.read(TOLLIVER, SHOWN.slice(0, 2));
    await feed.wait(200);
    await feed.read(TOLLIVER, SHOWN.slice(2, 4));
    expect(feed.said()).toBeUndefined();
    await feed.wait(50);
    expect(feed.parts()).toEqual(SHOWN.slice(0, 4));
    await feed.read(TOLLIVER, SHOWN.slice(4, 5));
    await feed.wait(250);
    expect(feed.parts()).toEqual(SHOWN.slice(4, 5));
  });

  it('ends the pulse early when a read brings your prompt', async () => {
    const feed = await mount(ON);
    await feed.read(TOLLIVER, SHOWN.slice(0, 2));
    await feed.read(TOLLIVER, SHOWN.slice(2, 3), { prompt: PROMPT });
    expect(feed.parts()).toEqual(SHOWN.slice(0, 3));
  });

  it('reads 9 lines as how many came and the last of them', async () => {
    const feed = await mount(ON);
    await feed.read(TOLLIVER, SHOWN.slice(0, 9));
    await feed.wait(250);
    expect(feed.parts()).toEqual(['9 lines.', SHOWN[8]]);
  });

  it('reads 12 lines whole with a burst of 16', async () => {
    const feed = await mount({ ...ON, screen_reader_burst: 16 });
    await feed.read(TOLLIVER, SHOWN.slice(0, 12));
    await feed.wait(250);
    expect(feed.parts()).toEqual(SHOWN.slice(0, 12));
  });

  it('stays quiet in the background but keeps the lines', async () => {
    const feed = await mount(ON);
    await feed.read(TOLLIVER, SHOWN.slice(0, 2), { away: true });
    await feed.wait(250);
    expect(feed.said()).toBeUndefined();
    expect(feed.lines()).toEqual(SHOWN.slice(0, 2));
  });

  it('reads in the background with Read in the background on', async () => {
    const feed = await mount({ ...ON, screen_reader_background: true });
    await feed.read(TOLLIVER, SHOWN.slice(0, 2), { away: true });
    await feed.wait(250);
    expect(feed.parts()).toEqual(SHOWN.slice(0, 2));
  });

  it('adds the prompt only with Read your prompt on', async () => {
    const off = await mount(ON);
    await off.read(TOLLIVER, SHOWN.slice(0, 1), { prompt: PROMPT });
    expect(off.parts()).toEqual(SHOWN.slice(0, 1));
    await off.read(TOLLIVER, [], { prompt: PROMPT });
    expect(off.parts()).toEqual(SHOWN.slice(0, 1));

    vi.useRealTimers();
    vi.resetModules();
    handlers.clear();
    const on = await mount({ ...ON, screen_reader_prompt: true });
    await on.read(TOLLIVER, SHOWN.slice(0, 1), { prompt: PROMPT });
    expect(on.parts()).toEqual([SHOWN[0], PROMPT]);
  });

  it('speaks only for the selected session', async () => {
    const feed = await mount(ON);
    await feed.read(ORLA, SHOWN.slice(0, 2));
    await feed.wait(250);
    expect(feed.said()).toBeUndefined();
    expect(feed.lines()).toEqual([]);
    await feed.select(ORLA);
    expect(feed.lines()).toEqual(SHOWN.slice(0, 2));
  });

  it('drops a pulse when another session comes to the front', async () => {
    const feed = await mount(ON);
    await feed.read(TOLLIVER, SHOWN.slice(0, 2));
    await feed.select(ORLA);
    await feed.wait(250);
    expect(feed.said()).toBeUndefined();
  });

  it('says the same words twice in a fresh node', async () => {
    const feed = await mount(ON);
    await feed.read(TOLLIVER, SHOWN.slice(0, 1), { prompt: PROMPT });
    const first = feed.said();
    await feed.read(TOLLIVER, SHOWN.slice(0, 1), { prompt: PROMPT });
    expect(feed.parts()).toEqual(SHOWN.slice(0, 1));
    expect(feed.said()).not.toBe(first);
  });
});

describe('the prompt key', () => {
  it('says when Vosh has not seen your prompt yet', async () => {
    const feed = await mount(ON);
    await feed.readPrompt();
    expect(feed.parts()).toEqual(['Vosh has not seen your prompt yet.']);
  });

  it('reads the latest prompt of the selected session', async () => {
    const feed = await mount(ON);
    await feed.read(TOLLIVER, [], { prompt: '<612hp 480m 702mv>' });
    await feed.read(TOLLIVER, SHOWN.slice(0, 1), { prompt: PROMPT });
    await feed.readPrompt();
    expect(feed.parts()).toEqual([PROMPT]);
  });
});
