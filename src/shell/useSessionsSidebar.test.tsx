import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import type { SessionsSidebar } from './useSessionsSidebar';

// Whether the main window shows the sessions sidebar, board 8. It shows
// with two or more sessions, folds in a window too narrow for it or
// after the toggle hides it, and comes back as the window widens. In a
// narrow window the toggle slides it over the terminal until you press
// it again or press Esc (Sessions toggle T5). Its width stays in
// localStorage, held between 180 and 320.

const doc = new FakeDocument();
const resize = new Set<() => void>();
const keys = new Set<(e: KeyboardEvent) => void>();
const win = {
  document: doc,
  innerWidth: 1280,
  location: { protocol: 'about:' },
  HTMLIFrameElement: class {},
  addEventListener: (type: string, fn: never) => {
    if (type === 'resize') resize.add(fn);
    if (type === 'keydown') keys.add(fn);
  },
  removeEventListener: (_type: string, fn: never) => {
    resize.delete(fn);
    keys.delete(fn);
  },
};

/** Press Escape in the window, and say whether something took it. */
function pressEscape(): boolean {
  let taken = false;
  const e = {
    key: 'Escape',
    isComposing: false,
    target: null,
    preventDefault: () => (taken = true),
    stopPropagation: () => undefined,
  } as unknown as KeyboardEvent;
  for (const fn of keys) fn(e);
  return taken;
}
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
  doc.activeElement = null;
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
  const focusInput = vi.fn();
  const sessions = { count };
  function Probe() {
    seen.now = useSessionsSidebar(sessions.count, panelOpen, focusInput);
    return null;
  }
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => root.render(createElement(Probe)));
  cleanups.push(async () => act(async () => root.unmount()));
  return {
    get: () => seen.now!,
    focusInput,
    /** Open or close sessions until `n` are open. */
    sessionsTo: (n: number) =>
      act(async () => {
        sessions.count = n;
        root.render(createElement(Probe));
      }),
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
    expect(m.get()).toMatchObject({
      shown: false,
      overlay: false,
      pressed: false,
      toggleable: true,
      folded: true,
      width: 220,
    });
  });

  it('comes back as the window widens, and folds again as it narrows', async () => {
    const m = await mount(720);
    await m.resizeTo(741);
    expect(m.get()).toMatchObject({ shown: true, pressed: true, folded: false });
    await m.resizeTo(740);
    expect(m.get()).toMatchObject({ shown: false, pressed: false, folded: true });
  });

  it('shows in a window 720 wide while the panel is hidden', async () => {
    const m = await mount(720, 2, false);
    expect(m.get().shown).toBe(true);
  });

  it('hides at a press of the toggle at any width, and a second press brings it back', async () => {
    const m = await mount(1280);
    await m.run(() => m.get().toggle());
    expect(m.get()).toMatchObject({ shown: false, pressed: false, toggleable: true, folded: true });
    await m.run(() => m.get().toggle());
    expect(m.get()).toMatchObject({ shown: true, pressed: true, folded: false });
  });

  it('stays hidden as sessions open and close, once you hid it', async () => {
    const m = await mount(1280);
    await m.run(() => m.get().toggle());
    await m.sessionsTo(3);
    expect(m.get()).toMatchObject({ shown: false, pressed: false, folded: true });
    // With one session there is no sidebar and no toggle.
    await m.sessionsTo(1);
    expect(m.get()).toMatchObject({ shown: false, toggleable: false, folded: false });
    await m.sessionsTo(2);
    expect(m.get()).toMatchObject({ shown: false, toggleable: true });
  });

  it('shows itself as a second session opens', async () => {
    const m = await mount(1280, 1);
    expect(m.get()).toMatchObject({ shown: false, toggleable: false });
    await m.sessionsTo(2);
    expect(m.get()).toMatchObject({ shown: true, pressed: true, toggleable: true });
  });

  it('shows nothing and lists nothing with one session', async () => {
    const m = await mount(1280, 1);
    expect(m.get()).toMatchObject({
      shown: false,
      overlay: false,
      pressed: false,
      toggleable: false,
      folded: false,
    });
  });
});

