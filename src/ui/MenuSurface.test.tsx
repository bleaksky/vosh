import { act, createElement, type ReactElement, type ReactNode } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import { HOLD_MS, notePointer, resetMenuAim } from './menuAim';
import type { MenuPlacement, MenuPlacer } from './menuPlacement';
import { MenuItem, MenuSurface } from './MenuSurface';

// MenuItem uses no hooks, so a test can call it and hand its button the
// events a player sends, with no page to render into.

interface ButtonProps {
  role: string;
  className: string;
  'aria-checked'?: boolean;
  'aria-disabled'?: boolean;
  children: ReactNode[];
  onKeyDown: (e: { key: string; preventDefault: () => void; stopPropagation: () => void }) => void;
  onClick: () => void;
  onPointerEnter: (e: { currentTarget: FakeRow }) => void;
  onPointerMove: (e: { currentTarget: FakeRow }) => void;
  onPointerLeave: (e: { currentTarget: FakeRow }) => void;
}

/** The button a pointer event lands on. */
interface FakeRow {
  focus: () => void;
  getAttribute: (name: string) => string | null;
}

// The page the row reads: what has focus, and the submenus open beside
// the menu, none unless a test opens one.
const page = {
  activeElement: null as unknown,
  platform: 'macos',
  submenus: [] as {
    id: string;
    box: { left: number; right: number; top: number; bottom: number };
  }[],
};

beforeEach(() => {
  resetMenuAim();
  page.activeElement = null;
  page.submenus = [];
  page.platform = 'macos';
  vi.stubGlobal('document', {
    documentElement: {
      dataset: {
        get platform() {
          return page.platform;
        },
      },
    },
    get activeElement() {
      return page.activeElement;
    },
    querySelectorAll: () =>
      page.submenus.map((m) => ({
        id: m.id,
        contains: () => false,
        getBoundingClientRect: () => m.box,
      })),
  });
});
afterEach(() => vi.unstubAllGlobals());

function fakeRow(): FakeRow {
  const el: FakeRow = {
    focus: vi.fn(() => {
      page.activeElement = el;
    }),
    getAttribute: () => null,
  };
  return el;
}

/** The row's button and a spy on its submenu. */
function row(disabled: boolean) {
  const onOpen = vi.fn();
  const onSelect = vi.fn();
  const li = MenuItem({
    children: 'Marker',
    disabled,
    onSelect,
    submenu: { open: false, controls: 'pane-menu-marker', onOpen },
  }) as ReactElement<{ children: ReactElement<ButtonProps> }>;
  const button = li.props.children.props;
  const key = (k: string) =>
    button.onKeyDown({ key: k, preventDefault: vi.fn(), stopPropagation: vi.fn() });
  return { button, key, onOpen, onSelect };
}

