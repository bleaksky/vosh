import { act, createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { VitalsText as Pushed } from '../ipc/vitals';
import type { BandEnv } from '../terminal/bandCells';
import { parseSgrCells } from '../terminal/sgrCells';
import panelCss from '../styles/panel.css?raw';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';

// The Text footer. The session's pushes come through a fake Tauri event
// bus, and every invoke is kept, so a test reads the watch the footer
// asks for. Each test loads fresh modules, since the stores keep their
// sessions at module scope.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const invoked: { cmd: string; args: unknown }[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args: unknown) => {
    invoked.push({ cmd, args });
    return null;
  },
}));

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

// Vosh's text in a fight at 23 cells, as the session renders it.
const LIVE =
  'a Blackwatch guard  \x1b[33m54%\x1b[39m\r\n\x1b[39m765\x1b[90m/1020hp\x1b[39m 800\x1b[90m/800mn\x1b[39m 930\x1b[90m/930mv\x1b[39m\x1b[0m';
const FULL =
  'a Blackwatch guard \x1b[33m100%\x1b[39m\r\n\x1b[39m1020\x1b[90m/1020hp\x1b[39m 800\x1b[90m/800mn\x1b[39m 930\x1b[90m/930mv\x1b[39m\x1b[0m';

function render(ansi: string, pushWidth: number) {
  const plain = { kind: 'default' } as const;
  return {
    ansi,
    plain: '',
    rows: 2,
    spans: [
      {
        piece: 2,
        row: 0,
        col: 18,
        width: pushWidth,
        fg: plain,
        bg: plain,
        bold: false,
        italic: false,
        underline: false,
      },
    ],
  };
}

const PUSHED: Pushed = {
  session: 1,
  live: render(LIVE, 2),
  full: render(FULL, 1),
  fight: [true, false],
  right: [2],
};

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
    matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  vi.stubGlobal('getComputedStyle', () => ({ getPropertyValue: () => '' }));
  vi.stubGlobal(
    'MutationObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
});

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  invoked.length = 0;
});

const COLORS = { themeTerminalColors: false, brightBold: false };

/** Mount the Text footer `width` px wide, and hand back how to read it. */
async function footer(width: number, fightOnly = false) {
  const { VitalsText } = await import('./VitalsText');
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  const draw = (w: number) =>
    act(async () => {
      root.render(createElement(VitalsText, { width: w, colors: COLORS, fightOnly }));
      await settle();
    });
  await draw(width);
  const classOf = (el: FakeElement) => el.getAttribute('class') ?? '';
  return {
    rows: () =>
      findAll(container, (el) => classOf(el) === 'panel-vitals-text-row').map((el) =>
        findAll(el, (part) => classOf(part).startsWith('panel-vitals-text-'))
          .map((part) => part.textContent)
          .join('|'),
      ),
    watches: () =>
      invoked.filter((c) => c.cmd === 'vitals_text_watch').map((c) => c.args as object),
    resize: draw,
    unmount: () => act(async () => root.unmount()),
  };
}

async function push(text: Pushed): Promise<void> {
  await act(async () => {
    for (const cb of handlers.get('session://vitals-text') ?? []) cb({ payload: text });
  });
}

describe('the Text footer', () => {
  it('watches at its width in cells and draws each push', async () => {
    // 196 px less the 30 px sides is 23 cells of 7.2 px, 0.6 of the
    // 12 px panel size where nothing measures a face.
    const shown = await footer(196);
    expect(shown.watches()).toEqual([{ cols: 23, session: 1 }]);
    expect(shown.rows()).toEqual([]);
    await push(PUSHED);
    expect(shown.rows()).toEqual(['a Blackwatch guard|54%', '765/1020hp 800/800mn', '930/930mv']);
    await shown.unmount();
  });

  it('watches again at a new width and stops when it goes', async () => {
    const shown = await footer(196);
    await shown.resize(300);
    await shown.unmount();
    expect(shown.watches()).toEqual([
      { cols: 23, session: 1 },
      { cols: null, session: 1 },
      { cols: 37, session: 1 },
      { cols: null, session: 1 },
    ]);
  });

  it('keeps only your opponent while your prompt hides your vitals', async () => {
    const shown = await footer(196, true);
    await push(PUSHED);
    expect(shown.rows()).toEqual(['a Blackwatch guard|54%']);
    await push({ ...PUSHED, live: render('', 0), full: render('', 0), fight: [] });
    expect(shown.rows()).toEqual([]);
    await shown.unmount();
  });
});

describe('the Text footer as drawn', () => {
  const env: BandEnv = {
    palette: [
      '#000000',
      '#cc3333',
      '#33cc33',
      '#cccc33',
      '#3333cc',
      '#cc33cc',
      '#33cccc',
      '#cccccc',
      '#666666',
      '#ff6666',
      '#66ff66',
      '#ffff66',
      '#6666ff',
      '#ff66ff',
      '#66ffff',
      '#ffffff',
    ],
    fg: '#d0d0d0',
    bg: '#101218',
    selection: '#333333',
    selectionText: '#ffffff',
    renderer: 'xterm',
    brightBold: false,
  };

  it('draws each cell in the color the terminal gives it, never lifted', async () => {
    const { VitalsTextBlock } = await import('./VitalsText');
    const [row] = parseSgrCells('\x1b[31m159\x1b[90m/1020hp\x1b[39m');
    const html = renderToStaticMarkup(
      <VitalsTextBlock lines={[{ left: row, right: null }]} env={env} />,
    );
    expect(html).toContain('<span style="color:#cc3333">159</span>');
    expect(html).toContain('<span style="color:#666666">/1020hp</span>');
    expect(html).toContain('aria-label="Vitals"');
  });

  it('keeps no room with nothing to draw', async () => {
    const { VitalsTextBlock } = await import('./VitalsText');
    expect(renderToStaticMarkup(<VitalsTextBlock lines={[]} env={env} />)).toBe(
      '<section class="panel-vitals-text is-empty" aria-hidden="true"></section>',
    );
    expect(panelCss).toMatch(/\.panel-vitals-text\.is-empty \{\s*padding: 0;\s*border-top: 0;/);
  });
});
