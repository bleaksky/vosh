import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { PromptShowState } from '../ipc/prompt';
import { DEFAULT_VITALS_OPTIONS, type VitalsOptions } from '../ipc/uiConfig';
import { sanitizeLayout, type PaneLayout } from './paneLayout';
import { PanelHost } from './PanelHost';

// The stores behind the panel reach the Tauri bridge when they start.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

let options: VitalsOptions = DEFAULT_VITALS_OPTIONS;
vi.mock('../stores/config/vitalsOptionsStore', () => ({
  useVitalsOptions: () => options,
}));

// No panes unless a test sets some, so only the footer can draw. The
// footer stands in as its own section, since its stores need the
// running app, and says the panel size it hears.
let layout: PaneLayout | null = null;
vi.mock('./panelLayoutStore', () => ({
  usePanelLayout: () => layout,
  getPanelLayout: () => null,
  setPaneTree: vi.fn(),
}));
vi.mock('./usePaneMins', () => ({ usePaneMins: () => ({}) }));
// A Lua pane's body reads the session stores, which LuaPane.test covers,
// and a Chat pane's reads the live theme.
vi.mock('./lua/LuaPane', () => ({ LuaPane: () => null }));
vi.mock('./chat/ChatPane', () => ({ ChatPane: () => null }));
vi.mock('./VitalsFooter', async () => {
  const { useContext } = await import('react');
  const { PaneTextSizeContext } = await import('./paneTextSize');
  return {
    VitalsFooter: ({ opponentOnly }: { opponentOnly?: boolean }) => (
      <section
        className={opponentOnly ? 'panel-opponent' : 'panel-vitals'}
        data-size={useContext(PaneTextSizeContext)}
      />
    ),
  };
});

const PINNED: PromptShowState = {
  show: 'pinned',
  capture: true,
  draw: true,
  gameSent: true,
  zone: 1,
  promptsOff: false,
};

function drawsVitals(promptShow: PromptShowState | null, hide: boolean): boolean {
  options = { ...DEFAULT_VITALS_OPTIONS, hide_when_pinned: hide };
  return renderToStaticMarkup(<PanelHost promptShow={promptShow} />).includes('panel-vitals');
}

describe('PanelHost', () => {
  it('drops the vitals while your prompt is pinned and the switch is on', () => {
    expect(drawsVitals(PINNED, true)).toBe(false);
  });

  it('keeps the opponent row while your pinned prompt hides the vitals', () => {
    options = { ...DEFAULT_VITALS_OPTIONS, hide_when_pinned: true };
    expect(renderToStaticMarkup(<PanelHost promptShow={PINNED} />)).toContain('panel-opponent');
    options = { ...DEFAULT_VITALS_OPTIONS, hide_when_pinned: false };
    expect(renderToStaticMarkup(<PanelHost promptShow={PINNED} />)).not.toContain('panel-opponent');
  });

  it('brings the vitals back when either one is off', () => {
    expect(drawsVitals(PINNED, false)).toBe(true);
    expect(drawsVitals({ ...PINNED, show: 'text' }, true)).toBe(true);
    expect(drawsVitals(null, true)).toBe(true);
  });

  it('keeps the vitals while prompts are off in the game', () => {
    expect(drawsVitals({ ...PINNED, promptsOff: true }, true)).toBe(true);
  });

  it('draws no footer with your vitals in the status line, so the panes reach the foot', () => {
    for (const hide of [true, false]) {
      options = { ...DEFAULT_VITALS_OPTIONS, place: 'status', hide_when_pinned: hide };
      for (const show of [PINNED, null]) {
        const html = renderToStaticMarkup(<PanelHost promptShow={show} />);
        expect(html).not.toContain('panel-vitals');
        expect(html).not.toContain('panel-opponent');
      }
    }
  });

  it('hands your panel size to every pane and the vitals', () => {
    options = DEFAULT_VITALS_OPTIONS;
    // The main window writes the size on the root for panel.css, so the
    // panel writes none of its own.
    const at16 = renderToStaticMarkup(<PanelHost promptShow={null} textSize={16} />);
    expect(at16).toContain('<div class="panel-host">');
    expect(at16).toContain('<section class="panel-vitals" data-size="16">');
    // 12 px, the panes as they were drawn, when no size or no real one
    // comes in.
    for (const size of [undefined, 0, Number.NaN]) {
      expect(renderToStaticMarkup(<PanelHost promptShow={null} textSize={size} />)).toContain(
        '<section class="panel-vitals" data-size="12">',
      );
    }
  });

  it('draws one section per Lua pane, named by its title, in key order', () => {
    options = DEFAULT_VITALS_OPTIONS;
    const lua = (id: string, title: string) => ({
      pane: 'lua',
      props: { plugin: 'weather_pane', id, title },
    });
    layout = sanitizeLayout({
      root: { split: 'column', children: [lua('weather', 'Weather'), lua('tides', 'Tides')] },
    });
    const html = renderToStaticMarkup(<PanelHost promptShow={null} />);
    layout = null;
    const labels = [...html.matchAll(/class="pane pane-lua" aria-label="([^"]*)"/g)].map(
      (m) => m[1],
    );
    expect(labels).toEqual(['Tides', 'Weather']);
  });

  it('draws one section per Chat pane', () => {
    // Server rendering never checks keys, so leafKey's own test in
    // paneLayout.test.ts covers the two keys.
    options = DEFAULT_VITALS_OPTIONS;
    const chat = (id: string) => ({ id, pane: 'chat', props: {} });
    layout = sanitizeLayout({
      root: { split: 'column', children: [chat('chat'), chat('chat-2')] },
    });
    const html = renderToStaticMarkup(<PanelHost promptShow={null} />);
    layout = null;
    expect(html.match(/class="pane pane-chat"/g)).toHaveLength(2);
  });

  it('names each Chat pane by its filter only while two or more show', () => {
    options = DEFAULT_VITALS_OPTIONS;
    const chatLabels = (...props: Record<string, string>[]) => {
      layout = sanitizeLayout({
        root: {
          split: 'column',
          children: props.map((p, i) => ({ id: `chat-${i}`, pane: 'chat', props: p })),
        },
      });
      const html = renderToStaticMarkup(<PanelHost promptShow={null} />);
      layout = null;
      return [...html.matchAll(/class="pane pane-chat" aria-label="([^"]*)"/g)].map((m) => m[1]);
    };
    expect(chatLabels({ channel: 'tell' }, { rest: '1' })).toEqual([
      'Chat, Tell',
      'Chat, Everything else',
    ]);
    expect(chatLabels({ channel: 'tell' })).toEqual(['Chat']);
  });
});
