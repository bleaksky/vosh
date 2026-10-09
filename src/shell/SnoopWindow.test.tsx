import { act, createElement } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SnoopTab } from '../ipc/snoop';
import type { Snoops } from '../stores/session/snoopStore';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import { shortcutLabel } from '../lib/shortcuts';
import { SnoopWindow } from './SnoopWindow';

// The snoop window. The snoop store, the look and the window are faked,
// and each snoop terminal stands in as a plain element that names its
// player, so what shows is what the window draws. The menu draws in
// place, not in a portal, and the find bar stands in as a plain
// element.

const fake = vi.hoisted(() => ({
  snoops: { tabs: [], windowed: true, selected: null, unread: new Set() } as unknown as Snoops,
  read: [] as number[],
  selected: [] as unknown[],
  stops: [] as unknown[],
  windows: [] as unknown[],
  closed: 0,
  find: null as ((session: number) => void) | null,
}));

vi.mock('../stores/session/snoopStore', () => ({
  useSnoopsOf: (session: number) => {
    fake.read.push(session);
    return fake.snoops;
  },
  selectSnoop: (name: string, session: number) => {
    fake.selected.push([name, session]);
    fake.snoops = { ...fake.snoops, selected: name };
  },
}));
vi.mock('./useSnoopWindowLook', () => ({
  useSnoopWindowLook: () => ({
    fontFamily: 'Menlo',
    fontSize: 14,
    lineHeight: 1.2,
    themeTerminalColors: false,
  }),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: async () => void (fake.closed += 1),
  }),
}));
vi.mock('../ipc/snoop', () => ({
  snoopStop: async (session?: number, name?: string) => void fake.stops.push([session, name]),
  snoopClose: async () => undefined,
  snoopWindowOpen: async (session?: number) => void fake.windows.push(session),
  onSnoopFind: async (cb: (session: number) => void) => {
    fake.find = cb;
    return () => {
      fake.find = null;
    };
  },
}));
vi.mock('../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: unknown }) =>
    createElement('menu', { role: 'menu', 'aria-label': label }, children as never),
}));
vi.mock('../terminal/FindToolbar', () => ({
  FindToolbar: () => createElement('div', { 'data-find': '' }),
}));
vi.mock('./sessionLine', async (actual) => ({
  ...(await actual<typeof import('./sessionLine')>()),
  useMinuteClock: () => 1_800_000_000_000,
}));
vi.mock('../terminal/SnoopTerminal', () => ({
  SnoopTerminal: ({ name, shown }: { name: string; shown: boolean }) =>
    createElement('div', { 'data-term': name, hidden: !shown }),
}));

const live = (name: string): SnoopTab => ({
  name,
  live: true,
  ended_at: null,
  last_output_at: null,
});

function snoops(tabs: SnoopTab[], selected: string | null, unread: string[] = []) {
  fake.snoops = { tabs, windowed: true, selected, folded: false, unread: new Set(unread) };
}

/** The Find row, with its keys as macOS writes them. */
const find = () => `Find${shortcutLabel('Mod+F')}`;

const STAFF = 3;

