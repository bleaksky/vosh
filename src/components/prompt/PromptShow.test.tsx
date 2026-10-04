import { act, createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { PromptShowState } from '../../lib/promptShow';
import type { PromptShow } from '../../lib/session';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../../test/fakeDom';
import { SHOW_MENU_WIDTH, ShowButton } from './PromptShow';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The button beside Draw your prompt at the foot of Customize prompt,
// and the menu it opens.

const reads: PromptShowState = {
  show: 'pinned',
  capture: true,
  draw: true,
  gameSent: true,
  zone: 1,
  promptsOff: false,
};

describe('the button that says where your prompt shows', () => {
  const draw = (value: PromptShow, state: PromptShowState | null = reads) =>
    renderToStaticMarkup(<ShowButton value={value} state={state} onChange={() => {}} />);

  it('reads the place your prompt shows now, with a chevron', () => {
    for (const [value, label] of [
      ['text', 'In the text'],
      ['lifted', 'Lifted'],
      ['pinned', 'Pinned'],
    ] as const) {
      const html = draw(value);
      expect(html, value).toMatch(
        new RegExp(
          `<button[^>]*class="st-button st-button-secondary pc-menu-button"[^>]*><span>${label}</span><svg`,
        ),
      );
      // A screen reader hears what the button picks and the place now.
      expect(html, value).toContain(`aria-label="Where your prompt shows, ${label}"`);
      expect(html, value).toContain('aria-haspopup="menu"');
      expect(html, value).toContain('aria-expanded="false"');
      expect(html, value).not.toContain('aria-disabled');
      expect(html, value).not.toContain('title=');
      // The menu waits for a press.
      expect(html, value).not.toContain('role="menu"');
    }
  });

  it('turns off and says why while the profile reads no prompt, as the Settings row does', () => {
    for (const [gameSent, why] of [
      [true, 'Customize your prompt first.'],
      [false, 'Tell Vosh your game&#x27;s prompt first.'],
    ] as const) {
      const html = draw('text', { ...reads, capture: false, gameSent });
      expect(html).toContain('aria-disabled="true"');
      expect(html).toContain(`title="${why}"`);
      const id = /aria-describedby="([^"]+)"/.exec(html)?.[1];
      expect(id).toBeTruthy();
      expect(html).toContain(`<span id="${id}" class="st-visually-hidden">${why}</span>`);
      // Off, not gone, so Tab still reaches it and a reader hears why.
      expect(html).not.toMatch(/<button[^>]*disabled=""/);
    }
  });

  it('waits, off with nothing to say, until Vosh knows whether the profile reads a prompt', () => {
    const html = draw('pinned', null);
    expect(html).toContain('aria-disabled="true"');
    expect(html).not.toContain('aria-describedby');
    expect(html).not.toContain('title=');
    expect(html).toContain('<span>Pinned</span>');
  });
});

// ── The menu, mounted ───────────────────────────────────────────────
// React DOM mounts the button on the stand in DOM (src/test/fakeDom.ts),
// with the card's own menu. The stand in learns here the few calls the
// menu makes beyond what React DOM needs: the button's box and the
// menu's height to place it, contains for a press outside, isConnected
// for handing focus back, and the selector the menu finds its items
// with. The DOM sends no events, so a test calls React's handlers, and
// a press stands in for Enter and Space, which press a button.

type Handler = (e?: unknown) => void;
const doc = new FakeDocument();
const windowListeners = new Map<string, Handler>();
const documentListeners = new Map<string, Set<Handler>>();
let createRoot: typeof import('react-dom/client').createRoot;

const ITEMS =
  '[role="menuitem"]:not(:disabled),[role="menuitemradio"]:not(:disabled),[role="menuitemcheckbox"]:not(:disabled)';

// A 1280 by 800 window with the button low in the card's foot.
const VW = 1280;
const VH = 800;
const BUTTON = { left: 200, top: 700, right: 290, bottom: 728 };
const MENU_H = 3 * 30 + 12;

function teachTheDom() {
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
    return findAll(
      this,
      (e) =>
        ['menuitem', 'menuitemradio', 'menuitemcheckbox'].includes(e.getAttribute('role') ?? '') &&
        !e.hasAttribute('disabled'),
    );
  };
  Object.defineProperty(FakeElement.prototype, 'offsetHeight', {
    configurable: true,
    get: () => MENU_H,
  });
}

/** The handlers React keeps on an element. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const keyEvent = (key: string) => ({
  key,
  isComposing: false,
  target: doc.activeElement,
  metaKey: false,
  ctrlKey: false,
  preventDefault() {},
  stopPropagation() {},
});

interface Mounted {
  button: FakeElement;
  /** The menu, or null while it is shut. */
  menu: () => FakeElement | null;
  /** The menu's items, in order. */
  items: () => FakeElement[];
  item: (label: string) => FakeElement;
  press: () => Promise<void>;
  /** Press a key in the menu. */
  key: (key: string) => Promise<void>;
  /** Press Esc, which goes to the surface opened last. */
  escape: () => Promise<void>;
  /** Draw the button again with another state. */
  update: (state: PromptShowState | null) => Promise<void>;
  onChange: ReturnType<typeof vi.fn>;
}

const cleanups: (() => Promise<void>)[] = [];

