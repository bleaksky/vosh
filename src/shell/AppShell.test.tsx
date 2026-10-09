import { act, createElement, useEffect, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import { PaneSizer, type SizedPane } from '../terminal/paneSizer';
import frameCss from '../styles/frame.css?raw';
import overlaysCss from '../styles/overlays.css?raw';
import { PANEL_WIDTH_MIN, PANEL_WIDTH_MIN_FRAMELESS } from '../panel/paneLayout';
import { AppShell } from './AppShell';

/** The bounds the live pane gave the native surface. */
const bounds = vi.hoisted(() => [] as { y: number; height: number }[]);

vi.mock('../ipc/nativeSurface', () => ({
  nativeSurfaceSetBounds: async (b: { y: number; height: number }) => void bounds.push(b),
  nativeSurfaceSetCellMetrics: async () => undefined,
}));
vi.mock('../terminal/terminalRenderer', () => ({ nativeSurfaceEnabled: () => true }));

// The width the frame draws the panel at. On Windows and Linux the
// title band carries the window controls, so the panel draws at least
// PANEL_WIDTH_MIN_FRAMELESS wide to keep them over it. A narrower saved
// width stays saved and draws at that floor. macOS draws it as saved.

type Platform = 'macos' | 'windows' | 'linux';

/** The frame as it draws on `platform` with a saved panel width, and the
 *  sessions sidebar when `sessions` is given. */
function draw(
  platform: Platform,
  panelWidth: number,
  panelOpen = true,
  sessions: ReactNode = null,
  sessionsWidth = 220,
): string {
  vi.stubGlobal('document', { documentElement: { dataset: { platform } } });
  try {
    return renderToStaticMarkup(
      <AppShell
        panelOpen={panelOpen}
        panelWidth={panelWidth}
        onPanelWidth={() => undefined}
        sessions={sessions}
        sessionsWidth={sessionsWidth}
        onSessionsWidth={() => undefined}
        titleBand={null}
        terminal={null}
        input={null}
        statusLine={null}
        panel={null}
      />,
    );
  } finally {
    vi.unstubAllGlobals();
  }
}

/** The custom properties the root publishes. */
function vars(html: string): Record<string, string> {
  const style = html.match(/<main [^>]*style="([^"]*)"/)?.[1] ?? '';
  return Object.fromEntries(
    style
      .split(';')
      .filter(Boolean)
      .map((d) => {
        const at = d.indexOf(':');
        return [d.slice(0, at), d.slice(at + 1)];
      }),
  );
}

/** The panel edge's width handle values. */
function handle(html: string): { min: number; now: number } {
  const tag = html.match(/<div role="separator"[^>]*>/)?.[0] ?? '';
  const read = (name: string) => Number(tag.match(new RegExp(` ${name}="([^"]*)"`))?.[1]);
  return { min: read('aria-valuemin'), now: read('aria-valuenow') };
}

const column = (px: number) => `min(${px}px, calc(100vw - 320px))`;

describe('the panel width the frame draws', () => {
  it('raises a narrower saved width to the floor on Windows and Linux', () => {
    expect(PANEL_WIDTH_MIN_FRAMELESS).toBe(248);
    for (const platform of ['windows', 'linux'] as const) {
      for (const saved of [PANEL_WIDTH_MIN, 220, 247]) {
        const html = draw(platform, saved);
        expect(vars(html), `${platform} ${saved}`).toEqual({
          '--panel-w': '248px',
          '--panel-col': column(248),
        });
        expect(handle(html)).toEqual({ min: 248, now: 248 });
      }
    }
  });

  it('draws the narrowest saved width as saved on macOS', () => {
    const html = draw('macos', PANEL_WIDTH_MIN);
    expect(vars(html)).toEqual({ '--panel-w': '200px', '--panel-col': column(200) });
    expect(handle(html)).toEqual({ min: PANEL_WIDTH_MIN, now: PANEL_WIDTH_MIN });
  });

  it('draws a width at or over the floor as saved on every platform', () => {
    for (const platform of ['macos', 'windows', 'linux'] as const) {
      for (const saved of [248, 300, 520]) {
        const html = draw(platform, saved);
        expect(vars(html), `${platform} ${saved}`).toEqual({
          '--panel-w': `${saved}px`,
          '--panel-col': column(saved),
        });
        expect(handle(html).now).toBe(saved);
      }
    }
  });

  it('keeps the floor in --panel-w while the panel is hidden', () => {
    expect(vars(draw('windows', PANEL_WIDTH_MIN, false))).toEqual({
      '--panel-w': '248px',
      '--panel-col': '0px',
    });
  });
});