describe('the snoop window (board 06)', () => {
  const doc = new FakeDocument();
  const keys = new Set<(event: unknown) => void>();
  let createRoot: typeof import('react-dom/client').createRoot;

  beforeAll(async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    Object.assign(doc, {
      addEventListener: (type: string, cb: (event: unknown) => void) => {
        if (type === 'keydown') keys.add(cb);
      },
      removeEventListener: (_type: string, cb: (event: unknown) => void) => keys.delete(cb),
    });
    doc.documentElement.dataset.platform = 'macos';
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener() {},
      removeEventListener() {},
    });
    vi.stubGlobal('navigator', { userAgent: 'Mac OS X', platform: 'MacIntel' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    // The menu hangs from the more button's box.
    Object.assign(FakeElement.prototype, {
      getBoundingClientRect: () => ({ left: 0, right: 28, top: 4, bottom: 28 }),
    });
    ({ createRoot } = await import('react-dom/client'));
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    fake.read.length = 0;
    fake.selected.length = 0;
    fake.stops.length = 0;
    fake.windows.length = 0;
    fake.closed = 0;
  });

  type Handler = () => void;
  function on(el: FakeElement): Record<string, Handler> {
    const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
    if (!key) throw new Error('the element has no React props');
    return (el as unknown as Record<string, Record<string, Handler>>)[key];
  }

  async function mount() {
    const host = doc.createElement('div');
    const root = createRoot(host as unknown as HTMLElement);
    const draw = () => act(() => root.render(createElement(SnoopWindow, { session: STAFF })));
    draw();
    // The menu bar's Find starts hearing once its listen settles.
    await act(async () => {});
    const all = (match: (el: FakeElement) => boolean) => findAll(host, match);
    const button = (text: string) =>
      all((el) => el.nodeName === 'BUTTON' && el.textContent === text)[0];
    return {
      all,
      redraw: draw,
      press: async (text: string) => {
        act(() => on(button(text)).onClick());
        await Promise.resolve();
      },
      more: () =>
        act(() => on(all((el) => el.getAttribute('aria-label') === 'Snoop options')[0]).onClick()),
      items: () => all((el) => el.getAttribute('role') === 'menuitem').map((el) => el.textContent),
      finding: () => all((el) => el.getAttribute('data-find') === '').length > 0,
      unmount: () => act(() => root.unmount()),
    };
  }

  it('reads the snoops of its own session', async () => {
    snoops([live('Tolliver')], 'Tolliver');
    const win = await mount();
    expect(new Set(fake.read)).toEqual(new Set([STAFF]));
    win.unmount();
  });

  it('puts the strip in the band, which drags the window, over a terminal for each tab', async () => {
    snoops([live('Tolliver'), live('Maren'), live('Orla')], 'Maren', ['Orla']);
    const win = await mount();
    const band = win.all((el) => el.nodeName === 'HEADER')[0];
    expect(band.getAttribute('class')).toBe('snoop-window-band');
    expect(band.getAttribute('data-tauri-drag-region')).toBe('');
    const strip = win.all((el) => el.getAttribute('class') === 'snoop-strip')[0];
    expect(strip.getAttribute('data-tauri-drag-region')).toBe('');
    const tabs = win.all((el) => el.getAttribute('role') === 'tab');
    expect(tabs.map((tab) => tab.textContent)).toEqual(['Tolliver', 'Maren', 'Orla']);
    expect(tabs.map((tab) => tab.getAttribute('aria-selected'))).toEqual([
      'false',
      'true',
      'false',
    ]);
    expect(tabs[2].getAttribute('class')).toBe('snoop-tab is-unread');
    const terms = win.all((el) => el.getAttribute('data-term') !== null);
    expect(terms.map((el) => [el.getAttribute('data-term'), el.getAttribute('hidden')])).toEqual([
      ['Tolliver', ''],
      ['Maren', null],
      ['Orla', ''],
    ]);
    // No command line, and the traffic lights draw the controls on macOS.
    expect(win.all((el) => el.nodeName === 'TEXTAREA' || el.nodeName === 'INPUT')).toEqual([]);
    expect(win.all((el) => el.getAttribute('aria-label') === 'Close')).toEqual([]);
    win.unmount();
  });

  it('stops the snoop in front and picks a tab in its own session', async () => {
    snoops([live('Tolliver'), live('Maren')], 'Maren');
    const win = await mount();
    await win.press('Stop');
    expect(fake.stops).toEqual([[STAFF, 'Maren']]);
    await win.press('Tolliver');
    expect(fake.selected).toEqual([['Tolliver', STAFF]]);
    win.unmount();
  });

  it('has a menu with no Open in a window and no Fold', async () => {
    snoops([live('Tolliver'), live('Maren')], 'Maren');
    const win = await mount();
    win.more();
    expect(win.items()).toEqual(['Stop snooping Maren', 'Stop every snoop', find()]);
    await win.press('Stop every snoop');
    expect(fake.stops).toEqual([[STAFF, undefined]]);
    expect(fake.windows).toEqual([]);
    win.unmount();
  });

  it('opens Find from the menu, from Cmd F and from the menu bar for its session', async () => {
    snoops([live('Maren')], 'Maren');
    const win = await mount();
    win.more();
    await win.press(find());
    expect(win.finding()).toBe(true);
    win.unmount();

    const again = await mount();
    act(() => fake.find?.(STAFF + 1));
    expect(again.finding()).toBe(false);
    act(() => fake.find?.(STAFF));
    expect(again.finding()).toBe(true);
    again.unmount();

    const keyed = await mount();
    let stopped = false;
    act(() => {
      for (const cb of keys) {
        cb({
          key: 'f',
          code: 'KeyF',
          metaKey: true,
          ctrlKey: false,
          altKey: false,
          shiftKey: false,
          preventDefault: () => (stopped = true),
        });
      }
    });
    expect(keyed.finding()).toBe(true);
    expect(stopped).toBe(true);
    keyed.unmount();
  });

  it('closes itself once the last tab goes, and not before the tabs come', async () => {
    snoops([], null);
    const win = await mount();
    expect(fake.closed).toBe(0);
    snoops([live('Orla')], 'Orla');
    win.redraw();
    expect(fake.closed).toBe(0);
    snoops([], null);
    win.redraw();
    expect(fake.closed).toBe(1);
    win.unmount();
  });
});
