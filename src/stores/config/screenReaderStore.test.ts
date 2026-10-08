import { beforeEach, describe, expect, it, vi } from 'vitest';

// Drives the screen reader store through a fake Tauri event bus and a
// fake ui_get_config. Each test loads a fresh store module, since it
// keeps its state at module scope.

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
    throw new Error(`no fake for ${cmd}`);
  },
}));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const OFF = {
  screen_reader: false,
  screen_reader_background: false,
  screen_reader_prompt: false,
  screen_reader_burst: 8,
};

async function load() {
  const store = await import('./screenReaderStore');
  store.startScreenReaderStore();
  await settle();
  return store;
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  config = {};
});

describe('screenReaderStore', () => {
  it('reads the choices of the profile in front, off until you turn the reader on', async () => {
    expect((await load()).getScreenReader()).toEqual(OFF);
    vi.resetModules();
    handlers.clear();
    config = { screen_reader: true, screen_reader_burst: 32 };
    expect((await load()).getScreenReader()).toEqual({
      ...OFF,
      screen_reader: true,
      screen_reader_burst: 32,
    });
  });

  it('follows what Settings sends and keeps the snapshot when nothing moved', async () => {
    const store = await load();
    const sent = { ...OFF, screen_reader: true, screen_reader_prompt: true };
    fire('vosh://screen-reader-changed', sent);
    const heard = store.getScreenReader();
    expect(heard).toEqual(sent);
    fire('vosh://screen-reader-changed', { ...sent });
    expect(store.getScreenReader()).toBe(heard);
    fire('vosh://screen-reader-changed', { ...sent, screen_reader_burst: 4 });
    expect(store.getScreenReader().screen_reader_burst).toBe(4);
  });

  it('reads again after a profile switch and after a replaced config', async () => {
    const store = await load();
    config = { screen_reader: true };
    fire('vosh://profile-switched', 'Maren');
    await settle();
    expect(store.getScreenReader().screen_reader).toBe(true);
    config = { screen_reader: true, screen_reader_background: true };
    fire('vosh://ui-config-replaced', null);
    await settle();
    expect(store.getScreenReader().screen_reader_background).toBe(true);
  });
});
