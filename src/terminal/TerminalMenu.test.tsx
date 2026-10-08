import type { WritingKind } from '../ipc/writing';
import { act, createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { SETTINGS_GOTO_TAB } from '../ipc/events';
import { pushEscape } from '../lib/escapeStack';
import { SETTINGS_PENDING_KEY } from '../lib/settingsLink';
import { SETTINGS_MENU } from './settingsMenu';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import { TerminalMenu } from './TerminalMenu';

// What the menu sends out: the Tauri commands it invokes, the events it
// emits, and when it asks to close, in one log so the order shows.
const calls = vi.hoisted(() => ({ log: [] as string[] }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => {
    calls.log.push(`invoke ${cmd}`);
    return Promise.resolve();
  }),
}));
// The menu surface draws in place, so a static render shows the rows
// and a mounted menu sits in the page the test made.
vi.mock('react-dom', async (actual) => ({
  ...(await actual<typeof import('react-dom')>()),
  createPortal: (children: ReactNode) => children,
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn((event: string, payload: unknown) => {
    calls.log.push(`emit ${event} ${String(payload)}`);
    return Promise.resolve();
  }),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The menu asks storage which renderer draws the terminal, and Settings
// links leave their target there for a cold open. The xterm renderer
// draws here, so Clear scrollback shows.
const store = new Map<string, string>();
vi.stubGlobal('localStorage', {
  getItem: (key: string) => store.get(key) ?? null,
  setItem: (key: string, value: string) => void store.set(key, String(value)),
  removeItem: (key: string) => void store.delete(key),
});
store.set('vosh.nativesurface', '0');

const props = {
  session: 1,
  termRef: { current: null },
  inputRef: { current: null },
  onOpenFind: () => {},
  onCustomizePrompt: () => {},
  writeKinds: ['note', 'journal', 'application', 'idea', 'bug', 'typo'] as WritingKind[],
  onWrite: () => {},
};

/** The menu as markup. The surface names the page body it would draw
 *  into, and the shortcuts the platform, which a bare page stands in
 *  for. */
function drawn(): string {
  const page = globalThis as { document?: unknown };
  page.document = { body: null, documentElement: { dataset: { platform: 'macos' } } };
  try {
    return renderToStaticMarkup(<TerminalMenu x={10} y={10} {...props} onClose={() => {}} />);
  } finally {
    delete page.document;
  }
}

describe('the terminal menu', () => {
  const labels = (html: string) =>
    [...html.matchAll(/class="menu-label">([^<]*)</g)].map((m) => m[1]);

  it('offers Customize prompt… and Write first, apart from the rest, on any row (P1, Note Editor Q2)', () => {
    const html = drawn();
    expect(labels(html).slice(0, 3)).toEqual(['Customize prompt…', 'Write', 'Copy']);
    // A separator stands between it and Copy.
    const first = html.indexOf('Customize prompt…');
    const sep = html.indexOf('role="separator"');
    expect(sep).toBeGreaterThan(first);
    expect(sep).toBeLessThan(html.indexOf('>Copy<'));
  });

  it('offers Settings apart after Find, with Clear scrollback still last', () => {
    const html = drawn();
    expect(labels(html)).toEqual([
      'Customize prompt…',
      'Write',
      'Copy',
      'Paste',
      'Select all',
      'Find in scrollback…',
      'Save a scene…',
      'Settings',
      'Clear scrollback',
    ]);
    // Settings sits in a group of its own, says it opens a menu, and
    // shows a chevron where the other rows show a shortcut.
    const groups = html.split('role="separator"');
    const settings = groups.find((g) => g.includes('>Settings<')) ?? '';
    expect(labels(settings)).toEqual(['Settings']);
    expect(settings).toContain('aria-haspopup="menu"');
    expect(settings).toContain('aria-expanded="false"');
    expect(settings).toContain('menu-chevron');
    expect(settings).not.toContain('menu-keys');
    // Clear scrollback, last, is drawn in the danger tone.
    expect(groups.at(-1)).toContain('class="menu-item is-danger"');
  });
});

// ── The Settings list, mounted ──────────────────────────────────────
// React DOM mounts the menu on a stand in DOM (src/test/fakeDom.ts),
// with the real menu surface the pane menus use. The stand in learns
// here the few calls the menu and its surface make beyond what React
// DOM needs: the size and box of a menu and a row, contains, closest
// for the menu surface mark, and the selector the surface finds its
// rows with.

const ITEM_SELECTOR = '[role^="menuitem"]:not([aria-disabled="true"]):not(:disabled)';

// A 1280 by 800 window. The terminal menu is 232 by 274 and the
// Settings list 160 by 398.
const VW = 1280;
const VH = 800;
const MENU = { w: 232, h: 274 };
const LIST = { w: 160, h: 398 };

interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}
const boxes = new Map<FakeElement, Box>();

/** The menu surface `start` sits in, or null. */
function surfaceAround(start: FakeNode | null): FakeElement | null {
  for (let n = start; n; n = n.parentNode) {
    if (n instanceof FakeElement && n.hasAttribute('data-menu-surface')) return n;
  }
  return null;
}

/** The rows of a menu surface you can move to, as ITEM_SELECTOR finds
 *  them. */
function menuRows(menu: FakeElement): FakeElement[] {
  return findAll(
    menu,
    (b) =>
      (b.getAttribute('role') ?? '').startsWith('menuitem') &&
      b.getAttribute('aria-disabled') !== 'true' &&
      !b.hasAttribute('disabled'),
  );
}

function teachTheDom() {
  const node = FakeNode.prototype as unknown as Record<string, unknown>;
  node.contains = function (this: FakeNode, other: FakeNode | null): boolean {
    for (let n = other; n; n = n.parentNode) if (n === this) return true;
    return false;
  };
  const el = FakeElement.prototype as unknown as Record<string, unknown>;
  // Focus moving to a row tells React, as the page does, so the row
  // closes a list it does not open.
  el.focus = function (this: FakeElement) {
    if (!this.ownerDocument || this.ownerDocument.activeElement === this) return;
    this.ownerDocument.activeElement = this;
    const key = Object.keys(this).find((k) => k.startsWith('__reactProps$'));
    const props = key ? (this as unknown as Record<string, { onFocus?: () => void }>)[key] : null;
    props?.onFocus?.();
  };
  el.getBoundingClientRect = function (this: FakeElement): Box {
    return boxes.get(this) ?? { left: 0, top: 0, right: 0, bottom: 0 };
  };
  el.closest = function (this: FakeElement, selector: string): FakeElement | null {
    // The menu surface mark, or the menu a submenu row sits in.
    if (selector !== '[data-menu-surface]' && selector !== 'menu')
      throw new Error(`no closest for ${selector}`);
    return surfaceAround(this);
  };
  el.querySelectorAll = function (this: FakeElement, selector: string): FakeElement[] {
    if (selector !== ITEM_SELECTOR) throw new Error(`no querySelectorAll for ${selector}`);
    return menuRows(this);
  };
  el.querySelector = function (this: FakeElement, selector: string): FakeElement | null {
    if (selector !== ITEM_SELECTOR) throw new Error(`no querySelector for ${selector}`);
    return menuRows(this)[0] ?? null;
  };
  const size = (pick: (s: { w: number; h: number }) => number) => ({
    configurable: true,
    get(this: FakeElement) {
      return pick(this.getAttribute('aria-label') === 'Terminal' ? MENU : LIST);
    },
  });
  Object.defineProperty(
    FakeElement.prototype,
    'offsetWidth',
    size((s) => s.w),
  );
  Object.defineProperty(
    FakeElement.prototype,
    'offsetHeight',
    size((s) => s.h),
  );
}

type Handler = (e?: unknown) => void;
const windowListeners = new Map<string, Handler>();
const documentListeners = new Map<string, Set<Handler>>();
const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;

/** The handlers React keeps on an element. This DOM sends no events. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

/** The pointer moving over `el`. */
const point = (el: FakeElement) => on(el).onPointerMove({ currentTarget: el });

const keyEvent = (key: string) => {
  const e = {
    key,
    isComposing: false,
    target: doc.activeElement,
    stopped: false,
    preventDefault() {},
    stopPropagation() {
      e.stopped = true;
    },
  };
  return e;
};

/** Press `key` where focus is: the row under focus first, then the
 *  menu surface around it, unless the row keeps it. */
function press(key: string) {
  const target = doc.activeElement as FakeElement | null;
  const surface = surfaceAround(target);
  if (!target || !surface) throw new Error('focus is outside every menu');
  const e = keyEvent(key);
  if (target !== surface) on(target).onKeyDown?.(e);
  if (!e.stopped) on(surface).onKeyDown(e);
}

/** The one element under `root` that `match` finds. */
function only(root: FakeNode, what: string, match: (el: FakeElement) => boolean): FakeElement {
  const found = findAll(root, match);
  if (found.length !== 1) throw new Error(`found ${found.length} of ${what}`);
  return found[0];
}

const isRow = (label: string) => (el: FakeElement) =>
  el.getAttribute('role') === 'menuitem' && el.textContent.startsWith(label);

interface Mounted {
  /** The terminal menu. */
  menu: FakeElement;
  row: (label: string) => FakeElement;
  /** The Settings list, or null while it is shut. */
  list: () => FakeElement | null;
  /** A row of the Settings list. */
  listRow: (label: string) => FakeElement;
  /** Press a key where focus is, in the menu or the list. */
  key: (key: string) => Promise<void>;
  /** Press Esc, which goes to the surface opened last. */
  escape: () => Promise<void>;
  onClose: ReturnType<typeof vi.fn>;
}

const cleanups: (() => Promise<void> | void)[] = [];

async function mount(x = 100, y = 100): Promise<Mounted> {
  calls.log.length = 0;
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const onClose = vi.fn(() => void calls.log.push('close'));
  // The main window puts the menu on the escape stack while it is open.
  const unescape = pushEscape(onClose);
  await act(async () => {
    root.render(createElement(TerminalMenu, { x, y, ...props, onClose }));
  });
  cleanups.push(async () => {
    unescape();
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const menu = only(container, 'the menu', (el) => el.getAttribute('aria-label') === 'Terminal');
  const list = () =>
    findAll(
      doc.body,
      (el) => el.nodeName === 'MENU' && el.getAttribute('aria-label') === 'Settings',
    )[0] ?? null;
  const run = async (fn: () => void) => {
    await act(async () => fn());
  };
  return {
    menu,
    row: (label) => only(menu, label, isRow(label)),
    list,
    listRow: (label) => {
      const shown = list();
      if (!shown) throw new Error('the Settings list is shut');
      return only(shown, label, isRow(label));
    },
    key: (k) => run(() => press(k)),
    escape: () => {
      const stack = windowListeners.get('keydown');
      if (!stack) throw new Error('the escape stack is not listening');
      return run(() => stack(keyEvent('Escape')));
    },
    onClose,
  };
}

/** Arrow down from nothing lit to the Settings row, the eighth. */
async function downToSettings(m: Mounted) {
  for (let i = 0; i < 8; i++) await m.key('ArrowDown');
}

/** The row under focus is lit, and so is a row while its list is open. */
const lit = (el: FakeElement) =>
  doc.activeElement === el || el.getAttribute('aria-expanded') === 'true';

describe('the Settings list in the terminal menu', () => {
  beforeAll(async () => {
    teachTheDom();
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    Object.assign(doc, {
      addEventListener: (type: string, fn: Handler) => {
        const set = documentListeners.get(type) ?? new Set<Handler>();
        set.add(fn);
        documentListeners.set(type, set);
      },
      removeEventListener: (type: string, fn: Handler) => documentListeners.get(type)?.delete(fn),
      // The submenus open beside the menu, which a row the pointer
      // crosses on its way into one leaves open (menuAim.ts).
      querySelectorAll: (selector: string) => {
        if (selector !== '[data-menu-surface][data-menu-nested]')
          throw new Error(`no querySelectorAll for ${selector}`);
        return findAll(doc.body, (el) => el.hasAttribute('data-menu-nested'));
      },
    });
    doc.documentElement.dataset.platform = 'macos';
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      innerWidth: VW,
      innerHeight: VH,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener: (type: string, fn: Handler) => void windowListeners.set(type, fn),
      removeEventListener() {},
    });
    vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    // React DOM checks for a DOM once, when it loads.
    ({ createRoot } = await import('react-dom/client'));
  });

  afterEach(async () => {
    for (const cleanup of cleanups.splice(0)) await cleanup();
    boxes.clear();
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  it('opens from the keyboard on its first row and steps back out with ArrowLeft', async () => {
    const m = await mount();
    // The menu opens with focus on itself and no row lit.
    expect(doc.activeElement).toBe(m.menu);
    expect(m.list()).toBeNull();
    await downToSettings(m);
    const settings = m.row('Settings');
    expect(lit(settings)).toBe(true);
    expect(m.list()).toBeNull();

    await m.key('ArrowRight');
    expect(m.list()).not.toBeNull();
    expect(settings.getAttribute('aria-expanded')).toBe('true');
    expect(settings.getAttribute('aria-controls')).toBe(m.list()?.getAttribute('id'));
    expect(doc.activeElement).toBe(m.listRow('Triggers'));

    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.listRow('Aliases'));
    await m.key('End');
    expect(doc.activeElement).toBe(m.listRow('Help'));

    await m.key('ArrowLeft');
    expect(m.list()).toBeNull();
    expect(doc.activeElement).toBe(settings);
    expect(settings.getAttribute('aria-expanded')).toBe('false');
    expect(m.onClose).not.toHaveBeenCalled();
  });

  it('lights a row that takes focus, as Show me gives Customize prompt…, so Enter picks it', async () => {
    const m = await mount();
    const row = m.row('Customize prompt…');
    await act(async () => row.focus());
    expect(lit(row)).toBe(true);
    // In the page, Enter on the row under focus clicks it.
    await act(async () => on(row).onClick());
    expect(m.onClose).toHaveBeenCalledTimes(1);
  });

  it('opens on Enter and Space too, and ArrowLeft on its row shuts it', async () => {
    const m = await mount();
    await downToSettings(m);
    for (const k of ['Enter', ' ']) {
      await m.key(k);
      expect(m.list(), k).not.toBeNull();
      expect(doc.activeElement, k).toBe(m.listRow('Triggers'));
      await m.key('ArrowLeft');
      expect(m.list(), k).toBeNull();
    }
    // Opened by pointing, the list leaves focus on its row, where
    // ArrowLeft shuts it.
    await act(async () => point(m.row('Settings')));
    expect(m.list()).not.toBeNull();
    expect(doc.activeElement).toBe(m.row('Settings'));
    await m.key('ArrowLeft');
    expect(m.list()).toBeNull();
    expect(doc.activeElement).toBe(m.row('Settings'));
    expect(m.onClose).not.toHaveBeenCalled();
  });

  it('opens on no other row', async () => {
    const m = await mount();
    for (let i = 0; i < 9; i++) {
      await m.key('ArrowDown');
      if (lit(m.row('Settings'))) continue;
      await m.key('ArrowRight');
      expect(m.list()).toBeNull();
    }
  });

  it('closes one level per Esc, the list and then the menu', async () => {
    const m = await mount();
    await downToSettings(m);
    await m.key('ArrowRight');
    await m.escape();
    expect(m.list()).toBeNull();
    expect(doc.activeElement).toBe(m.row('Settings'));
    expect(m.onClose).not.toHaveBeenCalled();
    await m.escape();
    expect(m.onClose).toHaveBeenCalledTimes(1);
  });

  it('closes one level per Esc when pointing opened the list', async () => {
    const m = await mount();
    await act(async () => point(m.row('Settings')));
    await m.escape();
    expect(m.list()).toBeNull();
    expect(m.onClose).not.toHaveBeenCalled();
    await m.escape();
    expect(m.onClose).toHaveBeenCalledTimes(1);
  });

  it('opens when you point at Settings, leaving focus on the row', async () => {
    const m = await mount();
    await act(async () => point(m.row('Settings')));
    expect(m.list()).not.toBeNull();
    expect(doc.activeElement).toBe(m.row('Settings'));
    // ArrowRight then moves focus into the list already open.
    await m.key('ArrowRight');
    expect(doc.activeElement).toBe(m.listRow('Triggers'));
    // Pointing at another row shuts it and lights that row.
    await act(async () => point(m.row('Find in scrollback…')));
    expect(m.list()).toBeNull();
    expect(lit(m.row('Find in scrollback…'))).toBe(true);
    // So do the arrow keys leaving Settings.
    await act(async () => point(m.row('Settings')));
    await m.key('ArrowDown');
    expect(m.list()).toBeNull();
    expect(lit(m.row('Clear scrollback'))).toBe(true);
  });

  it('stays open while the pointer crosses Clear scrollback on its way into the list', async () => {
    vi.useFakeTimers();
    try {
      const m = await mount();
      await act(async () => point(m.row('Settings')));
      const list = m.list();
      expect(list).not.toBeNull();
      boxes.set(list as FakeElement, { left: 340, top: 100, right: 500, bottom: 498 });
      // Down and right from Settings toward the list's lower rows.
      const move = (clientX: number, clientY: number) => {
        for (const fn of documentListeners.get('pointermove') ?? []) fn({ clientX, clientY });
      };
      move(200, 250);
      move(216, 262);
      await act(async () => point(m.row('Clear scrollback')));
      expect(m.list()).not.toBeNull();
      expect(lit(m.row('Settings'))).toBe(true);
      // The pointer rests on Clear scrollback, which then takes over.
      await act(async () => void vi.advanceTimersByTime(300));
      expect(m.list()).toBeNull();
      expect(lit(m.row('Clear scrollback'))).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('keeps Settings the row the keys act on after pointing opens its list', async () => {
    const m = await mount();
    // The pointer opens the list, then stops in the gap or the list
    // padding, or moves past the list, without landing on a list row.
    // Settings keeps focus, so it stays lit.
    const pointAt = async () => {
      await act(async () => point(m.row('Settings')));
      expect(m.list()).not.toBeNull();
      expect(lit(m.row('Settings'))).toBe(true);
      expect(doc.activeElement).toBe(m.row('Settings'));
    };

    // ArrowRight, Enter and Space move into the list on its first row.
    for (const k of ['ArrowRight', 'Enter', ' ']) {
      await pointAt();
      await m.key(k);
      expect(doc.activeElement, k).toBe(m.listRow('Triggers'));
      await m.key('ArrowLeft');
      expect(m.list(), k).toBeNull();
    }
    expect(m.onClose).not.toHaveBeenCalled();

    // ArrowDown goes on to the row after Settings, not back to the top.
    await pointAt();
    await m.key('ArrowDown');
    expect(m.list()).toBeNull();
    expect(lit(m.row('Clear scrollback'))).toBe(true);
    expect(lit(m.row('Settings'))).toBe(false);
  });

  it('counts a press in the list as inside the menu', async () => {
    const m = await mount();
    await act(async () => point(m.row('Settings')));
    const press = (target: FakeNode) =>
      act(async () => {
        for (const fn of documentListeners.get('pointerdown') ?? []) fn({ target });
      });
    await press(m.listRow('Macros'));
    await press(m.list() as FakeElement);
    expect(m.onClose).not.toHaveBeenCalled();
    await press(doc.body);
    expect(m.onClose).toHaveBeenCalledTimes(1);
  });

  it('draws each row of the list, split in three', async () => {
    const m = await mount();
    await act(async () => point(m.row('Settings')));
    const list = m.list() as FakeElement;
    const shown = list.childNodes
      .filter((li): li is FakeElement => li instanceof FakeElement)
      .map((li) => (li.getAttribute('role') === 'separator' ? '---' : li.textContent));
    expect(shown).toEqual([
      'Triggers',
      'Aliases',
      'Macros',
      'Timers',
      '---',
      'General',
      'Appearance',
      'Layout',
      'Input',
      'Automation',
      'Scripts',
      'Characters',
      '---',
      // Help shows its shortcut, ⌘/ on macOS.
      'Help⌘/',
    ]);
  });

  it('clears the scrollback through the backend, and xterm clears its own buffer', async () => {
    const m = await mount();
    calls.log.length = 0;
    await act(async () => on(m.row('Clear scrollback')).onClick());
    expect(calls.log).toEqual(['close', 'invoke scrollback_clear']);
  });

  it('opens Save a scene in Settings, under Find in scrollback', async () => {
    const m = await mount();
    store.delete(SETTINGS_PENDING_KEY);
    calls.log.length = 0;
    await act(async () => on(m.row('Save a scene…')).onClick());
    expect(calls.log).toEqual([
      'close',
      `emit ${SETTINGS_GOTO_TAB} general:scene`,
      'invoke open_settings_window',
    ]);
    expect(store.get(SETTINGS_PENDING_KEY)).toBe('general:scene');
  });

  it('closes the menu, then opens Settings on each row, or Help', async () => {
    for (const row of SETTINGS_MENU.flat()) {
      const m = await mount();
      await act(async () => point(m.row('Settings')));
      store.delete(SETTINGS_PENDING_KEY);
      calls.log.length = 0;
      await act(async () => on(m.listRow(row.label)).onClick());
      if (row.link === null) {
        expect(calls.log, row.label).toEqual(['close', 'invoke open_help_window']);
        expect(store.has(SETTINGS_PENDING_KEY), row.label).toBe(false);
      } else {
        // The same path the palette takes: a target for a cold open,
        // the goto event for a window already up, then the window.
        expect(calls.log, row.label).toEqual([
          'close',
          `emit ${SETTINGS_GOTO_TAB} ${row.link}`,
          'invoke open_settings_window',
        ]);
        expect(store.get(SETTINGS_PENDING_KEY), row.label).toBe(row.link);
      }
    }
  });

  describe('where the list opens', () => {
    /** Open the list from a menu at `x`, `y`, with the Settings row 6
     *  in and 6 rows down, and return where the list sits. */
    async function listAt(x: number, y: number) {
      const m = await mount(x, y);
      const left = Number.parseFloat(String(m.menu.style.left));
      const top = Number.parseFloat(String(m.menu.style.top));
      boxes.set(m.menu, { left, top, right: left + MENU.w, bottom: top + MENU.h });
      const rowTop = top + 6 + 5 * 30 + 4 * 13;
      boxes.set(m.row('Settings'), {
        left: left + 6,
        top: rowTop,
        right: left + MENU.w - 6,
        bottom: rowTop + 30,
      });
      await downToSettings(m);
      await m.key('ArrowRight');
      const list = m.list() as FakeElement;
      return {
        menu: { left, top },
        rowTop,
        list: {
          left: Number.parseFloat(String(list.style.left)),
          top: Number.parseFloat(String(list.style.top)),
        },
      };
    }

    it('opens right of the menu, its first row level with Settings', async () => {
      const { menu, rowTop, list } = await listAt(100, 100);
      expect(list).toEqual({ left: menu.left + MENU.w + 4, top: rowTop - 6 });
    });

    it('opens left of the menu at the right edge', async () => {
      // The list has no room right of a menu at 1000.
      const { menu, list } = await listAt(1000, 100);
      expect(menu.left).toBe(1000);
      expect(list.left).toBe(menu.left - 4 - LIST.w);
    });

    it('rises from Settings at the bottom edge', async () => {
      // The menu itself clamps to 8 above the bottom edge.
      const { menu, rowTop, list } = await listAt(100, 700);
      expect(menu.top).toBe(VH - 8 - MENU.h);
      expect(list.top).toBe(rowTop + 30 + 6 - LIST.h);
    });

    it('opens left and up in the bottom right corner', async () => {
      const { menu, rowTop, list } = await listAt(1200, 760);
      expect(menu).toEqual({ left: VW - 8 - MENU.w, top: VH - 8 - MENU.h });
      expect(list).toEqual({ left: menu.left - 4 - LIST.w, top: rowTop + 30 + 6 - LIST.h });
    });
  });
});
