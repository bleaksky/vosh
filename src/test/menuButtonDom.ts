import { act, type ReactElement } from 'react';
import { afterAll, afterEach, beforeAll, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode, findAll } from './fakeDom';

// Mounts a menu button of the prompt card (MenuButton) with React DOM on
// the stand in DOM (fakeDom.ts), with the card's own menu. The stand in
// learns here the few calls the menu makes beyond what React DOM needs:
// the button's box and the menu's height to place it, contains for a
// press outside, isConnected for handing focus back, and the selector
// the menu finds its items with. The DOM sends no events, so a test
// calls React's handlers, and a press stands in for Enter and Space,
// which press a button.

type Handler = (e?: unknown) => void;

const ITEMS =
  '[role="menuitem"]:not(:disabled),[role="menuitemradio"]:not(:disabled),[role="menuitemcheckbox"]:not(:disabled)';

const ITEM_ROLES = ['menuitem', 'menuitemradio', 'menuitemcheckbox'];

/** A 1280 by 800 window. */
export const VIEWPORT = { width: 1280, height: 800 };

/** The box of every element, so of the button too, low in the card's
 *  foot. */
export const BUTTON = { left: 200, top: 700, right: 290, bottom: 728 };

/** A menu's height: 30 a row and 6 above and below. */
export const menuHeight = (rows: number) => rows * 30 + 12;

/** How many check marks an item of the menu draws, 1 for the current
 *  choice and 0 for the rest. */
export function checkMarks(item: FakeElement): number {
  return findAll(item, (e) => e.nodeName === 'SVG' && e.getAttribute('class') === 'menu-check')
    .length;
}

/** The handlers React keeps on an element. */
export function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

export interface MountedMenuButton {
  /** What the element drew into. */
  container: FakeElement;
  button: FakeElement;
  /** The menu, or null while it is shut. */
  menu: () => FakeElement | null;
  /** The menu's items, in order. */
  items: () => FakeElement[];
  item: (label: string) => FakeElement;
  /** The labels of the items checked. */
  checked: () => (string | null)[];
  press: () => Promise<void>;
  /** Press a key in the menu. */
  key: (key: string) => Promise<void>;
  /** Press Esc, which goes to the surface opened last. */
  escape: () => Promise<void>;
  /** Press somewhere outside the button and its menu. */
  pressOutside: () => Promise<void>;
  /** Draw again from another element. */
  update: (element: ReactElement) => Promise<void>;
}

/** Sets the stand in DOM up around the tests of one file, and hands back
 *  the document and a mount. Call it at the top of a describe. */
export function menuButtonDom() {
  const doc = new FakeDocument();
  const windowListeners = new Map<string, Handler>();
  const documentListeners = new Map<string, Set<Handler>>();
  const cleanups: (() => Promise<void>)[] = [];
  let createRoot: typeof import('react-dom/client').createRoot;

  const isItem = (e: FakeElement) =>
    ITEM_ROLES.includes(e.getAttribute('role') ?? '') && !e.hasAttribute('disabled');

  const teachTheDom = () => {
    const node = FakeNode.prototype as unknown as Record<string, unknown>;
    node.contains = function (this: FakeNode, other: FakeNode | null): boolean {
      for (let n = other; n; n = n.parentNode) if (n === this) return true;
      return false;
    };
    Object.defineProperty(FakeNode.prototype, 'isConnected', {
      configurable: true,
      get(this: FakeNode) {
        return (doc as unknown as { contains: (n: FakeNode) => boolean }).contains(this);
      },
    });
    const el = FakeElement.prototype as unknown as Record<string, unknown>;
    el.getBoundingClientRect = () => BUTTON;
    el.querySelectorAll = function (this: FakeElement, selector: string): FakeElement[] {
      if (selector !== ITEMS) throw new Error(`no querySelectorAll for ${selector}`);
      return findAll(this, isItem);
    };
    Object.defineProperty(FakeElement.prototype, 'offsetHeight', {
      configurable: true,
      get(this: FakeElement) {
        return menuHeight(findAll(this, isItem).length);
      },
    });
  };

  const keyEvent = (key: string) => ({
    key,
    isComposing: false,
    target: doc.activeElement,
    metaKey: false,
    ctrlKey: false,
    preventDefault() {},
    stopPropagation() {},
  });

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
    });
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      innerWidth: VIEWPORT.width,
      innerHeight: VIEWPORT.height,
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
    doc.activeElement = null;
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  const run = async (fn: () => void) => {
    await act(async () => fn());
  };

  /** Mounts `element` and finds the menu button in it: the first button,
   *  or the button `which` picks. */
  async function mount(
    element: ReactElement,
    which: (button: FakeElement) => boolean = () => true,
  ): Promise<MountedMenuButton> {
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    const render = (now: ReactElement) =>
      act(async () => {
        root.render(now);
      });
    await render(element);
    cleanups.push(async () => {
      await act(async () => root.unmount());
      doc.body.removeChild(container);
    });
    const [button] = findAll(container, (el) => el.nodeName === 'BUTTON' && which(el));
    if (!button) throw new Error('no button');
    const menu = () => findAll(container, (el) => el.getAttribute('role') === 'menu')[0] ?? null;
    const items = () => {
      const shown = menu();
      if (!shown) throw new Error('the menu is shut');
      return findAll(shown, (el) => el.getAttribute('role') === 'menuitemradio');
    };
    return {
      container,
      button,
      menu,
      items,
      item: (label) => {
        const found = items().filter((el) => el.textContent === label);
        if (found.length !== 1) throw new Error(`found ${found.length} of ${label}`);
        return found[0];
      },
      checked: () =>
        items()
          .filter((el) => el.getAttribute('aria-checked') === 'true')
          .map((el) => el.textContent),
      press: () => run(() => on(button).onClick({ currentTarget: button })),
      key: (k) => {
        const shown = menu();
        if (!shown) throw new Error('the menu is shut');
        return run(() => on(shown).onKeyDown(keyEvent(k)));
      },
      escape: () => {
        const stack = windowListeners.get('keydown');
        if (!stack) throw new Error('the escape stack is not listening');
        return run(() => stack(keyEvent('Escape')));
      },
      pressOutside: async () => {
        const outside = doc.createElement('div');
        doc.body.appendChild(outside);
        const presses = [...(documentListeners.get('pointerdown') ?? [])];
        if (presses.length === 0) throw new Error('nothing listens for a press outside');
        await run(() => {
          for (const fn of presses) fn({ target: outside });
        });
        doc.body.removeChild(outside);
      },
      update: (now) => render(now),
    };
  }

  return { doc, mount };
}
