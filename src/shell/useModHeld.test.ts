import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';

// The hold that numbers the sidebar's rows, driven through a window that
// keeps its listeners, on macOS, where Mod is ⌘.

type Listener = (e: Partial<KeyboardEvent>) => void;
const listeners = new Map<string, Set<Listener>>();

function fire(type: string, e: Partial<KeyboardEvent> = {}): void {
  for (const cb of listeners.get(type) ?? []) cb(e);
}

const down = (key: string, mods: Partial<KeyboardEvent> = {}) =>
  fire('keydown', { key, metaKey: key === 'Meta', ...mods });
const up = (key: string) => fire('keyup', { key });

describe('useModHeld', () => {
  const doc = new FakeDocument();
  let createRoot: typeof import('react-dom/client').createRoot;
  let held: boolean[];
  let unmount: () => Promise<void>;

  beforeAll(async () => {
    vi.useFakeTimers();
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      setTimeout,
      clearTimeout,
      addEventListener(type: string, cb: Listener) {
        let set = listeners.get(type);
        if (!set) listeners.set(type, (set = new Set()));
        set.add(cb);
      },
      removeEventListener(type: string, cb: Listener) {
        listeners.get(type)?.delete(cb);
      },
    });
    vi.stubGlobal('navigator', { userAgent: 'Macintosh', platform: '' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    ({ createRoot } = await import('react-dom/client'));
  });

  afterAll(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  async function mount() {
    const { useModHeld, MOD_HOLD_MS } = await import('./useModHeld');
    held = [];
    function Probe() {
      held.push(useModHeld());
      return null;
    }
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () => root.render(createElement(Probe)));
    unmount = () => act(async () => root.unmount());
    return MOD_HOLD_MS;
  }

  const now = () => held[held.length - 1];
  const wait = (ms: number) => act(async () => void vi.advanceTimersByTime(ms));

  afterEach(async () => {
    await unmount();
    expect([...listeners.values()].every((set) => set.size === 0)).toBe(true);
  });

  it('shows once ⌘ is held alone a moment, until you let go', async () => {
    const hold = await mount();
    await act(async () => down('Meta'));
    await wait(hold - 1);
    expect(now()).toBe(false);
    await wait(1);
    expect(now()).toBe(true);
    // A number pressed while they show keeps them, so you can read on.
    await act(async () => down('2', { metaKey: true }));
    expect(now()).toBe(true);
    await act(async () => up('Meta'));
    expect(now()).toBe(false);
  });

  it('never shows for a quick shortcut like ⌘C', async () => {
    const hold = await mount();
    await act(async () => down('Meta'));
    await act(async () => down('c', { metaKey: true }));
    await wait(hold * 2);
    expect(now()).toBe(false);
    await act(async () => up('Meta'));
  });

  it('waits for ⌘ alone, not with Shift or Ctrl', async () => {
    const hold = await mount();
    await act(async () => down('Meta', { shiftKey: true }));
    await wait(hold * 2);
    expect(now()).toBe(false);
    await act(async () => down('Control', { ctrlKey: true }));
    await wait(hold * 2);
    expect(now()).toBe(false);
  });

  it('lets go when the window loses focus', async () => {
    const hold = await mount();
    await act(async () => down('Meta'));
    await wait(hold);
    expect(now()).toBe(true);
    await act(async () => fire('blur'));
    expect(now()).toBe(false);
  });
});
