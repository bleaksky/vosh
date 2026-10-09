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
      : cmd === 'input_known_words'
        ? { aliases: ['eb'], commands: ['alias', 'walk'] }
        : cmd === 'writing_file_get'
          ? { version: 1, spelling: false, guide: true, characters: {} }
          : null,
}));

function fire(event: string, payload: unknown): void {
  for (const cb of bus.handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let Input: typeof import('./Input').Input;
type InputHandle = import('./Input').InputHandle;

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
  // box sits at the origin. The coloring layer writes its scrollTop.
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
    Object.defineProperty(FakeElement.prototype, name, {
      value: 0,
      configurable: true,
      writable: true,
    });
  }
  // The game editor measures a column on a canvas the stand in DOM
  // does not draw.
  Object.defineProperty(FakeElement.prototype, 'getContext', {
    value: () => null,
    configurable: true,
  });
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
  const handle: { current: InputHandle | null } = { current: null };
  await act(async () => {
    root.render(
      createElement(Input, {
        ref: handle,
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
    /** The colored runs the layer draws, as text|class, or null with no
     *  layer. */
    layer: () => {
      const el = byClass('input-type-layer');
      if (!el) return null;
      return el.childNodes.map((n) =>
        n instanceof FakeElement ? `${n.textContent}|${n.getAttribute('class')}` : n.textContent,
      );
    },
    spellCheck: () =>
      findAll(host, (el) => el.tagName === 'TEXTAREA')[0]?.getAttribute('spellcheck'),
    /** The pill as its face, with no pill as null. */
    pill: () => {
      const el = byClass('input-pill');
      if (!el) return null;
      return findAll(el, (n) => n.getAttribute('aria-hidden') === 'true')
        .map((n) => n.textContent)
        .join(' ');
    },
    /** What the pill reads to a screen reader. */
    pillLabel: () => byClass('visually-hidden')?.textContent ?? null,
    pillCount: () => byClass('input-pill-count')?.getAttribute('class') ?? null,
    placeholder: () =>
      findAll(host, (el) => el.tagName === 'TEXTAREA' || el.tagName === 'INPUT')[0]?.getAttribute(
        'placeholder',
      ) ?? null,
    has: (name: string) => byClass(name) !== undefined,
    type: (text: string) =>
      act(async () => {
        handle.current?.insert(text);
        await settle();
      }),
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

/** Where the writer stands in session 1, idle unless `state` says. */
const writingAt = (state: object) =>
  act(async () =>
    fire('session://writing', {
      session: 1,
      game: 'unknown',
      editor: null,
      offer: null,
      lines: null,
      job: null,
      held: 0,
      done: null,
      ...state,
    }),
  );
const walkAt = (progress: object) =>
  act(async () => fire('session://walk', { session: 1, ...progress }));
const passwordAt = (password: boolean) =>
  act(async () => fire('session://input-mode', { session: 1, password }));

describe('the mode pill', () => {
  it('takes the mark’s place in the game’s editor, counting the line you are on', async () => {
    const line = await mountLine();
    await pick({ mark: 'chevron' });
    await writingAt({ game: 'editor', editor: 'description', lines: 3 });
    expect(line.mark()).toBeNull();
    expect(line.pill()).toBe('Description · 4 of 30');
    expect(line.pillLabel()).toBe('Description, line 4 of 30');
    expect(line.row()).toBe('input-row is-mode');
    expect(line.rowStyle()).toBe('--input-mode:var(--accent)');
    expect(line.placeholder()).toBe('Type @ on a blank line to finish');
    expect(line.has('wr-cl-count')).toBe(false);
    await writingAt({ game: 'editor', editor: 'description', lines: 30 });
    expect(line.pill()).toBe('Description · 31 of 30');
    expect(line.pillCount()).toBe('input-pill-count is-warn');
    expect(line.rowStyle()).toBe('--input-mode:var(--warn)');
    await writingAt({});
    expect(line.pill()).toBeNull();
    expect(line.mark()).toBe('prompt|\u203a');
    expect(line.row()).toBe('input-row');
    expect(line.placeholder()).toBeNull();
    await line.unmount();
  });

  it('shows More at the pager and Walking while a walk runs', async () => {
    const line = await mountLine();
    await writingAt({ game: 'pager' });
    expect(line.pill()).toBe('More');
    expect(line.placeholder()).toBe('Press Enter for the next page');
    expect(line.rowStyle()).toBe('--input-mode:var(--secondary)');
    await writingAt({});
    await walkAt({ kind: 'walking', done: 1, total: 2, left: 'w', route: false });
    expect(line.pill()).toBe('Walking · 1 step left');
    expect(line.pillLabel()).toBe('Walking, 1 step left');
    expect(line.placeholder()).toBe('Esc stops the walk');
    expect(line.rowStyle()).toBe('--input-mode:var(--success)');
    await walkAt({ kind: 'idle' });
    expect(line.pill()).toBeNull();
    await line.unmount();
  });

  it('names a password prompt in place of the placeholder', async () => {
    const line = await mountLine();
    await passwordAt(true);
    expect(line.pill()).toBe('Password');
    expect(line.placeholder()).toBeNull();
    expect(line.rowStyle()).toBe('--input-mode:var(--secondary)');
    await passwordAt(false);
    expect(line.pill()).toBeNull();
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

const typeColors = (on: boolean) =>
  act(async () => {
    fire('vosh://input-type-colors-changed', { on });
    await settle();
  });

describe('coloring as you type', () => {
  it('colors an alias, a # command, a chat line and an unknown # command', async () => {
    const line = await mountLine();
    await typeColors(true);
    await line.type('look');
    expect(line.layer()).toEqual(['look']);
    expect(line.row()).toBe('input-row is-typed');
    await line.type('eb');
    expect(line.layer()).toEqual(['eb|input-type-alias']);
    await line.type('#walk 3n2e');
    expect(line.layer()).toEqual(['#walk|input-type-hash', ' 3n2e']);
    await line.type('say The day has begun.');
    expect(line.layer()).toEqual(['say The day has begun.|input-type-chat']);
    await line.type('#walkies');
    expect(line.layer()).toEqual(['#walkies|input-type-unknown']);
    await line.type('Eb');
    expect(line.layer()).toEqual(['Eb']);
    await line.unmount();
  });

  it('judges each line of a compose', async () => {
    const line = await mountLine();
    await typeColors(true);
    await line.type("eb\n'hello");
    expect(line.layer()).toEqual(['eb|input-type-alias', '\n', "'hello|input-type-chat"]);
    await line.unmount();
  });

  it('keeps spell check on for a chat line', async () => {
    const line = await mountLine();
    await act(async () => fire('vosh://spellcheck-prompt-changed', true));
    await typeColors(true);
    await line.type('tell Maren hi');
    expect(line.layer()).toEqual(['tell Maren hi|input-type-chat']);
    expect(line.spellCheck()).toBe('true');
    await line.unmount();
  });

  it('draws no layer while off, on a password or in the game editor', async () => {
    const line = await mountLine();
    await typeColors(false);
    await line.type('eb');
    expect(line.layer()).toBeNull();
    expect(line.row()).toBe('input-row');
    await typeColors(true);
    expect(line.layer()).not.toBeNull();
    await act(async () => fire('session://input-mode', { session: 1, password: true }));
    expect(line.layer()).toBeNull();
    await act(async () => fire('session://input-mode', { session: 1, password: false }));
    await act(async () =>
      fire('session://writing', {
        session: 1,
        game: 'editor',
        editor: 'description',
        offer: null,
        lines: null,
        job: null,
        held: 0,
        done: null,
      }),
    );
    expect(line.layer()).toBeNull();
    expect(line.row()).toBe('input-row is-mode');
    await act(async () =>
      fire('session://writing', {
        session: 1,
        game: 'unknown',
        editor: null,
        offer: null,
        lines: null,
        job: null,
        held: 0,
        done: null,
      }),
    );
    await typeColors(false);
    expect(line.layer()).toBeNull();
    await line.unmount();
  });
});
