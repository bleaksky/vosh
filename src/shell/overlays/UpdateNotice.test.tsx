import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../../test/fakeDom';

// The update notice checks only when you turned update checks on, two
// seconds after launch, and shows its card while a new version is out.

const calls = vi.hoisted(() => [] as string[]);
const answer = vi.hoisted(() => ({ autoUpdate: false, available: true }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string) => {
    calls.push(cmd);
    if (cmd === 'ui_get_config') return Promise.resolve({ auto_update: answer.autoUpdate });
    if (cmd === 'updater_check')
      return Promise.resolve({ available: answer.available, version: '1.2.0', notes: null });
    return Promise.resolve(null);
  },
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let unmount: (() => Promise<void>) | null = null;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  ({ createRoot } = await import('react-dom/client'));
});

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
  });
  calls.length = 0;
  answer.autoUpdate = false;
  answer.available = true;
});

afterEach(async () => {
  await unmount?.();
  unmount = null;
  vi.useRealTimers();
});

afterAll(() => {
  vi.unstubAllGlobals();
});

async function mount() {
  const { UpdateNotice } = await import('./UpdateNotice');
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => root.render(createElement(UpdateNotice)));
  unmount = () => act(async () => root.unmount());
  return {
    checks: () => calls.filter((c) => c === 'updater_check').length,
    message: () =>
      findAll(container, (el) => el.getAttribute('class') === 'ov-update-msg')[0]?.textContent,
    tick: (ms: number) => act(async () => void vi.advanceTimersByTime(ms)),
  };
}

describe('the update notice', () => {
  it('never checks while update checks are off', async () => {
    const notice = await mount();
    await notice.tick(10_000);
    expect(calls).toContain('ui_get_config');
    expect(notice.checks()).toBe(0);
    expect(notice.message()).toBeUndefined();
  });

  it('checks once two seconds in while they are on, and shows the card', async () => {
    answer.autoUpdate = true;
    const notice = await mount();
    await notice.tick(1999);
    expect(notice.checks()).toBe(0);
    await notice.tick(1);
    expect(notice.checks()).toBe(1);
    expect(notice.message()).toBe('Update available');
    await notice.tick(10_000);
    expect(notice.checks()).toBe(1);
  });

  it('stays hidden when no new version is out', async () => {
    answer.autoUpdate = true;
    answer.available = false;
    const notice = await mount();
    await notice.tick(10_000);
    expect(notice.checks()).toBe(1);
    expect(notice.message()).toBeUndefined();
  });
});