describe('MenuItem', () => {
  it('opens its submenu from the keyboard, a click, or the pointer', () => {
    for (const k of ['ArrowRight', 'Enter', ' ']) {
      const { key, onOpen } = row(false);
      key(k);
      expect(onOpen, k).toHaveBeenCalledWith(true);
    }
    const { button, onOpen } = row(false);
    button.onClick();
    button.onPointerEnter({ currentTarget: fakeRow() });
    expect(onOpen.mock.calls).toEqual([[true], [false]]);
  });

  it('keeps a disabled row shut, even when a click left it focused', () => {
    const { button, key, onOpen, onSelect } = row(true);
    expect(button['aria-disabled']).toBe(true);
    for (const k of ['ArrowRight', 'Enter', ' ']) key(k);
    button.onClick();
    button.onPointerEnter({ currentTarget: fakeRow() });
    expect(onOpen).not.toHaveBeenCalled();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('takes the highlight when the pointer reaches it', () => {
    const { button } = row(false);
    const el = fakeRow();
    button.onPointerMove({ currentTarget: el });
    expect(el.focus).toHaveBeenCalledOnce();
  });
});

/** The button a row renders. */
function button(props: Parameters<typeof MenuItem>[0]): ButtonProps {
  const li = MenuItem(props) as ReactElement<{ children: ReactElement<ButtonProps> }>;
  return li.props.children.props;
}

describe('MenuItem rows', () => {
  it('draws its shortcut in the platform glyphs after the label', () => {
    const mac = button({ children: 'Find', keys: 'Mod+F' });
    const [label, keys] = mac.children as ReactElement<{
      className: string;
      children: string;
    }>[];
    expect(label.props.className).toBe('menu-label');
    expect(keys.type).toBe('kbd');
    expect(keys.props).toEqual({ className: 'menu-keys', children: '⌘F' });
    page.platform = 'windows';
    const pc = button({ children: 'Find', keys: 'Mod+F' }).children as ReactElement<{
      children: string;
    }>[];
    expect(pc[1].props.children).toBe('Ctrl+F');
    expect(button({ children: 'Find' }).children[1]).toBeFalsy();
  });

  it('draws a danger row in its own tone', () => {
    expect(button({ children: 'Close pane', danger: true }).className).toBe('menu-item is-danger');
    expect(button({ children: 'Close pane' }).className).toBe('menu-item');
  });

  it('reads out a radio row as picked or not, and a check row as checked', () => {
    const on = button({ children: 'Ledger', radio: true, checked: true });
    expect([on.role, on['aria-checked']]).toEqual(['menuitemradio', true]);
    const off = button({ children: 'Gauges', radio: true });
    expect([off.role, off['aria-checked']]).toEqual(['menuitemradio', false]);
    const check = button({ children: 'Timestamps', checked: false });
    expect([check.role, check['aria-checked']]).toEqual(['menuitemcheckbox', false]);
    const plain = button({ children: 'Copy' });
    expect([plain.role, plain['aria-checked']]).toEqual(['menuitem', undefined]);
  });
});

describe('MenuItem on the way into a submenu', () => {
  // A row under the one that opened a submenu, with the submenu to the
  // right of the menu, the way the writing card's New and Sent sit.
  const SUB = { id: 'wr-sub-new', box: { left: 370, right: 530, top: 314, bottom: 603 } };

  function sibling() {
    const onHover = vi.fn();
    const onOpen = vi.fn();
    const plain = MenuItem({ children: 'Close pane', onHover }) as ReactElement<{
      children: ReactElement<ButtonProps>;
    }>;
    const opener = MenuItem({
      children: 'Sent',
      submenu: { open: false, controls: 'wr-sub-sent', onOpen },
    }) as ReactElement<{ children: ReactElement<ButtonProps> }>;
    return {
      plain: plain.props.children.props,
      opener: opener.props.children.props,
      onHover,
      onOpen,
    };
  }

  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('leaves the submenu open while the pointer crosses a row toward it', () => {
    page.submenus = [SUB];
    const { plain, opener, onHover, onOpen } = sibling();
    const el = fakeRow();
    notePointer(194, 335);
    notePointer(210, 352);
    plain.onPointerEnter({ currentTarget: el });
    plain.onPointerMove({ currentTarget: el });
    plain.onPointerLeave({ currentTarget: el });
    const next = fakeRow();
    notePointer(226, 368);
    opener.onPointerEnter({ currentTarget: next });
    opener.onPointerLeave({ currentTarget: next });
    vi.advanceTimersByTime(HOLD_MS);
    expect(onHover).not.toHaveBeenCalled();
    expect(onOpen).not.toHaveBeenCalled();
    expect(el.focus).not.toHaveBeenCalled();
    expect(next.focus).not.toHaveBeenCalled();
  });

  it('takes over once the pointer rests on the row', () => {
    page.submenus = [SUB];
    const { opener, onOpen } = sibling();
    const el = fakeRow();
    notePointer(194, 335);
    notePointer(210, 352);
    opener.onPointerEnter({ currentTarget: el });
    expect(onOpen).not.toHaveBeenCalled();
    vi.advanceTimersByTime(HOLD_MS);
    expect(onOpen).toHaveBeenCalledWith(false);
    expect(el.focus).toHaveBeenCalledOnce();
  });

  it('takes over at once when the pointer heads away from the submenu', () => {
    page.submenus = [SUB];
    const { plain, onHover } = sibling();
    notePointer(194, 335);
    notePointer(194, 365);
    plain.onPointerEnter({ currentTarget: fakeRow() });
    expect(onHover).toHaveBeenCalledOnce();
  });
});

// ── The surface, mounted ────────────────────────────────────────────
// React DOM mounts the surface on a stand in DOM (src/test/fakeDom.ts).
// The stand in learns here the few calls the surface makes beyond what
// React DOM needs: its size, contains, a parent element, and the selectors it finds rows
// and fields with. The window keeps its listeners, so a test can send a
// resize or a blur, and a slot's ResizeObserver can be told its slot
// changed size.

const ROW_SELECTOR = '[role^="menuitem"]:not([aria-disabled="true"]):not(:disabled)';
const FIELD_SELECTOR = 'input, select, button';

type Handler = (e?: unknown) => void;
const windowListeners = new Map<string, Set<Handler>>();
const observers: { slot: unknown; changed: () => void }[] = [];
const doc = new FakeDocument();
const fakeWindow = {
  document: doc,
  innerWidth: 1280,
  innerHeight: 800,
  location: { protocol: 'about:' },
  HTMLIFrameElement: class {},
  addEventListener: (type: string, fn: Handler) => {
    const set = windowListeners.get(type) ?? new Set<Handler>();
    set.add(fn);
    windowListeners.set(type, set);
  },
  removeEventListener: (type: string, fn: Handler) => windowListeners.get(type)?.delete(fn),
};
class FakeResizeObserver {
  constructor(private readonly changed: () => void) {}
  observe(slot: unknown) {
    observers.push({ slot, changed: this.changed });
  }
  disconnect() {
    for (let i = observers.length - 1; i >= 0; i--)
      if (observers[i].changed === this.changed) observers.splice(i, 1);
  }
}
let createRoot: typeof import('react-dom/client').createRoot;

function teachTheDom() {
  const node = FakeNode.prototype as unknown as Record<string, unknown>;
  node.contains = function (this: FakeNode, other: FakeNode | null): boolean {
    for (let n = other; n; n = n.parentNode) if (n === this) return true;
    return false;
  };
  const found = (root: FakeElement, selector: string): FakeElement[] => {
    if (selector === ROW_SELECTOR)
      return findAll(
        root,
        (el) =>
          (el.getAttribute('role') ?? '').startsWith('menuitem') &&
          el.getAttribute('aria-disabled') !== 'true' &&
          !el.hasAttribute('disabled'),
      );
    if (selector === FIELD_SELECTOR)
      return findAll(root, (el) => ['INPUT', 'SELECT', 'BUTTON'].includes(el.nodeName));
    throw new Error(`no selector ${selector}`);
  };
  const el = FakeElement.prototype as unknown as Record<string, unknown>;
  el.querySelectorAll = function (this: FakeElement, selector: string) {
    return found(this, selector);
  };
  el.querySelector = function (this: FakeElement, selector: string) {
    return found(this, selector)[0] ?? null;
  };
  Object.defineProperty(FakeElement.prototype, 'parentElement', {
    configurable: true,
    get(this: FakeElement) {
      return this.parentNode instanceof FakeElement ? this.parentNode : null;
    },
  });
  // A menu 240 by 200.
  Object.defineProperty(FakeElement.prototype, 'offsetWidth', { configurable: true, value: 240 });
  Object.defineProperty(FakeElement.prototype, 'offsetHeight', { configurable: true, value: 200 });
}

function stubTheDom() {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', fakeWindow);
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  vi.stubGlobal('ResizeObserver', FakeResizeObserver);
}

/** The handlers React keeps on an element. This DOM sends no events. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

function keyEvent(key: string, mods: { metaKey?: boolean; ctrlKey?: boolean } = {}) {
  return {
    key,
    metaKey: false,
    ctrlKey: false,
    ...mods,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  };
}

const cleanups: (() => Promise<void>)[] = [];

interface MountOptions {
  at?: MenuPlacement | MenuPlacer;
  anchor?: FakeElement;
  anchored?: boolean;
  focus?: 'first' | 'surface';
  kind?: 'menu' | 'dialog';
  keepKeys?: boolean;
  children?: ReactNode;
}

async function mount(options: MountOptions = {}) {
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const onClose = vi.fn();
  const { at = { x: 100, y: 100 }, children, anchor, ...rest } = options;
  await act(async () => {
    root.render(
      createElement(MenuSurface, {
        label: 'Pane',
        at,
        onClose,
        ...(anchor && { anchor: anchor as unknown as HTMLElement }),
        ...rest,
        children: children ?? [
          createElement(MenuItem, { key: 'a', children: 'Copy' }),
          createElement(MenuItem, { key: 'b', children: 'Close pane' }),
        ],
      }),
    );
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const surface = findAll(doc.body, (el) => el.getAttribute('aria-label') === 'Pane').at(-1);
  if (!surface) throw new Error('no surface');
  const rows = findAll(surface, (el) => el.getAttribute('role') === 'menuitem');
  const send = (type: string) =>
    act(async () => {
      for (const fn of windowListeners.get(type) ?? []) fn();
    });
  const key = async (k: string, mods?: { metaKey?: boolean; ctrlKey?: boolean }) => {
    const e = keyEvent(k, mods);
    await act(async () => on(surface).onKeyDown(e));
    return e;
  };
  return { surface, rows, onClose, send, key };
}

describe('MenuSurface', () => {
  beforeAll(async () => {
    teachTheDom();
    stubTheDom();
    // React DOM checks for a DOM once, when it loads.
    ({ createRoot } = await import('react-dom/client'));
  });
  beforeEach(() => {
    stubTheDom();
    doc.activeElement = null;
  });
  afterEach(async () => {
    for (const cleanup of cleanups.splice(0)) await cleanup();
    windowListeners.clear();
    observers.length = 0;
  });

  it('lets a placer cap its height, and scrolls past it', async () => {
    const placer: MenuPlacer = (size, viewport) => ({
      left: viewport.width - size.width - 8,
      top: 40,
      maxHeight: 120,
    });
    const { surface } = await mount({ at: placer });
    expect(surface.style).toMatchObject({
      left: '1032px',
      top: '40px',
      maxHeight: '120px',
      overflowY: 'auto',
    });
    const plain = await mount();
    expect(plain.surface.style.maxHeight).toBeUndefined();
    expect(plain.surface.style.overflowY).toBeUndefined();
  });

  it('follows its button when anchored, and stays open on a resize or a blur', async () => {
    const slot = doc.createElement('div');
    const anchor = doc.createElement('button');
    slot.appendChild(anchor);
    let left = 300;
    const { surface, send, onClose } = await mount({
      at: () => ({ left, top: 44 }),
      anchor,
      anchored: true,
    });
    expect(surface.style.left).toBe('300px');
    left = 420;
    await send('resize');
    expect(surface.style.left).toBe('420px');
    left = 380;
    const watching = observers.find((o) => o.slot === slot);
    expect(watching).toBeDefined();
    await act(async () => watching?.changed());
    expect(surface.style.left).toBe('380px');
    await send('blur');
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes on a resize or a blur when not anchored', async () => {
    for (const type of ['resize', 'blur']) {
      const { send, onClose } = await mount();
      await send(type);
      expect(onClose, type).toHaveBeenCalledWith('outside');
      for (const cleanup of cleanups.splice(0)) await cleanup();
      windowListeners.clear();
    }
  });

  it('focuses its first row, or itself with no row lit', async () => {
    const first = await mount();
    expect(doc.activeElement).toBe(first.rows[0]);
    for (const cleanup of cleanups.splice(0)) await cleanup();
    doc.activeElement = null;

    const { surface, rows, key } = await mount({ focus: 'surface' });
    expect(surface.getAttribute('tabindex')).toBe('-1');
    expect(doc.activeElement).toBe(surface);
    await key('ArrowDown');
    expect(doc.activeElement).toBe(rows[0]);
  });

  it('as a dialog focuses its first field and leaves the arrows to it', async () => {
    const { surface, key } = await mount({
      kind: 'dialog',
      children: [
        createElement('p', { key: 'p' }, 'Name the session'),
        createElement('input', { key: 'i', 'aria-label': 'Name' }),
        createElement('button', { key: 'b', type: 'button' }, 'Create'),
      ],
    });
    expect(surface.nodeName).toBe('DIV');
    expect(surface.getAttribute('role')).toBe('dialog');
    const field = findAll(surface, (el) => el.nodeName === 'INPUT')[0];
    expect(doc.activeElement).toBe(field);
    for (const k of ['ArrowDown', 'ArrowUp', 'Home', 'End', 'Tab']) {
      const e = await key(k);
      expect(e.preventDefault, k).not.toHaveBeenCalled();
      expect(doc.activeElement, k).toBe(field);
    }
  });

  it('keeps its keys from what renders it, all but Esc and the shortcuts', async () => {
    const { key } = await mount({ keepKeys: true });
    expect((await key('Delete')).stopPropagation).toHaveBeenCalled();
    expect((await key('Escape')).stopPropagation).not.toHaveBeenCalled();
    expect((await key('k', { metaKey: true })).stopPropagation).not.toHaveBeenCalled();
    expect((await key('k', { ctrlKey: true })).stopPropagation).not.toHaveBeenCalled();
    for (const cleanup of cleanups.splice(0)) await cleanup();
    const loose = await mount();
    expect((await loose.key('Delete')).stopPropagation).not.toHaveBeenCalled();
  });
});
