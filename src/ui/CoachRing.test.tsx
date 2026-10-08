import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import overlaysCss from '../styles/overlays.css?raw';
import { FakeDocument, FakeElement, findAll } from '../test/fakeDom';
import type { Coach } from './coach';

// Show me's ring: it finds what to pick once it
// draws, moves focus there, rings it 2 px out with its line beside it,
// and goes at the pick, at Esc, at a press anywhere, or when the menu
// it rings closes.

type Handler = (e: unknown) => void;
const doc = new FakeDocument();
const listeners = new Map<string, Set<Handler>>();
let frames: (() => void)[] = [];
let createRoot: typeof import('react-dom/client').createRoot;
let CoachRing: typeof import('./CoachRing').CoachRing;
let ring: typeof import('./coach');

/** A row a menu draws, with the rect the ring reads. */
function row(left: number, top: number, width = 188, height = 30) {
  return {
    isConnected: true,
    focused: false,
    focus() {
      this.focused = true;
    },
    getBoundingClientRect: () => ({ left, top, right: left + width, bottom: top + height }),
  };
}
type Row = ReturnType<typeof row>;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  Object.assign(doc, {
    addEventListener: (type: string, fn: Handler) => {
      const set = listeners.get(type) ?? new Set<Handler>();
      set.add(fn);
      listeners.set(type, set);
    },
    removeEventListener: (type: string, fn: Handler) => listeners.get(type)?.delete(fn),
  });
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    innerWidth: 1280,
    innerHeight: 800,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('requestAnimationFrame', (cb: () => void) => frames.push(cb));
  vi.stubGlobal('cancelAnimationFrame', () => undefined);
  // The line measures 160 by 36, as Pick Chat or Group. does.
  Object.defineProperty(FakeElement.prototype, 'offsetWidth', {
    configurable: true,
    get: () => 160,
  });
  Object.defineProperty(FakeElement.prototype, 'offsetHeight', {
    configurable: true,
    get: () => 36,
  });
  ({ createRoot } = await import('react-dom/client'));
  ({ CoachRing } = await import('./CoachRing'));
  ring = await import('./coach');
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
  frames = [];
});

/** Run the frames waiting, as the browser would paint. */
const paint = (times = 1) =>
  act(async () => {
    for (let i = 0; i < times; i++) {
      const due = frames;
      frames = [];
      for (const cb of due) cb();
    }
  });

async function mount(coach: Coach) {
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => root.render(createElement(CoachRing)));
  await act(async () => ring.showCoach(coach));
  cleanups.push(async () => {
    await act(async () => ring.clearCoach());
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const find = (cls: string) =>
    findAll(container, (el) => (el.getAttribute('class') ?? '').split(' ').includes(cls))[0] as
      | FakeElement
      | undefined;
  return {
    ringEl: () => find('cm-ring'),
    tip: () => find('cm-tip'),
    fire: (type: string, event: unknown) =>
      act(async () => {
        for (const fn of listeners.get(type) ?? []) fn(event);
      }),
  };
}

describe('the coach ring', () => {
  it('rings Chat and Group together, 2 px out, and focuses the first', async () => {
    const group = row(980, 38);
    const chat = row(980, 68);
    const m = await mount({ find: () => [group, chat] as never, line: 'Pick Chat or Group.' });
    expect(m.ringEl()).toBeUndefined();
    await paint();
    expect(group.focused).toBe(true);
    expect(chat.focused).toBe(false);
    const style = m.ringEl()?.style;
    expect([style?.left, style?.top, style?.width, style?.height]).toEqual([
      '978px',
      '36px',
      '192px',
      '64px',
    ]);
    expect(m.tip()?.textContent).toBe('Pick Chat or Group.');
    expect([m.tip()?.style.left, m.tip()?.style.top]).toEqual(['802px', '50px']);
    expect(m.tip()?.getAttribute('role')).toBe('status');
  });

  it('waits for a menu that opens a frame later', async () => {
    let rows: Row[] = [];
    const m = await mount({ find: () => rows as never, line: 'Pick Customize prompt…' });
    await paint(3);
    expect(m.ringEl()).toBeUndefined();
    rows = [row(30, 458, 220, 30)];
    await paint();
    expect(rows[0].focused).toBe(true);
    expect(m.ringEl()).toBeDefined();
  });

  it('gives up when nothing to pick ever shows', async () => {
    const m = await mount({ find: () => [], line: 'Pick Chat or Group.' });
    await paint(60);
    expect(frames).toHaveLength(0);
    expect(m.ringEl()).toBeUndefined();
  });

  it('clears on Esc', async () => {
    const m = await mount({ find: () => [row(30, 458)] as never, line: 'Pick Customize prompt…' });
    await paint();
    await m.fire('keydown', { key: 'ArrowDown' });
    expect(m.ringEl()).toBeDefined();
    await m.fire('keydown', { key: 'Escape' });
    expect(m.ringEl()).toBeUndefined();
  });

  it('clears at the pick, or a press anywhere else', async () => {
    const m = await mount({ find: () => [row(30, 458)] as never, line: 'Pick Customize prompt…' });
    await paint();
    await m.fire('pointerdown', {});
    expect(m.ringEl()).toBeUndefined();
  });

  it('clears when the menu it rings closes, as a pick from the keyboard does', async () => {
    const target = row(30, 458);
    const m = await mount({ find: () => [target] as never, line: 'Pick Customize prompt…' });
    await paint();
    target.isConnected = false;
    await paint();
    expect(m.ringEl()).toBeUndefined();
  });

  it('sets its line right of the ring, or left where the window ends', () => {
    const tip = { width: 160, height: 36 };
    const view = { width: 1280, height: 800 };
    expect(ring.tipPlace({ left: 28, top: 456, width: 224, height: 34 }, tip, view)).toEqual({
      left: 268,
      top: 455,
    });
    expect(ring.tipPlace({ left: 978, top: 36, width: 192, height: 64 }, tip, view)).toEqual({
      left: 802,
      top: 50,
    });
    expect(ring.tipPlace({ left: 28, top: 780, width: 224, height: 34 }, tip, view).top).toBe(756);
  });

  it('pulses once, and not at all under reduced motion', () => {
    expect(overlaysCss).toMatch(/\.cm-ring \{[^}]*animation: cm-pulse 700ms ease-out 1;/);
    expect(overlaysCss).toMatch(
      /@media \(prefers-reduced-motion: reduce\) \{\s*\.cm-ring \{\s*animation: none;/,
    );
  });
});
