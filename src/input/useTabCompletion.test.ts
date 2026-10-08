import { act, createElement, useState, type RefObject } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import { useTabCompletion } from './useTabCompletion';

// Tab completes the word before the caret and cycles its matches. With
// no word there and no cycle running, it leaves the key, so the focus
// moves on to the panel (Q22). React DOM mounts the hook on a stand in
// DOM (src/test/fakeDom.ts).

vi.mock('../stores/gmcp/roomStore', () => ({ getRoom: () => ({ people: [] }) }));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('requestAnimationFrame', () => 0);
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  // React DOM checks for a DOM once, when it loads.
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

/** The command line with its caret at the end of what it holds. */
const field = { selectionStart: 0, setSelectionRange() {} };
const inputRef = { current: field } as unknown as RefObject<HTMLTextAreaElement>;

interface Line {
  value: string;
  setValue: (next: string) => void;
  complete: (step: number) => boolean;
}

function CommandLine({ line }: { line: { now: Line | null } }) {
  const [value, setValue] = useState('');
  const { complete } = useTabCompletion(inputRef, value, setValue, ['kill Orla', 'look'], 1);
  line.now = { value, setValue, complete };
  return null;
}

async function mount() {
  const line: { now: Line | null } = { now: null };
  const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
  await act(async () => root.render(createElement(CommandLine, { line })));
  const now = () => {
    if (!line.now) throw new Error('the command line never rendered');
    return line.now;
  };
  const type = async (text: string) => {
    await act(async () => now().setValue(text));
    field.selectionStart = text.length;
  };
  const tab = async (step = 1) => {
    let took = false;
    await act(async () => {
      took = now().complete(step);
    });
    return took;
  };
  return { now, type, tab, unmount: () => act(async () => root.unmount()) };
}

describe('Tab on the command line', () => {
  it('leaves the key on an empty line, so the focus moves on', async () => {
    const line = await mount();
    await line.type('');
    expect(await line.tab()).toBe(false);
    expect(await line.tab(-1)).toBe(false);
    await line.type('   ');
    expect(await line.tab()).toBe(false);
    await line.unmount();
  });

  it('keeps the key mid-line with no word before the caret', async () => {
    const line = await mount();
    await line.type('kill ');
    expect(await line.tab()).toBe(true);
    expect(line.now().value).toBe('kill ');
    await line.unmount();
  });

  it('takes the key for a word before the caret, matched or not', async () => {
    const line = await mount();
    await line.type('kill Or');
    expect(await line.tab()).toBe(true);
    expect(line.now().value).toBe('kill Orla');
    await line.unmount();

    const other = await mount();
    await other.type('kill Zz');
    expect(await other.tab()).toBe(true);
    expect(other.now().value).toBe('kill Zz');
    await other.unmount();
  });

  it('takes the key while a cycle runs', async () => {
    const line = await mount();
    await line.type('l');
    expect(await line.tab()).toBe(true);
    expect(line.now().value).toBe('look');
    expect(await line.tab()).toBe(true);
    expect(await line.tab(-1)).toBe(true);
    await line.unmount();
  });
});
