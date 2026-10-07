import { act, createElement } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument } from '../../test/fakeDom';
import type { BannerPermission } from './useBannerPermission';

// What the system says about Vosh's banners, read on mount and again
// when the window comes back to the front, and the ask that runs once
// per window before the system's own question.

const system = vi.hoisted(() => ({
  permission: 'not_asked',
  answer: 'granted',
  sent: [] as string[],
  focus: new Set<() => void>(),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string) => {
    system.sent.push(cmd);
    if (cmd === 'alerts_permission') return Promise.resolve(system.permission);
    if (cmd === 'alerts_ask_permission') return Promise.resolve(system.answer);
    return Promise.reject(new Error(`no fake for ${cmd}`));
  },
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let useBannerPermission: typeof import('./useBannerPermission').useBannerPermission;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener: (name: string, cb: () => void) => {
      if (name === 'focus') system.focus.add(cb);
    },
    removeEventListener: (name: string, cb: () => void) => {
      if (name === 'focus') system.focus.delete(cb);
    },
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  ({ createRoot } = await import('react-dom/client'));
  ({ useBannerPermission } = await import('./useBannerPermission'));
});

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
  system.sent.splice(0);
});

const settle = () => act(async () => new Promise((resolve) => setTimeout(resolve, 0)));

/** Mount the hook and hand back what it last returned. */
async function mount(): Promise<() => BannerPermission> {
  let last: BannerPermission | undefined;
  function Probe() {
    last = useBannerPermission();
    return null;
  }
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => root.render(createElement(Probe)));
  await settle();
  cleanups.push(async () => {
    await act(async () => root.unmount());
  });
  return () => {
    if (!last) throw new Error('the hook never ran');
    return last;
  };
}

// The tests run in order, since Not now holds for the module.
describe('useBannerPermission', () => {
  it('reads the permission on mount and again when the window comes back', async () => {
    system.permission = 'denied';
    const hook = await mount();
    expect(hook().permission).toBe('denied');
    system.permission = 'granted';
    await act(async () => {
      for (const cb of system.focus) cb();
    });
    await settle();
    expect(hook().permission).toBe('granted');
    expect(system.sent).toEqual(['alerts_permission', 'alerts_permission']);
  });

  it('runs at once while the system has asked already', async () => {
    system.permission = 'granted';
    const hook = await mount();
    const ran: string[] = [];
    await act(async () => hook().askFirst(() => ran.push('banner')));
    expect(hook().asking).toBe(false);
    expect(ran).toEqual(['banner']);
  });

  it('asks first while the system has not, and Continue keeps the answer', async () => {
    system.permission = 'not_asked';
    system.answer = 'denied';
    const hook = await mount();
    const ran: string[] = [];
    await act(async () => hook().askFirst(() => ran.push('banner')));
    expect(hook().asking).toBe(true);
    expect(ran).toEqual([]);
    await act(async () => hook().answer(true));
    await settle();
    expect(hook().asking).toBe(false);
    expect(ran).toEqual(['banner']);
    expect(hook().permission).toBe('denied');
    expect(system.sent).toContain('alerts_ask_permission');
  });

  it('runs after Not now and asks no more in this window', async () => {
    system.permission = 'not_asked';
    const hook = await mount();
    const ran: string[] = [];
    await act(async () => hook().askFirst(() => ran.push('first')));
    await act(async () => hook().answer(false));
    expect(ran).toEqual(['first']);
    expect(system.sent).not.toContain('alerts_ask_permission');

    const again = await mount();
    await act(async () => again().askFirst(() => ran.push('second')));
    expect(again().asking).toBe(false);
    expect(ran).toEqual(['first', 'second']);
  });
});