describe('the sessions sidebar over the terminal', () => {
  it('slides in at a press of the toggle in a window too narrow for its column', async () => {
    const m = await mount(720);
    await m.run(() => m.get().toggle());
    expect(m.get()).toMatchObject({ shown: false, overlay: true, pressed: true, folded: true });
    // A press again puts it away and hands the caret to the command line.
    await m.run(() => m.get().toggle());
    expect(m.get()).toMatchObject({ overlay: false, pressed: false });
    expect(m.focusInput).toHaveBeenCalledTimes(1);
  });

  it('slides in for a sidebar you hid as well, and keeps it hidden in its column', async () => {
    const m = await mount(1280);
    await m.run(() => m.get().toggle());
    await m.resizeTo(720);
    await m.run(() => m.get().toggle());
    expect(m.get().overlay).toBe(true);
    await m.resizeTo(1280);
    expect(m.get()).toMatchObject({ shown: false, overlay: false, pressed: false });
  });

  it('goes at Esc, which hands the caret to the command line', async () => {
    const m = await mount(720);
    expect(pressEscape()).toBe(false);
    await m.run(() => m.get().toggle());
    let taken = false;
    await m.run(() => {
      taken = pressEscape();
    });
    expect(taken).toBe(true);
    expect(m.get().overlay).toBe(false);
    expect(m.focusInput).toHaveBeenCalledTimes(1);
    // Nothing waits on Esc once it went.
    expect(pressEscape()).toBe(false);
  });

  it('goes when you pick a row, leaving the caret to the row', async () => {
    const m = await mount(720);
    await m.run(() => m.get().toggle());
    await m.run(() => m.get().closeOverlay());
    expect(m.get().overlay).toBe(false);
    expect(m.focusInput).not.toHaveBeenCalled();
  });

  it('gives way to the column as the window widens, and stays away as it narrows again', async () => {
    const m = await mount(720);
    await m.run(() => m.get().toggle());
    await m.resizeTo(1280);
    expect(m.get()).toMatchObject({ shown: true, overlay: false, pressed: true });
    await m.resizeTo(720);
    expect(m.get()).toMatchObject({ shown: false, overlay: false, pressed: false });
  });

  it('goes as the sessions drop to one', async () => {
    const m = await mount(720);
    await m.run(() => m.get().toggle());
    await m.sessionsTo(1);
    expect(m.get()).toMatchObject({ overlay: false, toggleable: false });
    await m.sessionsTo(2);
    expect(m.get().overlay).toBe(false);
  });
});

describe('the caret after a toggle press', () => {
  /** An element inside one with `className`, focused. */
  function focusInside(className: string) {
    const outer = doc.createElement('div');
    outer.setAttribute('class', className);
    const button = doc.createElement('button');
    outer.appendChild(button);
    button.focus();
  }

  it('goes back to the command line from the toggle a click focused', async () => {
    const m = await mount(1280);
    focusInside('shell-lead');
    await m.run(() => m.get().toggle());
    expect(m.focusInput).toHaveBeenCalledTimes(1);
  });

  it('goes back to the command line from a row of a sidebar that hides', async () => {
    const m = await mount(1280);
    focusInside('shell-slot-sessions');
    await m.run(() => m.get().toggle());
    expect(m.focusInput).toHaveBeenCalledTimes(1);
  });

  it('stays in the command line, or wherever else it was, after the key', async () => {
    const m = await mount(1280);
    focusInside('input-field');
    await m.run(() => m.get().toggle());
    await m.run(() => m.get().toggle());
    expect(m.focusInput).not.toHaveBeenCalled();
  });
});

describe('the sessions sidebar width', () => {
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