describe('the sessions column', () => {
  const sidebar = <nav>sessions</nav>;

  it('keeps the grid of one session as it was, the first column at 0', () => {
    const html = draw('macos', 300);
    expect(vars(html)).toEqual({ '--panel-w': '300px', '--panel-col': column(300) });
    expect(html).not.toContain('shell-slot-sessions');
    const shell = frameCss.match(/\n\.shell \{([^}]*)\}/)?.[1] ?? '';
    expect(shell).toMatch(
      /grid-template-columns: var\(--sessions-col, 0px\) minmax\(0, 1fr\) var\(--panel-col, var\(--panel-w\)\);/,
    );
  });

  it('takes 221 px for the sidebar and keeps the terminal floor past it', () => {
    const html = draw('macos', 300, true, sidebar);
    expect(vars(html)).toEqual({
      '--panel-w': '300px',
      '--panel-col': 'min(300px, calc(100vw - 541px))',
      '--sessions-col': '221px',
    });
    expect(html).toContain('<div class="shell-slot-sessions"><nav>sessions</nav></div>');
  });

  it('gives the panel column nothing while the panel is hidden', () => {
    expect(vars(draw('linux', 300, false, sidebar))).toEqual({
      '--panel-w': '300px',
      '--panel-col': '0px',
      '--sessions-col': '221px',
    });
  });

  it('takes the width you gave the sidebar and its line', () => {
    const html = draw('macos', 300, true, sidebar, 180);
    expect(vars(html)).toEqual({
      '--panel-w': '300px',
      '--panel-col': 'min(300px, calc(100vw - 501px))',
      '--sessions-col': '181px',
    });
  });

  it('makes the sidebar line its width handle, 180 to 320, only while it shows', () => {
    const edge = (html: string) =>
      html.match(/<div role="separator"[^>]*aria-label="Sessions width"[^>]*>/)?.[0] ?? null;
    expect(edge(draw('macos', 300))).toBeNull();
    const tag = edge(draw('macos', 300, true, sidebar, 260)) ?? '';
    expect(tag).toContain('aria-valuemin="180"');
    expect(tag).toContain('aria-valuemax="320"');
    expect(tag).toContain('aria-valuenow="260"');
    expect(tag).toContain('class="shell-sessions-edge"');
  });

  // The corner that holds the toasts and the notices pins itself to the
  // terminal's cell. Column 1 is the sidebar now, so a toast placed there landed on
  // the sidebar, its words squeezed to a letter a line.
  it('keeps the toasts and the notices in the terminal column', () => {
    const rule = (css: string, selector: string) =>
      css.match(
        new RegExp(`\\n${selector.replace(/[.>]/g, (c) => `\\${c}`)} \\{([^}]*)\\}`),
      )?.[1] ?? '';
    const term = rule(frameCss, '.shell-slot-term');
    expect(term).toMatch(/grid-column: 2;/);
    for (const selector of ['.shell > .ov-corner']) {
      const placed = rule(overlaysCss, selector);
      expect(placed, selector).toMatch(/grid-column: 2 \/ 3;/);
      expect(placed, selector).toMatch(/grid-row: 2 \/ 3;/);
    }
  });
});

