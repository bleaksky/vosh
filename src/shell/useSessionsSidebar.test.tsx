import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import type { SessionsSidebar } from './useSessionsSidebar';

// Whether the main window shows the sessions sidebar, board 8. It shows
// with two or more sessions, folds in a window too narrow for it or
// after Hide sessions, and comes back as the window widens. Its width
// stays in localStorage, held between 180 and 320.

const doc = new FakeDocument();
const resize = new Set<() => void>();
const win = {
  document: doc,
  innerWidth: 1280,
  location: { protocol: 'about:' },
  HTMLIFrameElement: class {},
  addEventListener: (type: string, fn: () => void) => void (type === 'resize' && resize.add(fn)),
  removeEventListener: (_type: string, fn: () => void) => void resize.delete(fn),
};
const saved = new Map<string, string>();
const storage = {
  failing: false,
  getItem(key: string) {
    if (this.failing) throw new Error('storage is off');
    return saved.get(key) ?? null;
  },
  setItem(key: string, value: string) {
    if (this.failing) throw new Error('storage is off');
    saved.set(key, value);
  },
};
let createRoot: typeof import('react-dom/client').createRoot;
const cleanups: (() => Promise<void>)[] = [];

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', win);
  vi.stubGlobal('localStorage', storage);
  vi.stubGlobal('navigator', { userAgent: 'Macintosh', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  ({ createRoot } = await import('react-dom/client'));
});

afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) await cleanup();
  saved.clear();
  storage.failing = false;
  win.innerWidth = 1280;
});

afterAll(() => {
  vi.unstubAllGlobals();
});

/** The hook in a window `width` wide, with `count` sessions and the
 *  panel open or hidden. */
async function mount(width: number, count = 2, panelOpen = true) {
  win.innerWidth = width;
  const { useSessionsSidebar } = await import('./useSessionsSidebar');
  const seen: { now: SessionsSidebar | null } = { now: null };
  function Probe() {
    seen.now = useSessionsSidebar(count, panelOpen);
    return null;
  }
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => root.render(createElement(Probe)));
  cleanups.push(async () => act(async () => root.unmount()));
  return {
    get: () => seen.now!,
    run: (fn: () => void) => act(async () => fn()),
    resizeTo: (px: number) =>
      act(async () => {
        win.innerWidth = px;
        for (const fn of resize) fn();
      }),
  };
}

describe('the sessions sidebar in the main window', () => {
  it('folds in a window 720 by 450 with the panel open, and the popover lists the sessions', async () => {
    const m = await mount(720);
    expect(m.get()).toMatchObject({ shown: false, wanted: true, folded: true, width: 220 });
  });

  it('comes back as the window widens, and folds again as it narrows', async () => {
    const m = await mount(720);
    await m.resizeTo(741);
    expect(m.get()).toMatchObject({ shown: true, folded: false });
    await m.resizeTo(740);
    expect(m.get()).toMatchObject({ shown: false, folded: true });
  });

  it('shows in a window 720 wide while the panel is hidden', async () => {
    const m = await mount(720, 2, false);
    expect(m.get().shown).toBe(true);
  });

  it('folds after Hide sessions at any width, and Show sessions brings it back', async () => {
    const m = await mount(1280);
    await m.run(() => m.get().hide());
    expect(m.get()).toMatchObject({ shown: false, wanted: false, folded: true });
    await m.run(() => m.get().toggle());
    expect(m.get()).toMatchObject({ shown: true, wanted: true, folded: false });
  });

  it('stays hidden as a session opens, and says so while two or more are open', async () => {
    const { useSessionsSidebar } = await import('./useSessionsSidebar');
    let count = 2;
    const seen: { now: SessionsSidebar | null } = { now: null };
    function Probe() {
      seen.now = useSessionsSidebar(count, true);
      return null;
    }
    const container = doc.createElement('div');
    const root = createRoot(container as unknown as HTMLElement);
    await act(async () => root.render(createElement(Probe)));
    cleanups.push(async () => act(async () => root.unmount()));
    expect(seen.now?.hidden).toBe(false);
    await act(async () => seen.now!.hide());
    expect(seen.now).toMatchObject({ shown: false, hidden: true });
    count = 3;
    await act(async () => root.render(createElement(Probe)));
    expect(seen.now).toMatchObject({ shown: false, wanted: false, folded: true, hidden: true });
    // With one session there is no sidebar to show, so nothing offers it.
    count = 1;
    await act(async () => root.render(createElement(Probe)));
    expect(seen.now?.hidden).toBe(false);
    count = 2;
    await act(async () => root.render(createElement(Probe)));
    expect(seen.now?.hidden).toBe(true);
  });

  it('shows nothing and lists nothing with one session', async () => {
    const m = await mount(1280, 1);
    expect(m.get()).toMatchObject({ shown: false, wanted: false, folded: false });
  });

  it('keeps the width you drag it to for the next launch', async () => {
    const m = await mount(1280);
    await m.run(() => m.get().setWidth(260));
    expect(m.get().width).toBe(260);
    expect(saved.get('vosh.layout.sessionsWidth')).toBe('260');
    saved.set('vosh.layout.sessionsWidth', '180');
    expect((await mount(1280)).get().width).toBe(180);
  });

  it('holds a kept width between 180 and 320, and starts at 220 without one', async () => {
    saved.set('vosh.layout.sessionsWidth', '400');
    expect((await mount(1280)).get().width).toBe(320);
    saved.set('vosh.layout.sessionsWidth', 'wide');
    expect((await mount(1280)).get().width).toBe(220);
    storage.failing = true;
    const m = await mount(1280);
    expect(m.get().width).toBe(220);
    // A width storage refuses still holds in this window.
    await m.run(() => m.get().setWidth(200));
    expect(m.get().width).toBe(200);
  });
});
