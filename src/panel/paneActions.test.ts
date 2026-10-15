import { describe, expect, it, vi } from 'vitest';
import { allPanes, isLeaf, type PaneLayout, type PaneType } from './paneLayout';
import type { ToastInput } from '../stores/toasts';
import {
  chatRefToAdd,
  closeHere,
  closeWithUndo,
  dropPane,
  readyForChat,
  togglePane,
} from './paneActions';
import { paneTypesToAdd } from './paneTypes';

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
// The profile in front, which Close pane's Undo checks.
const front = vi.hoisted(() => ({ profile: 'Tolliver' as string | null }));
vi.mock('../stores/session/sessionsStore', async (actual) => ({
  ...(await actual<typeof import('../stores/session/sessionsStore')>()),
  profileInFront: () => front.profile,
}));
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
    togglePane('chat');
    expect(leaves()).toEqual(['map {}', 'chat-2 {}']);
    lay('map', 'chat');
    state.layout!.root.children[1] = {
      ...state.layout!.root.children[1],
      props: { channel: 'tell' },
    };
    closeHere('map');
    expect(leaves()).toEqual(['chat {"channel":"tell"}']);
  });
});

describe('a Chat pane joining another', () => {
  it('turns the pane on All to Everything else, and says it was alone there', () => {
    lay('map', 'chat');
    const { layout, turned } = readyForChat(state.layout!);
    state.layout = layout;
    expect(leaves()).toEqual(['map {}', 'chat {"rest":"1"}']);
    expect(turned).toBe(true);
  });

  it('leaves a pane on a channel alone', () => {
    lay('chat');
    state.layout!.root.children[0] = {
      ...state.layout!.root.children[0],
      props: { channel: 'say' },
    };
    const before = state.layout!;
    expect(readyForChat(before)).toEqual({ layout: before, turned: false });
  });
});

describe('Close pane', () => {
  it('closes the pane and offers it back where it was', () => {
    state.toasts = [];
    layBoth(['map', 'affects'], ['stats']);
    closeWithUndo('affects', 'Affects');
    expect(trees()).toEqual([['map'], ['stats']]);
    const toast = state.toasts.at(-1);
    expect(toast?.message).toBe('Closed Affects');
    toast?.action?.run();
    expect(trees()).toEqual([['map', 'affects'], ['stats']]);
  });

  it('puts nothing back once another profile is in front', () => {
    state.toasts = [];
    front.profile = 'Tolliver';
    layBoth(['map', 'affects'], []);
    closeWithUndo('affects', 'Affects');
    front.profile = 'Maren';
    state.toasts.at(-1)?.action?.run();
    expect(trees()).toEqual([['map'], []]);
    front.profile = 'Tolliver';
  });
});
