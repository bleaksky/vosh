import { beforeEach, describe, expect, it, vi } from 'vitest';

// What makes the window read where your prompt shows, through a fake
// Tauri event bus with two sessions, Tolliver's (1) and Orla's (2). Each
// test loads fresh modules.

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

vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => [] }));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const TOLLIVER = 1;
const ORLA = 2;

/** The list the app sends, with `selected` the one selected. */
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

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

describe('what changes where your prompt shows', () => {
  it('follows the selected session and leaves out what a session behind hears', async () => {
    const { subscribePromptShowChanges } = await import('./showState');
    const changed = vi.fn();
    const stop = subscribePromptShowChanges(changed);
    await settle();
    const status = { status: 'matching', last_match_at: null };
    const seen = { kind: 'gmcp', text: '%h ', applied: true, lost: [] };

    fire('session://prompt-status', { session: ORLA, ...status });
    fire('session://game-prompt-seen', { session: ORLA, ...seen });
    fire('session://state', { session: ORLA, kind: 'disconnected', reason: null });
    expect(changed).not.toHaveBeenCalled();

    fire('session://prompt-status', { session: TOLLIVER, ...status });
    expect(changed).toHaveBeenCalledTimes(1);
    select(ORLA);
    expect(changed).toHaveBeenCalledTimes(2);
    fire('session://game-prompt-seen', { session: ORLA, ...seen });
    expect(changed).toHaveBeenCalledTimes(3);
    stop();
  });
});
