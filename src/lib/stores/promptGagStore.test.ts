import { beforeEach, describe, expect, it, vi } from 'vitest';

// The triggers the session names on session://prompt-gag-without-reader,
// through a fake Tauri event bus. Each test loads a fresh store module.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
// What prompt_gags_without_reader answers.
let named: string[] = [];

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
    if (cmd === 'prompt_gags_without_reader') return named;
    throw new Error(`no fake for ${cmd}`);
  },
}));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const gag = (trigger: unknown) => fire('session://prompt-gag-without-reader', { trigger });
const state = (kind: string) => fire('session://state', { kind, reason: null });

async function load() {
  const store = await import('./promptGagStore');
  store.subscribePromptGags(() => {});
  await settle();
  return store;
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  named = [];
});

describe('the triggers that hide your prompt with nothing drawn', () => {
  it('asks for the ones the session named before the window opened', async () => {
    named = ['old-capture'];
    const store = await load();
    expect([...store.getPromptGags()]).toEqual(['old-capture']);
  });

  it('adds each trigger the session names, once', async () => {
    const store = await load();
    gag('my-capture');
    gag('my-capture');
    gag('');
    gag(7);
    expect([...store.getPromptGags()]).toEqual(['my-capture']);
    const before = store.getPromptGags();
    gag('my-capture');
    expect(store.getPromptGags()).toBe(before);
  });

  it('starts over when a connection opens or closes', async () => {
    const store = await load();
    gag('my-capture');
    state('connected');
    expect(store.getPromptGags().size).toBe(1);
    state('disconnected');
    expect(store.getPromptGags().size).toBe(0);
    gag('my-capture');
    state('connecting');
    expect(store.getPromptGags().size).toBe(0);
  });
});

describe('a trigger the mark names no longer hides your prompt', () => {
  it('asks again when the profile reads a prompt, or another profile takes over', async () => {
    named = [];
    const store = await load();
    gag('my-capture');
    expect([...store.getPromptGags()]).toEqual(['my-capture']);
    // You told Vosh your prompt in Customize prompt, so the session
    // forgot the trigger.
    fire('vosh://prompt-config-changed', { profile: 'default' });
    await settle();
    expect(store.getPromptGags().size).toBe(0);
    gag('my-capture');
    fire('vosh://profile-switched', 'Healer');
    await settle();
    expect(store.getPromptGags().size).toBe(0);
    named = ['other-capture'];
    fire('vosh://ui-config-replaced', null);
    await settle();
    expect([...store.getPromptGags()]).toEqual(['other-capture']);
  });
});
