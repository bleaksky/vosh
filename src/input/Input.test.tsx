import { act, createElement } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';

// The command line starts with the mark your commands echo with while
// Use the same mark in the command line is on, and with no mark at all
// while the mark is off or the switch is. The row draws the look you
// pick, the caret and text colors, the background and the size. React
// DOM mounts the command line on a stand in DOM (src/test/fakeDom.ts),
// and each setting reaches it as Settings sends it, through a fake
// event bus.

type Handler = (event: { payload: unknown }) => void;
const bus = vi.hoisted(() => ({ handlers: new Map<string, Set<Handler>>() }));

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = bus.handlers.get(event);
    if (!set) bus.handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string) =>
    cmd === 'ui_get_config'
      ? {
          tracked_affects: [],
          input_caret_blink: true,
          input_caret_color: null,
          input_line_color: null,
          input_line_background: 'theme',
          input_line_background_color: null,
          input_line_size: 0,
        }
      : null,
}));

function fire(event: string, payload: unknown): void {
  for (const cb of bus.handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let Input: typeof import('./Input').Input;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    requestAnimationFrame: () => 0,
    cancelAnimationFrame() {},
    setTimeout,
    clearTimeout,
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  vi.stubGlobal('HTMLTextAreaElement', FakeElement);
  vi.stubGlobal('getComputedStyle', () => ({ fontSize: '14px' }));
  // The caret measures where the stand in DOM lays nothing out, so every
  // box sits at the origin.
  for (const name of [
    'selectionStart',
    'selectionEnd',
    'offsetLeft',
    'offsetTop',
    'offsetHeight',
    'scrollLeft',
    'scrollTop',
    'clientWidth',
  ]) {
    Object.defineProperty(FakeElement.prototype, name, { value: 0, configurable: true });
  }
  ({ createRoot } = await import('react-dom/client'));
  ({ Input } = await import('./Input'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

/** The inline style React set on `el`, as name:value pairs. */
function styleOf(el: FakeElement | undefined): string {
  return Object.entries(el?.style ?? {})
    .filter(([, value]) => typeof value === 'string' && value !== '')
    .map(([name, value]) => `${name}:${String(value)}`)
    .join(';');
}

/** Mount the command line, and read the mark it draws as its class and
 *  text, or null with no mark, and the row and its caret. `enabled`
 *  focuses the line, so it draws the caret. */
async function mountLine(enabled = false) {
  const host = doc.createElement('div');
  const root = createRoot(host as unknown as HTMLElement);
  await act(async () => {
    root.render(
      createElement(Input, {
        enabled,
        macroKeys: { command: () => undefined, bound: () => false },
      }),
    );
    await settle();
  });
  const mark = () => {
    const spans = findAll(host, (el) => /\bprompt\b/.test(el.getAttribute('class') ?? ''));
    return spans.map((el) => `${el.getAttribute('class')}|${el.textContent}`).join(',') || null;
  };
  const byClass = (name: string) =>
    findAll(host, (el) => (el.getAttribute('class') ?? '').split(' ').includes(name))[0];
  return {
    mark,
    row: () => byClass('input-row')?.getAttribute('class'),
    rowStyle: () => styleOf(byClass('input-row')),
    caret: () => byClass('input-caret')?.getAttribute('class'),
    unmount: () => act(async () => root.unmount()),
  };
}

const pick = (options: object) =>
  act(async () =>
    fire('vosh://input-echo-mark-changed', { text: '', color: null, dim: false, ...options }),
  );
const lineMark = (on: boolean) => act(async () => fire('vosh://input-line-mark-changed', on));

describe('the mark at the start of the command line', () => {
  it('draws the mark you picked', async () => {
    const line = await mountLine();
    await pick({ mark: 'chevron' });
    expect(line.mark()).toBe('prompt|\u203a');
    await pick({ mark: 'gt' });
    expect(line.mark()).toBe('prompt|>');
    await pick({ mark: 'own', text: 'you:' });
    expect(line.mark()).toBe('prompt input-mark-wide|you:');
    await line.unmount();
  });

  it('draws no mark while the mark is off or your own text is blank', async () => {
    const line = await mountLine();
    await pick({ mark: 'off', text: 'you:' });
    expect(line.mark()).toBeNull();
    await pick({ mark: 'own', text: '' });
    expect(line.mark()).toBeNull();
    await line.unmount();
  });

  it('draws no mark while the switch is off', async () => {
    const line = await mountLine();
    await pick({ mark: 'gt' });
    await lineMark(false);
    expect(line.mark()).toBeNull();
    await lineMark(true);
    expect(line.mark()).toBe('prompt|>');
    await line.unmount();
  });
});

const look = (options: object) => act(async () => fire('vosh://input-line-look-changed', options));

describe('the look of the command line', () => {
  it('draws the theme band and an accent caret that blinks at the defaults', async () => {
    const line = await mountLine(true);
    expect(line.row()).toBe('input-row');
    expect(line.rowStyle()).toBe('');
    expect(line.caret()).toBe('input-caret caret-shape--block');
    await line.unmount();
  });

  it('tints the band, or lays your own color over it', async () => {
    const line = await mountLine();
    await look({ background: 'tint', backgroundColor: '#0f1a22' });
    expect(line.row()).toBe('input-row is-tint');
    expect(line.rowStyle()).toBe('');
    await look({ background: 'own', backgroundColor: '#0f1a22' });
    expect(line.row()).toBe('input-row is-own');
    expect(line.rowStyle()).toBe('--line-ground:#0f1a22');
    await look({ background: 'theme', backgroundColor: '#0f1a22' });
    expect(line.row()).toBe('input-row');
    expect(line.rowStyle()).toBe('');
    await line.unmount();
  });

  it('colors the caret and what you type', async () => {
    const line = await mountLine();
    await look({ caretColor: '#7ec8d4', textColor: '#c0bdbb' });
    expect(line.rowStyle()).toBe('--caret:#7ec8d4;--line-text:#c0bdbb');
    await line.unmount();
  });

  it('holds the caret steady while Caret blinks is off', async () => {
    const line = await mountLine(true);
    await look({ blink: false });
    expect(line.caret()).toBe('input-caret caret-shape--block is-steady');
    await look({ blink: true });
    expect(line.caret()).toBe('input-caret caret-shape--block');
    await line.unmount();
  });

  it('sets the size only when it is not your terminal size', async () => {
    const line = await mountLine();
    await look({ size: 17 });
    expect(line.rowStyle()).toBe('fontSize:17px');
    await look({ size: 0 });
    expect(line.rowStyle()).toBe('');
    await line.unmount();
  });
});