async function mount(
  value: PromptShow = 'pinned',
  state: PromptShowState | null = reads,
): Promise<Mounted> {
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const onChange = vi.fn();
  const render = (now: PromptShowState | null) =>
    act(async () => {
      root.render(createElement(ShowButton, { value, state: now, onChange }));
    });
  await render(state);
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const [button] = findAll(container, (el) => el.nodeName === 'BUTTON');
  if (!button) throw new Error('no button');
  const menu = () => findAll(container, (el) => el.getAttribute('role') === 'menu')[0] ?? null;
  const items = () => {
    const shown = menu();
    if (!shown) throw new Error('the menu is shut');
    return findAll(shown, (el) => el.getAttribute('role') === 'menuitemradio');
  };
  const run = async (fn: () => void) => {
    await act(async () => fn());
  };
  return {
    button,
    menu,
    items,
    item: (label) => {
      const found = items().filter((el) => el.textContent === label);
      if (found.length !== 1) throw new Error(`found ${found.length} of ${label}`);
      return found[0];
    },
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
    update: (now) => render(now),
    onChange,
  };
}

const label = (el: FakeElement) => el.textContent;
const checked = (m: Mounted) =>
  m
    .items()
    .filter((el) => el.getAttribute('aria-checked') === 'true')
    .map(label);
const checks = (el: FakeElement) =>
  findAll(el, (e) => e.nodeName === 'SVG' && e.getAttribute('class') === 'pane-menu-check').length;

describe('the menu of where your prompt shows', () => {
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
    doc.activeElement = null;
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  it('opens on a press with the three places, the current one checked on the right', async () => {
    const m = await mount('lifted');
    // A plain button, so Enter and Space press it as they press any.
    expect(m.button.getAttribute('type')).toBe('button');
    expect(on(m.button).onKeyDown).toBeUndefined();
    expect(m.menu()).toBeNull();

    await m.press();
    const menu = m.menu();
    expect(menu).not.toBeNull();
    expect(menu?.getAttribute('aria-label')).toBe('Where your prompt shows');
    expect(menu?.getAttribute('class')).toBe('pc-menu');
    expect(m.button.getAttribute('aria-expanded')).toBe('true');
    expect(m.items().map(label)).toEqual(['In the text', 'Lifted', 'Pinned']);
    for (const item of m.items()) {
      expect(item.getAttribute('class')).toBe('ov-menu-item');
      expect(checks(item), label(item)).toBe(label(item) === 'Lifted' ? 1 : 0);
    }
    expect(checked(m)).toEqual(['Lifted']);
    // It opens above the button, their left edges together, the narrow
    // pane menus' width, and takes focus.
    expect(menu?.style.width).toBe(`${SHOW_MENU_WIDTH}px`);
    expect(menu?.style.left).toBe(`${BUTTON.left}px`);
    expect(menu?.style.top).toBe(`${BUTTON.top - 4 - MENU_H}px`);
    expect(doc.activeElement).toBe(menu);

    // A second press shuts it.
    await m.press();
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('moves between the places with the arrow keys, and closes on Esc', async () => {
    const m = await mount('pinned');
    await m.press();
    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.item('In the text'));
    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.item('Lifted'));
    await m.key('ArrowUp');
    expect(doc.activeElement).toBe(m.item('In the text'));
    // Past either end it comes round.
    await m.key('ArrowUp');
    expect(doc.activeElement).toBe(m.item('Pinned'));
    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.item('In the text'));
    await m.key('End');
    expect(doc.activeElement).toBe(m.item('Pinned'));
    await m.key('Home');
    expect(doc.activeElement).toBe(m.item('In the text'));

    await m.escape();
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('closes on Tab, leaving the place as it was', async () => {
    const m = await mount('text');
    await m.press();
    await m.key('ArrowDown');
    await m.key('Tab');
    expect(m.menu()).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('closes on a press outside it', async () => {
    const m = await mount('text');
    await m.press();
    const outside = doc.createElement('div');
    doc.body.appendChild(outside);
    const presses = [...(documentListeners.get('pointerdown') ?? [])];
    expect(presses.length).toBeGreaterThan(0);
    await act(async () => {
      for (const fn of presses) fn({ target: outside });
    });
    expect(m.menu()).toBeNull();
    doc.body.removeChild(outside);
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('saves the place you pick and hands focus back to the button', async () => {
    const m = await mount('pinned');
    await m.press();
    await m.key('ArrowDown');
    await m.key('ArrowDown');
    const lifted = m.item('Lifted');
    expect(doc.activeElement).toBe(lifted);
    // Enter and Space press the item, as a click does.
    await act(async () => on(lifted).onClick());
    expect(m.onChange).toHaveBeenCalledTimes(1);
    expect(m.onChange).toHaveBeenCalledWith('lifted');
    expect(m.menu()).toBeNull();
    expect(doc.activeElement).toBe(m.button);
  });

  it('closes without a save when you pick the place your prompt shows now', async () => {
    const m = await mount('pinned');
    await m.press();
    await act(async () => on(m.item('Pinned')).onClick());
    expect(m.menu()).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
    expect(doc.activeElement).toBe(m.button);
  });

  it('opens no menu while the profile reads no prompt', async () => {
    const m = await mount('text', { ...reads, capture: false });
    expect(m.button.getAttribute('aria-disabled')).toBe('true');
    await m.press();
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    const waiting = await mount('text', null);
    await waiting.press();
    expect(waiting.menu()).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('shuts an open menu when the profile stops reading a prompt', async () => {
    const m = await mount('lifted');
    await m.press();
    expect(m.menu()).not.toBeNull();
    await m.update({ ...reads, capture: false, gameSent: false });
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    expect(m.button.getAttribute('title')).toBe("Tell Vosh your game's prompt first.");
    // It stays shut when the profile reads one again.
    await m.update(reads);
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-disabled')).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
  });
});
