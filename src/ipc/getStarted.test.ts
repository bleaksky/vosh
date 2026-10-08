import { beforeEach, describe, expect, it, vi } from 'vitest';

// The Get started wrappers. Tauri takes camelCase argument keys from JS,
// so these pin the exact calls.
const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: tauri.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, handler: (event: { payload: unknown }) => void) => {
    tauri.handlers.set(event, handler);
    return Promise.resolve(() => undefined);
  }),
}));

const { getStartedGet, getStartedSet, openGetStarted, subscribeGetStartedOpen } =
  await import('./getStarted');

beforeEach(() => {
  tauri.invoke.mockReset();
  tauri.invoke.mockResolvedValue(undefined);
  tauri.handlers.clear();
});

describe('Get started wrappers', () => {
  it('reads where you are', async () => {
    tauri.invoke.mockResolvedValue({ atLaunch: true, done: [] });
    await expect(getStartedGet()).resolves.toEqual({ atLaunch: true, done: [] });
    expect(tauri.invoke).toHaveBeenLastCalledWith('get_started_get');
  });

  it('saves with camelCase keys', async () => {
    await getStartedSet({ atLaunch: false, done: ['connect'] });
    expect(tauri.invoke).toHaveBeenLastCalledWith('get_started_set', {
      atLaunch: false,
      done: ['connect'],
    });
  });

  it('asks the app to open it in the main window', async () => {
    await openGetStarted();
    expect(tauri.invoke).toHaveBeenLastCalledWith('open_get_started');
  });

  it('hears Help open it', async () => {
    const cb = vi.fn();
    await subscribeGetStartedOpen(cb);
    tauri.handlers.get('vosh://get-started-open')?.({ payload: null });
    expect(cb).toHaveBeenCalledOnce();
  });
});
