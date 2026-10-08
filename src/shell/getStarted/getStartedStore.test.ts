import { beforeEach, describe, expect, it, vi } from 'vitest';

// The Get started store over a stand in for Tauri, so the real IPC
// wrappers and the saved world run as the window runs them.
const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: tauri.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn((event: string, handler: (event: { payload: unknown }) => void) => {
    tauri.handlers.set(event, handler);
    return Promise.resolve(() => undefined);
  }),
}));

type Store = typeof import('./getStartedStore');
type Steps = typeof import('./steps');

const FORSAKEN = { host: 'play.theforsakenlands.com', port: 1848, tls: false };
const OTHER = { host: 'mud.example.net', port: 4000, tls: false };

/** Storage that holds `target` as the saved world, or nothing. */
function storage(target: object | null) {
  vi.stubGlobal('localStorage', {
    getItem: () => (target ? JSON.stringify(target) : null),
    setItem: () => undefined,
  });
}

/** A fresh store mounted over what profiles.toml keeps. */
async function mounted(saved: unknown): Promise<Store & Steps> {
  vi.resetModules();
  tauri.invoke.mockImplementation((cmd: string) =>
    Promise.resolve(cmd === 'get_started_get' ? saved : undefined),
  );
  const store = await import('./getStartedStore');
  const steps = await import('./steps');
  store.mountGetStarted();
  await vi.waitFor(() => expect(tauri.handlers.size).toBeGreaterThanOrEqual(5));
  await Promise.resolve();
  return { ...store, ...steps };
}

const fire = (event: string, payload: unknown) => tauri.handlers.get(event)?.({ payload });

/** The saves sent to get_started_set. */
const saves = () => tauri.invoke.mock.calls.filter(([cmd]) => cmd === 'get_started_set');

beforeEach(() => {
  tauri.invoke.mockReset();
  tauri.handlers.clear();
  storage(null);
});

describe('Get started store', () => {
  it('names The Forsaken Lands when storage holds no world', async () => {
    const s = await mounted(null);
    const steps = s.stepsFor(s.getGetStarted().target);
    expect(steps[0]?.title).toBe('Connect to The Forsaken Lands');
    expect(steps).toHaveLength(5);
  });

  it('lists five steps when the saved world is The Forsaken Lands', async () => {
    storage(FORSAKEN);
    const s = await mounted(null);
    expect(s.stepsFor(s.getGetStarted().target)).toHaveLength(5);
  });

  it('lists two steps when the saved world is another game', async () => {
    storage(OTHER);
    const s = await mounted(null);
    expect(s.stepsFor(s.getGetStarted().target).map((step) => step.id)).toEqual([
      'connect',
      'prompt',
    ]);
  });

  it('follows the world another window saves', async () => {
    const s = await mounted(null);
    fire('vosh://connection-target-changed', OTHER);
    expect(s.getGetStarted().target.host).toBe('mud.example.net');
  });

  it('keeps the card shut when profiles.toml has no table', async () => {
    const s = await mounted(null);
    expect(s.getGetStarted().shows).toBe('shut');
    fire('session://gmcp/Char-Status', { session: 1, data: { name: 'Orla' } });
    expect(saves()).toEqual([]);
  });

  it('opens the card at launch', async () => {
    const s = await mounted({ atLaunch: true, done: [] });
    await vi.waitFor(() => expect(s.getGetStarted().shows).toBe('open'));
  });

  it('stays shut when at_launch is off', async () => {
    const s = await mounted({ atLaunch: false, done: ['connect'] });
    await Promise.resolve();
    expect(s.getGetStarted().shows).toBe('shut');
  });

  it('writes at_launch off with connect done at the first connect', async () => {
    await mounted({ atLaunch: true, done: [] });
    fire('session://gmcp/Char-Status', { session: 1, data: { name: 'Orla' } });
    fire('session://gmcp/Char-Status', { session: 1, data: { name: 'Orla' } });
    expect(saves()).toEqual([['get_started_set', { atLaunch: false, done: ['connect'] }]]);
  });

  it('counts the connect on Char.Vitals alone', async () => {
    const s = await mounted({ atLaunch: true, done: [] });
    fire('session://gmcp/Char-Vitals', { session: 1, data: { hp: 1020 } });
    expect(s.getGetStarted().saved).toEqual({ atLaunch: false, done: ['connect'] });
  });

  it('counts the connect on another game at its first line, and never on a packet', async () => {
    storage(OTHER);
    const s = await mounted({ atLaunch: true, done: [] });
    fire('session://gmcp/Char-Vitals', { session: 1, data: {} });
    expect(saves()).toEqual([]);
    fire('session://output', { session: 1, id: 1, b64: btoa('Welcome!\r\n') });
    expect(s.getGetStarted().saved).toEqual({ atLaunch: false, done: ['connect'] });
  });

  it('adds each later step to what is done', async () => {
    const s = await mounted({ atLaunch: false, done: ['connect'] });
    s.markDone('prompt');
    s.noteFacts({
      character: 'Orla',
      enabledPresets: ['none'],
      panes: ['map', 'chat'],
      tracked: 0,
      promptPlace: null,
    });
    expect(saves().map(([, args]) => args)).toEqual([
      { atLaunch: false, done: ['connect', 'prompt'] },
      { atLaunch: false, done: ['connect', 'prompt', 'panes'] },
    ]);
  });

  it('ends with at_launch off, and opens on its list again from Help', async () => {
    const s = await mounted({ atLaunch: true, done: [] });
    await vi.waitFor(() => expect(s.getGetStarted().shows).toBe('open'));
    s.showPage('panes');
    s.end();
    expect(s.getGetStarted().shows).toBe('shut');
    expect(saves().at(-1)).toEqual(['get_started_set', { atLaunch: false, done: [] }]);
    fire('vosh://get-started-open', null);
    expect(s.getGetStarted()).toMatchObject({ shows: 'open', page: null });
  });

  it('folds to the notice and comes back on the page you left', async () => {
    const s = await mounted({ atLaunch: true, done: [] });
    await vi.waitFor(() => expect(s.getGetStarted().shows).toBe('open'));
    s.showPage('presets');
    s.fold();
    expect(s.getGetStarted().shows).toBe('folded');
    s.unfold();
    expect(s.getGetStarted()).toMatchObject({ shows: 'open', page: 'presets' });
  });

  it('starts a place to keep the steps when Help opens it with no table', async () => {
    await mounted(null);
    fire('vosh://get-started-open', null);
    fire('session://gmcp/Char-Status', { session: 1, data: { name: 'Orla' } });
    expect(saves()).toEqual([['get_started_set', { atLaunch: false, done: ['connect'] }]]);
  });
});
