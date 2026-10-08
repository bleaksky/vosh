import { act, createElement } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { LuaLine } from '../../ipc/scripts';
import { FakeDocument, findAll, type FakeElement } from '../../test/fakeDom';
import { lineTime } from './scriptTimes';
import type { LuaConsole as LuaConsoleType } from './LuaConsole';

// The Console section of Scripts, mounted in the stand in for the DOM in
// src/test/fakeDom.ts, which runs the layout effect that keeps the
// newest line in view.

const calls = vi.hoisted(() => ({ invoked: [] as { cmd: string; args: unknown }[] }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: unknown) => {
    calls.invoked.push({ cmd, args });
    return Promise.resolve(null);
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let LuaConsole: typeof LuaConsoleType;
let NO_LUA_LINES: string;

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
  vi.stubGlobal('navigator', { userAgent: 'node' });
  ({ createRoot } = await import('react-dom/client'));
  ({ LuaConsole, NO_LUA_LINES } = await import('./LuaConsole'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

/** A moment on October 4 in your own time zone. */
const at = (h: number, m: number, s: number) => new Date(2026, 9, 4, h, m, s).getTime();

// An Output with one line of each kind.
const LINES: LuaLine[] = [
  {
    ts_ms: at(21, 14, 3),
    owner: 'plugin:vitals_alert',
    kind: 'error',
    text: "vitals_alert/main.lua:22: attempt to concatenate a nil value (field 'hp_pct')",
    at: { source: 'vitals_alert/main.lua', line: 22 },
  },
  {
    ts_ms: at(21, 14, 31),
    owner: 'plugin:vitals_alert',
    kind: 'note',
    text: 'Vosh reloaded vitals_alert.',
  },
  { ts_ms: at(21, 15, 2), owner: '#lua', kind: 'input', text: 'print(mud.var("target"))' },
  { ts_ms: at(21, 15, 2), owner: '#lua', kind: 'print', text: 'a bank representative' },
];

/** Call a React handler on `el`. This DOM sends no events. */
function handler<E>(el: FakeElement, name: string): (e: E) => void {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
  return (el as unknown as Record<string, Record<string, (e: E) => void>>)[key][name];
}

interface Mounted {
  container: FakeElement;
  cleared: () => number;
  errors: (string | null)[];
  /** Draw the Console again with other lines. */
  show: (lines: LuaLine[]) => Promise<void>;
  unmount: () => Promise<void>;
}

async function mount(lines: LuaLine[]): Promise<Mounted> {
  calls.invoked.length = 0;
  let cleared = 0;
  const errors: (string | null)[] = [];
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const show = async (next: LuaLine[]) => {
    await act(async () => {
      root.render(
        createElement(LuaConsole, {
          lines: next,
          onCleared: () => void cleared++,
          onError: (e) => void errors.push(e),
        }),
      );
    });
  };
  await show(lines);
  return {
    container,
    cleared: () => cleared,
    errors,
    show,
    unmount: async () => {
      await act(async () => {
        root.unmount();
      });
      doc.body.removeChild(container);
    },
  };
}

const byClass = (root: FakeElement, name: string) =>
  findAll(root, (el) => (el.getAttribute('class') ?? '').split(' ').includes(name));

const clearButton = (root: FakeElement) =>
  findAll(root, (el) => el.nodeName === 'BUTTON' && el.textContent === 'Clear')[0];

const field = (root: FakeElement) =>
  findAll(root, (el) => el.getAttribute('aria-label') === 'Run Lua')[0];

describe('lineTime', () => {
  it('reads your local time as hours, minutes and seconds, two digits each', () => {
    expect(lineTime(at(21, 14, 3))).toBe('21:14:03');
    expect(lineTime(at(9, 5, 0))).toBe('09:05:00');
    expect(lineTime(at(0, 0, 59))).toBe('00:00:59');
  });
});

describe('the Console', () => {
  it('shows each line with its time, its tag and the tone of its kind', async () => {
    const m = await mount(LINES);
    const rows = byClass(m.container, 'st-lua-line').map((li) => ({
      class: li.getAttribute('class'),
      time: byClass(li, 'st-lua-time')[0].textContent,
      text: byClass(li, 'st-lua-text')[0].textContent,
      tagged: byClass(li, 'st-lua-tag').length === 1,
    }));
    expect(rows).toEqual([
      {
        class: 'st-lua-line is-error',
        time: '21:14:03',
        text: "[lua] vitals_alert/main.lua:22: attempt to concatenate a nil value (field 'hp_pct')",
        tagged: true,
      },
      {
        class: 'st-lua-line is-note',
        time: '21:14:31',
        text: '[lua] Vosh reloaded vitals_alert.',
        tagged: true,
      },
      // A line you ran shows as you typed it, after the field's glyph.
      {
        class: 'st-lua-line is-input',
        time: '21:15:02',
        text: '› print(mud.var("target"))',
        tagged: false,
      },
      {
        class: 'st-lua-line',
        time: '21:15:02',
        text: '[lua] a bank representative',
        tagged: true,
      },
    ]);
    expect(byClass(m.container, 'st-lua-empty')).toHaveLength(0);
    expect(clearButton(m.container).hasAttribute('disabled')).toBe(false);
    await m.unmount();
  });

  it('says what shows there and holds Clear while it has no line', async () => {
    const m = await mount([]);
    expect(byClass(m.container, 'st-lua-empty')[0].textContent).toBe(NO_LUA_LINES);
    expect(byClass(m.container, 'st-lua-out')[0].getAttribute('class')).toBe('st-lua-out is-empty');
    expect(byClass(m.container, 'st-lua-line')).toHaveLength(0);
    expect(clearButton(m.container).hasAttribute('disabled')).toBe(true);
    expect(field(m.container).getAttribute('placeholder')).toBe('Run Lua');
    await m.unmount();
  });

  it('scrolls to the newest line as one arrives', async () => {
    const m = await mount(LINES.slice(0, 2));
    const list = byClass(m.container, 'st-lua-lines')[0] as FakeElement & {
      scrollTop: number;
      scrollHeight: number;
    };
    list.scrollHeight = 400;
    await m.show(LINES);
    expect(list.scrollTop).toBe(400);
    await m.unmount();
  });

  it('empties every line of the session with Clear', async () => {
    const m = await mount(LINES);
    await act(async () => {
      handler<unknown>(clearButton(m.container), 'onClick')({});
    });
    expect(calls.invoked).toEqual([{ cmd: 'lua_output_clear', args: { owner: undefined } }]);
    expect(m.cleared()).toBe(1);
    expect(m.errors).toEqual([null]);
    await m.unmount();
  });

  it('runs what you type on Enter and empties the field', async () => {
    const m = await mount(LINES);
    const enter = { key: 'Enter', preventDefault() {} };
    // A blank line runs nothing.
    await act(async () => {
      handler<unknown>(field(m.container), 'onKeyDown')(enter);
    });
    expect(calls.invoked).toEqual([]);
    await act(async () => {
      handler<{ target: { value: string } }>(
        field(m.container),
        'onChange',
      )({
        target: { value: 'print(mud.var("target"))' },
      });
    });
    expect(field(m.container).value).toBe('print(mud.var("target"))');
    await act(async () => {
      handler<unknown>(field(m.container), 'onKeyDown')({ key: 'a', preventDefault() {} });
    });
    expect(calls.invoked).toEqual([]);
    await act(async () => {
      handler<unknown>(field(m.container), 'onKeyDown')(enter);
    });
    expect(calls.invoked).toEqual([{ cmd: 'lua_run', args: { code: 'print(mud.var("target"))' } }]);
    expect(field(m.container).value).toBe('');
    await m.unmount();
  });
});
