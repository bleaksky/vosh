import { act, createElement } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';

// The command line starts with the mark your commands echo with while
// Use the same mark in the command line is on, and with no mark at all
// while the mark is off or the switch is. React DOM mounts the command
// line on a stand in DOM (src/test/fakeDom.ts), and the mark and the
// switch reach it as Settings sends them, through a fake event bus.

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
  invoke: async (cmd: string) => (cmd === 'ui_get_config' ? { tracked_affects: [] } : null),
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
  vi.stubGlobal('getComputedStyle', () => ({}));
  ({ createRoot } = await import('react-dom/client'));
  ({ Input } = await import('./Input'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

/** Mount the command line, and read the mark it draws as its class and
 *  text, or null with no mark. */
async function mountLine() {
  const host = doc.createElement('div');
  const root = createRoot(host as unknown as HTMLElement);
  await act(async () => {
    root.render(
      createElement(Input, {
        // Off, so the line takes no focus and draws no caret, which
        // measures a layout the stand in DOM does not have.
        enabled: false,
        macroKeys: { command: () => undefined, bound: () => false },
      }),
    );
    await settle();
  });
  const mark = () => {
    const spans = findAll(host, (el) => /\bprompt\b/.test(el.getAttribute('class') ?? ''));
    return spans.map((el) => `${el.getAttribute('class')}|${el.textContent}`).join(',') || null;
  };
  return { mark, unmount: () => act(async () => root.unmount()) };
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
