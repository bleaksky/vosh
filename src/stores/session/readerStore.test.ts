import { beforeEach, describe, expect, it, vi } from 'vitest';
import { foldReader, READER_LINES, type ReaderState } from './readerStore';

// Drives the reader store through a fake Tauri event bus with two
// sessions, Tolliver's (1) and Orla's (2). Each test loads fresh store
// modules, since they keep their state at module scope.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();

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
    if (cmd === 'sessions_list') return [];
    throw new Error(`no fake for ${cmd}`);
  },
}));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const TOLLIVER = 1;
const ORLA = 2;

const select = (selected: number) =>
  fire(
    'vosh://sessions-changed',
    [TOLLIVER, ORLA].map((id) => ({
      id,
      name: null,
      character: id === TOLLIVER ? 'Tolliver' : 'Orla',
      host: 'play.theforsakenlands.com',
      port: 1848,
      tls: false,
      profile: id === TOLLIVER ? 'Tolliver' : 'Orla',
      connected: true,
      selected: id === selected,
    })),
  );

const PROMPT = '<512/512hp 300/300m 410/410mv>';

/** One read as the session sends it. */
const feed = (lines: string[], prompt: string | null = null) => ({
  lines,
  count: lines.length,
  prompt,
  away: false,
});

const read = (session: number, lines: string[], prompt: string | null = null) =>
  fire('session://screen-reader', { session, ...feed(lines, prompt) });

async function load() {
  const store = await import('./readerStore');
  store.startReaderStore();
  await settle();
  select(TOLLIVER);
  return store;
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

describe('foldReader', () => {
  const empty: ReaderState = { lines: [], total: 0, prompt: null };

  it('keeps the newest 500 lines, newest last, however many come', () => {
    const many = Array.from({ length: 1_200 }, (_, i) => `line ${i}`);
    let state = empty;
    for (let i = 0; i < many.length; i += 300)
      state = foldReader(state, feed(many.slice(i, i + 300)));
    expect(state.lines).toHaveLength(READER_LINES);
    expect(state.lines[0]).toBe('line 700');
    expect(state.lines.at(-1)).toBe('line 1199');
    expect(state.total).toBe(1_200);
  });

  it('counts the lines a read dropped before it sent the rest', () => {
    const state = foldReader(empty, { ...feed(['You are thirsty.']), count: 640 });
    expect(state.lines).toEqual(['You are thirsty.']);
    expect(state.total).toBe(640);
  });

  it('replaces the prompt only when a read brings one', () => {
    const once = foldReader(empty, feed(['You are hungry.'], PROMPT));
    expect(once.prompt).toBe(PROMPT);
    const later = foldReader(once, feed(['You are thirsty.']));
    expect(later.prompt).toBe(PROMPT);
    const hurt = '<380/512hp 300/300m 410/410mv>';
    expect(foldReader(later, feed([], hurt))).toEqual({
      lines: ['You are hungry.', 'You are thirsty.'],
      total: 2,
      prompt: hurt,
    });
  });

  it('hands back the same state for a read with nothing in it', () => {
    const state = foldReader(empty, feed(['The day has begun.']));
    expect(foldReader(state, feed([]))).toBe(state);
  });
});

describe('readerStore', () => {
  it('starts with nothing read', async () => {
    const store = await load();
    expect(store.getReader()).toEqual({ lines: [], total: 0, prompt: null });
  });

  it('keeps each session apart and shows the selected one', async () => {
    const store = await load();
    read(TOLLIVER, ['The day has begun.'], PROMPT);
    read(ORLA, ['You are hungry.', 'You are thirsty.']);
    expect(store.getReader()).toEqual({
      lines: ['The day has begun.'],
      total: 1,
      prompt: PROMPT,
    });
    select(ORLA);
    expect(store.getReader()).toEqual({
      lines: ['You are hungry.', 'You are thirsty.'],
      total: 2,
      prompt: null,
    });
  });

  it('keeps what was read through a disconnect', async () => {
    const store = await load();
    read(TOLLIVER, ['The sun rises in the east.'], PROMPT);
    const before = store.getReader();
    fire('session://state', { session: TOLLIVER, kind: 'disconnected', reason: null });
    expect(store.getReader()).toBe(before);
  });
});
