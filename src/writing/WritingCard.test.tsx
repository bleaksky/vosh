import { act, createElement } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';

// The card opens on a request: a kind alone, the offer the notice holds,
// or the text the game's editor holds now, from the command line's pill.
// React DOM mounts the card on a stand in DOM (src/test/fakeDom.ts), and
// each call into Rust lands in a list.

const calls = vi.hoisted(() => [] as [string, unknown][]);

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args?: unknown) => {
    calls.push([cmd, args]);
    if (cmd === 'writing_file_get')
      return { version: 1, spelling: false, guide: true, characters: {} };
    return null;
  },
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: async () => () => {},
  emit: async () => undefined,
}));

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let WritingCard: typeof import('./WritingCard').WritingCard;

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
    matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
    innerWidth: 1280,
    innerHeight: 800,
    setTimeout,
    clearTimeout,
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal(
    'MutationObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  vi.stubGlobal('getComputedStyle', () => ({ getPropertyValue: () => '' }));
  // The card keeps its own box in the body, which it moves and takes out.
  const node = FakeNode.prototype as unknown as Record<string, unknown>;
  node.contains = function (this: FakeNode, other: FakeNode | null): boolean {
    for (let n = other; n; n = n.parentNode) if (n === this) return true;
    return false;
  };
  node.remove = function (this: FakeNode) {
    this.parentNode?.removeChild(this);
  };
  Object.defineProperty(FakeElement.prototype, 'parentElement', {
    configurable: true,
    get(this: FakeElement) {
      return this.parentNode instanceof FakeElement ? this.parentNode : null;
    },
  });
  // The card measures a column on a canvas the stand in DOM does not
  // draw.
  Object.defineProperty(FakeElement.prototype, 'getContext', {
    value: () => null,
    configurable: true,
  });
  ({ createRoot } = await import('react-dom/client'));
  ({ WritingCard } = await import('./WritingCard'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe('the writing card', () => {
  it('opens on the text the game’s editor holds, from the pill', async () => {
    const host = doc.createElement('div');
    const root = createRoot(host as unknown as HTMLElement);
    await act(async () => {
      root.render(
        createElement(WritingCard, {
          session: 1,
          request: { kind: 'description', fromEditor: true, n: 1 },
          host: { terminal: () => null, area: () => null, dock: () => null },
          cell: null,
          fontFamily: 'monospace',
          fontSize: 14,
          themeTerminalColors: true,
          brightBold: false,
          renderer: 'xterm',
          onClose: () => {},
        }),
      );
      await settle();
    });
    const taken = calls.filter(
      ([cmd]) => cmd.startsWith('writing_take') || cmd === 'writing_start',
    );
    expect(taken).toHaveLength(1);
    expect(taken[0][0]).toBe('writing_take_editor');
    expect(taken[0][1]).toMatchObject({ session: 1 });
    await act(async () => root.unmount());
  });
});