// Tab follows the eye (Q22): the band, the sessions, the terminal as
// one stop, the command line, then the panel.
describe('the Tab order', () => {
  it('runs band, sessions, Terminal, Command line, then the panel', () => {
    vi.stubGlobal('document', { documentElement: { dataset: { platform: 'macos' } } });
    let html = '';
    try {
      html = renderToStaticMarkup(
        <AppShell
          panelOpen
          panelWidth={300}
          onPanelWidth={() => undefined}
          sessions={<button>Tolliver</button>}
          sessionsWidth={220}
          onSessionsWidth={() => undefined}
          sessionsToggle={<button>Sessions</button>}
          titleBand={<button>Session</button>}
          terminal={<div className="terminal-area" />}
          reader={<ol role="log" aria-label="Game lines" />}
          input={<textarea aria-label="Command line" />}
          statusLine={null}
          panel={<button>Pane</button>}
        />,
      );
    } finally {
      vi.unstubAllGlobals();
    }
    const stops = [...html.matchAll(/<(button|textarea|section|div)\b([^>]*)>([^<]*)/g)]
      .filter(([, tag, attrs]) =>
        tag === 'button' || tag === 'textarea' ? true : / tabindex="0"/.test(attrs),
      )
      .map(([, tag, attrs, text]) => attrs.match(/aria-label="([^"]*)"/)?.[1] ?? text ?? tag);
    expect(stops).toEqual([
      'Sessions',
      'Session',
      'Tolliver',
      'Sessions width',
      'Terminal',
      'Command line',
      'Pane',
      'Panel width',
    ]);
    // The game lines a screen reader reads follow the terminal in its
    // slot, so the one stop holds them under the underlay and xterm.
    expect(html).toContain(
      '<section class="shell-slot-term" aria-label="Terminal" tabindex="0"><div class="terminal-area"></div><ol role="log" aria-label="Game lines"></ol></section>',
    );
    expect(html).toContain('<section class="shell-slot-input" aria-label="Command line">');
  });
});

// A snoop that opens takes the top of the terminal column.
// The split renders first in the terminal's slot, so the live terminal
// keeps its parent and never remounts, which would reload its scrollback.
describe('the snoop slot', () => {
  it('opens and closes the split over the live terminal without remounting it', () => {
    const doc = new FakeDocument();
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      innerWidth: 1280,
      addEventListener() {},
      removeEventListener() {},
    });
    vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    return import('react-dom/client')
      .then(({ createRoot }) => {
        let mounts = 0;
        let unmounts = 0;
        function Live() {
          useEffect(() => {
            mounts += 1;
            return () => {
              unmounts += 1;
            };
          }, []);
          return createElement('div', { className: 'terminal-area' });
        }
        const live = createElement(Live);
        const shell = (snoop: ReactNode) =>
          createElement(AppShell, {
            panelOpen: true,
            panelWidth: 300,
            onPanelWidth: () => undefined,
            titleBand: null,
            snoop,
            terminal: live,
            input: null,
            statusLine: null,
            panel: null,
          });
        const host = doc.createElement('div');
        const root = createRoot(host as unknown as HTMLElement);
        const area = () => findAll(host, (el) => el.getAttribute('class') === 'terminal-area')[0];
        act(() => root.render(shell(null)));
        const first = area();
        const slot = first.parentNode as FakeElement;
        expect(slot.getAttribute('class')).toBe('shell-slot-term');

        act(() => root.render(shell(createElement('section', { className: 'snoop' }))));
        expect(area()).toBe(first);
        expect(slot.childNodes.map((n) => (n as FakeElement).getAttribute('class'))).toEqual([
          'snoop',
          'terminal-area',
        ]);

        act(() => root.render(shell(null)));
        expect(area()).toBe(first);
        expect({ mounts, unmounts }).toEqual({ mounts: 1, unmounts: 0 });
        act(() => root.unmount());
      })
      .finally(() => vi.unstubAllGlobals());
  });

  // Under the native surface the live pane measures its own box, so the
  // grid shrinks with the pane when the split takes the top of the
  // column: a 700 column with a 280 split leaves the pane 420, its sizer
  // 408 inside the terminal's 6 px top and foot.
  it('keeps the native grid on the live pane under a 280 split', () => {
    vi.useFakeTimers();
    vi.stubGlobal('window', {
      devicePixelRatio: 2,
      addEventListener() {},
      removeEventListener() {},
    });
    try {
      const top = 32 + 280 + 6;
      const sizer = {
        getBoundingClientRect: () => ({ left: 16, top, width: 948, height: 420 - 12 }),
      } as unknown as HTMLDivElement;
      const pane: SizedPane = {
        term: { dimensions: undefined } as unknown as SizedPane['term'],
        fit: {} as SizedPane['fit'],
        sizer,
        host: { style: {} } as unknown as HTMLDivElement,
        resize: () => undefined,
        lent: () => 0,
        anchor: () => false,
        quiet: () => false,
        shown: () => true,
        onCellSize: () => undefined,
      };
      new PaneSizer(pane).show();
      expect(bounds.at(-1)).toMatchObject({ x: 16, y: top, width: 948, height: 408 });
    } finally {
      vi.useRealTimers();
      vi.unstubAllGlobals();
    }
  });
});
