import { act, createElement, useState } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import { historyGet, historyPush } from '../ipc/input';
import {
  HISTORY_CAP,
  joinHistory,
  useCommandHistory,
  withLine,
  withoutNewest,
} from './useCommandHistory';

vi.mock('../ipc/input', () => ({
  historyGet: vi.fn(),
  historyPush: vi.fn(),
}));
const get = vi.mocked(historyGet);
const push = vi.mocked(historyPush);

beforeEach(() => {
  get.mockReset().mockResolvedValue([]);
  push.mockReset().mockResolvedValue(true);
});

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

function CommandLine({
  session,
  line,
  open,
}: {
  session: number;
  line: { now: Line | null };
  open?: readonly number[];
}) {
  const [value, setValue] = useState('');
  const lines = useCommandHistory(value, setValue, session, open);
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

  it('shows what a session sent in earlier runs before what it sends now', async () => {
    let answer: (lines: string[]) => void = () => undefined;
    get.mockReturnValue(new Promise((resolve) => (answer = resolve)));
    const line: { now: Line | null } = { now: null };
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () => root.render(createElement(CommandLine, { session: 1, line })));
    expect(get).toHaveBeenCalledWith(1);
    // A command goes out while the earlier ones still load, and one of
    // them reached the file already.
    await act(async () => line.now?.lines.remember('score', 1));
    await act(async () => answer(['look', 'cast armor', 'score']));
    expect(line.now?.lines.history).toEqual(['look', 'cast armor', 'score']);
    await act(async () => line.now?.lines.recallOlder());
    expect(line.now?.value).toBe('score');
    await act(async () => line.now?.lines.recallOlder());
    expect(line.now?.value).toBe('cast armor');
    // Showing the session again asks for nothing more.
    await act(async () => root.render(createElement(CommandLine, { session: 1, line })));
    expect(get).toHaveBeenCalledTimes(1);
    await act(async () => root.unmount());
  });

  it('hands each new command to Rust once, and drops one Rust turns down', async () => {
    push.mockImplementation(async (sent) => !sent.startsWith('password '));
    const line: { now: Line | null } = { now: null };
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () => root.render(createElement(CommandLine, { session: 1, line })));
    await act(async () => {
      line.now?.lines.remember('look', 1);
      line.now?.lines.remember('look', 1);
      line.now?.lines.remember('password old new', 1);
    });
    expect(push.mock.calls).toEqual([
      ['look', 1],
      ['password old new', 1],
    ]);
    expect(line.now?.lines.history).toEqual(['look']);
    await act(async () => root.unmount());
  });

  it('lets the history of a closed session go', async () => {
    const line: { now: Line | null } = { now: null };
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    const show = (session: number, open: readonly number[]) =>
      act(async () => root.render(createElement(CommandLine, { session, line, open })));
    await show(2, [1, 2]);
    await act(async () => line.now?.lines.remember('where', 2));
    await show(1, [1, 2]);
    await show(1, [1]);
    // A new session that takes the same number starts empty and reads
    // its own file again.
    await show(2, [1, 2]);
    expect(line.now?.lines.history).toEqual([]);
    expect(get.mock.calls.filter(([id]) => id === 2)).toHaveLength(2);
    await act(async () => root.unmount());
  });
});

describe('the lines a history holds', () => {
  it('skips a repeat of the newest line and keeps the newest thousand', () => {
    const full = Array.from({ length: HISTORY_CAP }, (_, n) => `say ${n}`);
    expect(withLine(['look'], 'look')).toEqual(['look']);
    const next = withLine(full, 'look');
    expect(next).toHaveLength(HISTORY_CAP);
    expect(next[0]).toBe('say 1');
    expect(next[HISTORY_CAP - 1]).toBe('look');
  });

  it('joins earlier runs to this one with no line twice in a row', () => {
    expect(joinHistory(['look', 'look', 'score'], ['score', 'where'])).toEqual([
      'look',
      'score',
      'where',
    ]);
  });

  it('drops only the newest copy of a line', () => {
    expect(withoutNewest(['look', 'score', 'look'], 'look')).toEqual(['look', 'score']);
    const lines = ['look'];
    expect(withoutNewest(lines, 'where')).toBe(lines);
  });
});
