import { describe, expect, it, vi } from 'vitest';
import { allPanes, type PaneLayout, type PaneType } from './paneLayout';
import { paneToSplitIn, togglePane } from './paneActions';

// The layout a split reads, and whether the plugin that draws the
// Weather pane runs.
const state = vi.hoisted(() => ({ layout: null as PaneLayout | null, on: true }));
vi.mock('./panelLayoutStore', async (actual) => ({
  ...(await actual<typeof import('./panelLayoutStore')>()),
  getPanelLayout: () => state.layout,
  updatePanelLayout: (fn: (l: PaneLayout) => PaneLayout) => {
    if (state.layout) state.layout = fn(state.layout);
  },
}));
vi.mock('../stores/gmcp/immStore', () => ({ getImmState: () => ({ received: false }) }));
vi.mock('../stores/session/luaPanesStore', () => ({
  getLuaPanes: () =>
    new Map([
      [
        'weather',
        { plugin: 'weather_pane', id: 'weather', title: 'Weather', meta: '', blocks: [] },
      ],
    ]),
}));
vi.mock('../stores/session/pluginRowsStore', () => ({
  getPluginRows: () => [{ name: 'weather_pane', on: state.on, stopped: null }],
}));

// Each pane's leaf id is its type, with -2 and so on for a repeat.
function lay(...panes: PaneType[]): void {
  state.layout = {
    version: 1,
    panel_open: true,
    panel_width: null,
    root: {
      id: 'root',
      split: 'column',
      weight: 1,
      children: panes.map((pane, i) => {
        const n = panes.slice(0, i).filter((p) => p === pane).length;
        return { id: n === 0 ? pane : `${pane}-${n + 1}`, pane, weight: 1, props: {} };
      }),
    },
  };
}

describe('paneToSplitIn', () => {
  it('splits in the first built-in pane the panel does not show', () => {
    lay('map', 'affects');
    expect(paneToSplitIn()).toEqual({ pane: 'group', props: {} });
  });

  it('splits in a Lua pane once every built-in pane shows', () => {
    lay('map', 'affects', 'group', 'chat');
    expect(paneToSplitIn()).toEqual({
      pane: 'lua',
      props: { plugin: 'weather_pane', id: 'weather', title: 'Weather' },
    });
    state.on = false;
    expect(paneToSplitIn()).toBeNull();
  });
});

describe('togglePane', () => {
  it('closes only the first Chat pane', () => {
    lay('map', 'chat', 'affects', 'chat');
    togglePane('chat');
    expect(state.layout?.root.children.map((c) => c.id)).toEqual(['map', 'affects', 'chat-2']);
    expect(allPanes(state.layout!.root)).toEqual(['map', 'affects', 'chat']);
  });
});
