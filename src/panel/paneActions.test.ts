import { describe, expect, it, vi } from 'vitest';
import { allPanes, isLeaf, paneRef, type PaneLayout, type PaneType } from './paneLayout';
import type { ToastInput } from '../stores/toasts';
import {
  addPaneAtBottom,
  addPaneToSidebar,
  chatRefToAdd,
  closeHere,
  dropPane,
  movePaneAcross,
  paneToSplitIn,
  showHereInstead,
  splitHere,
  togglePane,
} from './paneActions';
import { paneTypesToAdd, panesToShowInstead } from './paneTypes';

// The layout a split reads, whether the plugin that draws the
// Weather pane runs, and the toasts the actions raise.
const state = vi.hoisted(() => ({
  layout: null as PaneLayout | null,
  on: true,
  toasts: [] as ToastInput[],
}));
vi.mock('./panelLayoutStore', async (actual) => ({
  ...(await actual<typeof import('./panelLayoutStore')>()),
  getPanelLayout: () => state.layout,
  updatePanelLayout: (fn: (l: PaneLayout) => PaneLayout) => {
    if (state.layout) state.layout = fn(state.layout);
  },
  setPaneTree: (root: PaneLayout['root']) => {
    if (state.layout) state.layout = { ...state.layout, root };
  },
}));
vi.mock('../stores/toasts', () => ({
  pushToast: (input: ToastInput) => state.toasts.push(input),
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
  layBoth(panes, []);
}

// The panel holds `panes` and the sidebar `side`, ids as lay gives.
function layBoth(panes: PaneType[], side: PaneType[]): void {
  const all = [...panes, ...side];
  const leaf = (pane: PaneType, i: number) => {
    const n = all.slice(0, i).filter((p) => p === pane).length;
    return { id: n === 0 ? pane : `${pane}-${n + 1}`, pane, weight: 1, props: {} };
  };
  state.layout = {
    version: 1,
    panel_open: true,
    panel_width: null,
    sidebar: {
      id: 'sidebar',
      split: 'column',
      weight: 1,
      children: side.map((pane, i) => leaf(pane, panes.length + i)),
    },
    root: { id: 'root', split: 'column', weight: 1, children: panes.map(leaf) },
  };
}

// The pane ids of the panel and of the sidebar.
function trees(): [string[], string[]] {
  const ids = (tree: PaneLayout['root']) => tree.children.map((c) => c.id);
  return state.layout ? [ids(state.layout.root), ids(state.layout.sidebar)] : [[], []];
}

describe('the Writing pane', () => {
  it('is never offered, since only the card pins it', () => {
    lay('map', 'affects', 'group', 'stats', 'chat');
    expect(paneTypesToAdd(state.layout?.root ?? null)).not.toContain('writing');
    expect(paneTypesToAdd(null)).toEqual(['map', 'affects', 'group', 'stats', 'chat']);
    const map = state.layout?.root.children[0];
    if (!map || !isLeaf(map)) throw new Error('no map');
    expect(panesToShowInstead(map, state.layout?.root ?? null).builtIns).not.toContain('writing');
  });
});

describe('paneToSplitIn', () => {
  it('splits in the first built-in pane the panel does not show', () => {
    lay('map', 'affects');
    expect(paneToSplitIn('map')).toEqual({ pane: 'group', props: {} });
  });

  it('splits in a pane the panel does not show before another Chat', () => {
    state.on = true;
    lay('map', 'affects', 'group', 'stats', 'chat');
    expect(paneToSplitIn('map')).toEqual({
      pane: 'lua',
      props: { plugin: 'weather_pane', id: 'weather', title: 'Weather' },
    });
  });

  it('splits in another Chat on a Chat pane', () => {
    lay('map', 'affects', 'chat');
    expect(paneToSplitIn('chat')).toEqual({ pane: 'chat', props: {} });
  });

  it('splits in another Chat once the panel shows every pane', () => {
    state.on = false;
    lay('map', 'affects', 'group', 'stats', 'chat');
    expect(paneToSplitIn('map')).toEqual({ pane: 'chat', props: {} });
    lay('map', 'affects', 'group', 'stats', 'chat', 'chat', 'chat', 'chat');
    expect(paneToSplitIn('map')).toBeNull();
    expect(paneToSplitIn('chat')).toBeNull();
    state.on = true;
  });
});

describe('togglePane', () => {
  it('closes only the first Chat pane', () => {
    lay('map', 'chat', 'affects', 'chat');
    togglePane('chat');
    expect(state.layout?.root.children.map((c) => c.id)).toEqual(['map', 'affects', 'chat-2']);
    expect(allPanes(state.layout!.root)).toEqual(['map', 'affects', 'chat']);
  });

  it('closes a pane in the sidebar there, or moves it to the panel while the sidebar hides', () => {
    layBoth(['map'], ['stats', 'group']);
    togglePane('stats');
    expect(trees()).toEqual([['map'], ['group']]);
    state.layout = { ...state.layout!, panel_open: false };
    togglePane('group', false);
    expect(trees()).toEqual([['map', 'group'], []]);
    expect(state.layout?.panel_open).toBe(true);
  });
});

describe('the sidebar', () => {
  it('takes a new pane at its foot, and a pane the panel shows moves there', () => {
    layBoth(['map', 'stats'], ['group']);
    addPaneToSidebar(paneRef('affects'));
    expect(trees()).toEqual([
      ['map', 'stats'],
      ['group', 'affects'],
    ]);
    addPaneToSidebar(paneRef('stats'));
    expect(trees()).toEqual([['map'], ['group', 'affects', 'stats']]);
  });

  it('counts Chat panes across both trees', () => {
    layBoth(['chat', 'chat'], ['chat', 'chat']);
    addPaneToSidebar(paneRef('chat'));
    addPaneAtBottom(paneRef('chat'));
    expect(trees()).toEqual([
      ['chat', 'chat-2'],
      ['chat-3', 'chat-4'],
    ]);
  });

  it('turns the panel Chat pane to Everything else as one joins the sidebar', () => {
    state.toasts = [];
    layBoth(['map', 'chat'], []);
    addPaneToSidebar(paneRef('chat'));
    expect(leaves()).toEqual(['map {}', 'chat {"rest":"1"}']);
    expect(state.layout?.sidebar.children).toMatchObject([{ props: { channel: 'tell' } }]);
    expect(state.toasts).toHaveLength(1);
  });

  it('splits a pane in the sidebar with one neither tree shows', () => {
    layBoth(['map', 'affects', 'chat'], ['group']);
    expect(paneToSplitIn('group')).toEqual(paneRef('stats'));
    splitHere('group', 'column');
    expect(trees()).toEqual([
      ['map', 'affects', 'chat'],
      ['group', 'stats'],
    ]);
  });

  it('moves a pane across with Move to sidebar and Move to panel', () => {
    layBoth(['map', 'stats'], []);
    state.layout = { ...state.layout!, panel_open: false };
    movePaneAcross('stats');
    expect(trees()).toEqual([['map'], ['stats']]);
    expect(state.layout?.panel_open).toBe(false);
    movePaneAcross('stats');
    expect(trees()).toEqual([['map', 'stats'], []]);
    expect(state.layout?.panel_open).toBe(true);
  });

  it('moves a dropped pane where it lands, and opens the panel for it', () => {
    layBoth(['map', 'affects'], ['stats']);
    state.layout = { ...state.layout!, panel_open: false };
    dropPane('affects', { tree: 'sidebar', at: 'stats', edge: 'before' });
    expect(trees()).toEqual([['map'], ['affects', 'stats']]);
    expect(state.layout?.panel_open).toBe(false);
    dropPane('stats', { tree: 'panel', at: 'map', edge: 'before' });
    expect(trees()).toEqual([['stats', 'map'], ['affects']]);
    expect(state.layout?.panel_open).toBe(true);
  });

  it('returns the last Chat pane to All as its partner leaves the sidebar', () => {
    layBoth(['chat'], ['chat']);
    state.layout!.root.children[0] = { ...state.layout!.root.children[0], props: { rest: '1' } };
    closeHere('chat-2');
    expect(leaves()).toEqual(['chat {}']);
  });
});

// Each leaf's id and props, in reading order.
function leaves(): string[] {
  const out: string[] = [];
  const visit = (n: PaneLayout['root'] | PaneLayout['root']['children'][number]) => {
    if (isLeaf(n)) out.push(`${n.id} ${JSON.stringify(n.props)}`);
    else n.children.forEach(visit);
  };
  if (state.layout) visit(state.layout.root);
  return out;
}

describe('a new Chat pane', () => {
  it('starts on All alone, on tell beside another, then on Everything else or no channels', () => {
    expect(chatRefToAdd(null)).toEqual({ pane: 'chat', props: {} });
    lay('map', 'chat');
    expect(chatRefToAdd(state.layout!.root)).toEqual({ pane: 'chat', props: { channel: 'tell' } });
    state.layout!.root.children[1] = {
      ...state.layout!.root.children[1],
      props: { channel: 'tell' },
    };
    expect(chatRefToAdd(state.layout!.root)).toEqual({ pane: 'chat', props: { rest: '1' } });
    lay('chat', 'chat');
    state.layout!.root.children[1] = {
      ...state.layout!.root.children[1],
      props: { channel: 'gtell', channels: 'gtell,tell' },
    };
    // The first pane shows Everything else, so a third starts with no
    // channels for you to pick.
    expect(chatRefToAdd(state.layout!.root)).toEqual({ pane: 'chat', props: {} });
  });

  it('turns the pane on All to Everything else and says so', () => {
    state.toasts = [];
    lay('map', 'chat');
    addPaneAtBottom(paneRef('chat'));
    expect(leaves()).toEqual(['map {}', 'chat {"rest":"1"}', 'chat-2 {"channel":"tell"}']);
    expect(state.toasts.map((t) => [t.message, t.action?.label])).toEqual([
      ['Your other Chat pane now shows Everything else.', undefined],
    ]);
  });

  it('keeps Everything else on its pane as a third pane joins', () => {
    state.toasts = [];
    lay('chat', 'chat');
    state.layout!.root.children[1] = {
      ...state.layout!.root.children[1],
      props: { channel: 'tell' },
    };
    showHereInstead('chat-2', paneRef('chat'));
    addPaneAtBottom(paneRef('chat'));
    expect(leaves()).toEqual(['chat {"rest":"1"}', 'chat-2 {"channel":"tell"}', 'chat-3 {}']);
    expect(state.toasts).toEqual([]);
  });

  it('returns the last Chat pane to All as the others close', () => {
    lay('map', 'chat', 'chat');
    state.layout!.root.children[1] = {
      ...state.layout!.root.children[1],
      props: { channel: 'gtell', channels: 'gtell,tell' },
    };
    state.layout!.root.children[2] = {
      ...state.layout!.root.children[2],
      props: { rest: '1' },
    };
    closeHere('chat-2');
    expect(leaves()).toEqual(['map {}', 'chat {}']);
    lay('map', 'chat', 'chat');
    state.layout!.root.children[1] = {
      ...state.layout!.root.children[1],
      props: { channel: 'tell' },
    };
    showHereInstead('chat-2', paneRef('group'));
    // The leaf keeps its id as it turns to Group.
    expect(leaves()).toEqual(['map {}', 'chat {}', 'chat-2 {}']);
    lay('map', 'chat');
    state.layout!.root.children[1] = {
      ...state.layout!.root.children[1],
      props: { channel: 'tell' },
    };
    closeHere('map');
    expect(leaves()).toEqual(['chat {"channel":"tell"}']);
  });

  it('leaves a pane on a channel alone and raises no toast', () => {
    state.toasts = [];
    lay('chat');
    state.layout!.root.children[0] = {
      ...state.layout!.root.children[0],
      props: { channel: 'say' },
    };
    addPaneAtBottom(paneRef('chat'));
    expect(leaves()).toEqual(['chat {"channel":"say"}', 'chat-2 {"channel":"tell"}']);
    expect(state.toasts).toEqual([]);
  });

  it('starts on tell from Split and Show here instead too', () => {
    state.toasts = [];
    lay('map', 'affects', 'group', 'chat');
    splitHere('chat', 'column');
    expect(leaves().slice(3)).toEqual(['chat {"rest":"1"}', 'chat-2 {"channel":"tell"}']);
    lay('map', 'chat');
    showHereInstead('map', paneRef('chat'));
    expect(leaves()).toEqual(['map {"channel":"tell"}', 'chat {"rest":"1"}']);
    expect(state.toasts).toHaveLength(2);
  });

  it('changes nothing when no room is left for another Chat', () => {
    state.toasts = [];
    lay('chat', 'chat', 'chat', 'chat');
    addPaneAtBottom(paneRef('chat'));
    expect(leaves()).toEqual(['chat {}', 'chat-2 {}', 'chat-3 {}', 'chat-4 {}']);
    expect(state.toasts).toEqual([]);
  });
});
