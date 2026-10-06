import { act, createElement, useState } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import { useCommandHistory } from './useCommandHistory';

// Each session keeps its own command history and the line you were
// composing in it. React DOM mounts a command line on a stand in DOM
// (src/test/fakeDom.ts) and the test moves the selection under it.

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
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  // React DOM checks for a DOM once, when it loads.
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

/** What the command line shows and does, as the newest render left it. */
interface Line {
  value: string;
  setValue: (next: string) => void;
  lines: ReturnType<typeof useCommandHistory>;
}

function CommandLine({ session, line }: { session: number; line: { now: Line | null } }) {
  const [value, setValue] = useState('');
  const lines = useCommandHistory(value, setValue, session);
  line.now = { value, setValue, lines };
  return null;
}

describe('the command line of each session', () => {
  it('keeps a history and a draft for each session', async () => {
    const line: { now: Line | null } = { now: null };
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    const show = (session: number) =>
      act(async () => root.render(createElement(CommandLine, { session, line })));
    const now = () => {
      if (!line.now) throw new Error('the command line never rendered');
      return line.now;
    };

    // Tolliver's session sends two commands and starts a third.
    await show(1);
    await act(async () => {
      now().lines.remember('look', 1);
      now().lines.remember('score', 1);
    });
    await act(async () => now().setValue('cast armor'));

    // Orla's session shows an empty line and a history of its own.
    await show(2);
    expect(now().value).toBe('');
    expect(now().lines.history).toEqual([]);
    await act(async () => now().lines.remember('where', 2));
    await act(async () => now().setValue('exa helm'));

    // Back in Tolliver's, his draft and his history come back.
    await show(1);
    expect(now().value).toBe('cast armor');
    expect(now().lines.history).toEqual(['look', 'score']);
    await act(async () => now().setValue(''));
    await act(async () => now().lines.recallOlder());
    expect(now().value).toBe('score');

    // And Orla's keep hers.
    await show(2);
    expect(now().value).toBe('exa helm');
    expect(now().lines.history).toEqual(['where']);
    await act(async () => root.unmount());
  });

  it('adds a line to the session it went to', async () => {
    const line: { now: Line | null } = { now: null };
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () => root.render(createElement(CommandLine, { session: 2, line })));
    // A paste that began in Tolliver's session ends after you selected
    // Orla's.
    await act(async () => line.now?.lines.remember('look', 1));
    expect(line.now?.lines.history).toEqual([]);
    await act(async () => root.render(createElement(CommandLine, { session: 1, line })));
    expect(line.now?.lines.history).toEqual(['look']);
    await act(async () => root.unmount());
  });
});
