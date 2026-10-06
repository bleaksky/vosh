import { Fragment, isValidElement, type ReactElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CHAT_CHANNELS, chatChannelColor, type ChatColors } from './chat/chatColors';
import type { PaneLayout, PaneLeaf, PaneType } from './paneLayout';
import { resetChatColors, setChatColor } from '../ipc/uiConfig';
import { openSettingsTab } from '../lib/settingsLink';
import { findTheme } from '../theme/themes';
import { MenuItem, MenuSeparator } from '../ui/MenuSurface';
import { ChannelColorItems, ChannelColorRows, PaneMenu, ShowHereRows } from './PaneMenu';
import { panesToShowInstead } from './paneTypes';
import { PaneTextSizeContext } from './paneTextSize';

// The stores behind the menu reach the Tauri bridge when they start.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('../ipc/uiConfig', async (actual) => ({
  ...(await actual<typeof import('../ipc/uiConfig')>()),
  resetChatColors: vi.fn(() => Promise.resolve()),
  setChatColor: vi.fn(() => Promise.resolve()),
}));
vi.mock('../ipc/profiles', async (actual) => ({
  ...(await actual<typeof import('../ipc/profiles')>()),
  profilesList: vi.fn(() => new Promise(() => undefined)),
}));
// The menu draws its rows in place, with no page to portal into, and
// reads the stores' values without the running app. The rows of the
// last menu drawn stay at hand, so a test can pick one.
const drawn = vi.hoisted(() => ({ rows: null as unknown }));
vi.mock('../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: ReactNode }) => {
    drawn.rows = children;
    return <menu aria-label={label}>{children}</menu>;
  },
}));
// The display the menu reads, the default unless a test picks a style.
const shown = vi.hoisted(() => ({ style: null as string | null }));
vi.mock('../stores/config/affectsDisplayStore', async () => {
  const { DEFAULT_AFFECTS_DISPLAY } = await import('../ipc/affects');
  return {
    useAffectsDisplay: () =>
      shown.style ? { ...DEFAULT_AFFECTS_DISPLAY, style: shown.style } : DEFAULT_AFFECTS_DISPLAY,
  };
});
vi.mock('../stores/config/chatColorsStore', () => ({ useChatColors: () => new Map() }));
vi.mock('../lib/settingsLink', () => ({ openSettingsTab: vi.fn() }));
// The layout the menu splits, none unless a test lays one out.
const laid = vi.hoisted(() => ({ layout: null as PaneLayout | null }));
vi.mock('./panelLayoutStore', async (actual) => ({
  ...(await actual<typeof import('./panelLayoutStore')>()),
  getPanelLayout: () => laid.layout,
}));
// The Lua panes the session in front holds, Weather and Worth, and
// whether their plugins run, neither unless a test turns them on.
const lua = vi.hoisted(() => ({ on: false }));
vi.mock('../stores/session/luaPanesStore', () => ({
  getLuaPanes: () =>
    new Map(
      ['worth', 'weather'].map((id) => [
        id,
        {
          plugin: `${id}_pane`,
          id,
          title: id[0].toUpperCase() + id.slice(1),
          meta: '',
          blocks: [],
        },
      ]),
    ),
}));
vi.mock('../stores/session/pluginRowsStore', () => ({
  getPluginRows: () =>
    ['weather_pane', 'worth_pane'].map((name) => ({ name, on: lua.on, stopped: null })),
}));
vi.mock('../theme/useActiveTheme', async () => {
  const { findTheme: find } = await import('../theme/themes');
  return { useActiveTheme: () => find('kanso-zen') };
});

const palette = findTheme('kanso-zen').xterm;

interface ItemProps {
  children: ReactNode;
  onSelect?: () => void;
  disabled?: boolean;
  trailing?: ReactNode;
  submenu?: { open: boolean; controls: string; onOpen: (focus: boolean) => void };
  onHover?: () => void;
  onFocus?: () => void;
}
type Item = ReactElement<ItemProps>;

/** The menu rows and separators in `node`, in order, as the menu lays
 *  them out. A row is its element, a separator is the string `---`. */
function rows(node: ReactNode): (Item | '---')[] {
  const out: (Item | '---')[] = [];
  const walk = (n: ReactNode) => {
    if (Array.isArray(n)) {
      n.forEach(walk);
      return;
    }
    if (!isValidElement(n)) return;
    if (n.type === MenuItem) out.push(n as Item);
    else if (n.type === MenuSeparator) out.push('---');
    else if (n.type === Fragment) walk((n.props as { children?: ReactNode }).children);
  };
  walk(node);
  return out;
}

const items = (node: ReactNode): Item[] => rows(node).filter((r): r is Item => r !== '---');

/** A row's text, without its swatch. */
function label(item: Item): string {
  const kids = item.props.children;
  return (Array.isArray(kids) ? kids : [kids]).filter((k) => typeof k === 'string').join('');
}

/** The color of a row's swatch, or null for none. */
function swatch(item: Item): string | null {
  const kids = item.props.children;
  const dot = (Array.isArray(kids) ? kids : [kids]).find(isValidElement) as
    | ReactElement<{ color: string }>
    | undefined;
  return dot?.props.color ?? null;
}

function channelRows(colors: ChatColors, open: string | null = null) {
  const calls = {
    open: vi.fn<(channel: string, focus: boolean) => void>(),
    leave: vi.fn<(channel: string | null) => void>(),
    done: vi.fn(),
  };
  const node = ChannelColorRows({
    colors,
    palette,
    open,
    listId: (channel) => `colors-${channel}`,
    rowRef: () => undefined,
    onOpen: calls.open,
    onLeave: calls.leave,
    done: calls.done,
  });
  return { list: rows(node), all: items(node), ...calls };
}

beforeEach(() => {
  vi.mocked(resetChatColors).mockClear();
  vi.mocked(setChatColor).mockClear();
  shown.style = null;
  laid.layout = null;
  lua.on = false;
});

describe('PaneMenu', () => {
  const anchor = {
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 0, bottom: 0 }),
    closest: () => null,
  } as unknown as HTMLButtonElement;
  const menu = (pane: PaneType) => {
    const leaf: PaneLeaf = { id: `leaf-${pane}`, pane, weight: 1, props: {} };
    return renderToStaticMarkup(<PaneMenu leaf={leaf} anchor={anchor} onClose={() => {}} />);
  };

  /** Each menu row's text, `(off)` after a disabled one. */
  const menuRows = (html: string) =>
    [...html.matchAll(/<button[^>]*role="menuitem"([^>]*)>(.*?)<\/button>/g)].map(
      ([, attrs, inner]) =>
        `${inner.replace(/<[^>]*>/g, '')}${attrs.includes('aria-disabled="true"') ? ' (off)' : ''}`,
    );

  it('offers Style, Marker, Change when affects warn and Edit tracked affects in the Affects menu', () => {
    const rows = menuRows(menu('affects'));
    const at = rows.indexOf('Marker');
    expect(rows[at - 1]).toBe('Style');
    expect(rows[at + 1]).toBe('Change when affects warn…');
    expect(rows[at + 2]).toBe('Edit tracked affects…');
    for (const pane of ['map', 'chat', 'group'] as const) {
      expect(menu(pane), pane).not.toContain('Change when affects warn');
    }
  });

  it('quiets Marker for both chip styles', () => {
    for (const [style, off] of [
      ['timers', false],
      ['countdown', false],
      ['chips', true],
      ['chips_drain', true],
    ] as const) {
      shown.style = style;
      const rows = menuRows(menu('affects'));
      expect(rows, style).toContain(off ? 'Marker (off)' : 'Marker');
      // The hours apply to every style.
      expect(rows, style).toContain('Change when affects warn…');
    }
  });

  it('offers Channel colors in the Chat pane menu alone', () => {
    expect(menu('chat')).toContain('Channel colors');
    for (const pane of ['map', 'affects', 'group'] as const) {
      expect(menu(pane), pane).not.toContain('Channel colors');
    }
  });

  it('offers to edit the plugin of a Lua pane between Show here instead and Close pane', () => {
    const leaf: PaneLeaf = {
      id: 'leaf-weather',
      pane: 'lua',
      weight: 1,
      props: { plugin: 'weather_pane', id: 'weather', title: 'Weather' },
    };
    renderToStaticMarkup(<PaneMenu leaf={leaf} anchor={anchor} onClose={() => {}} />);
    const node = drawn.rows as ReactNode;
    const list = rows(node).map((row) => (row === '---' ? row : label(row)));
    expect(list.slice(-5)).toEqual([
      'Show here instead',
      '---',
      'Edit weather_pane in Scripts…',
      '---',
      'Close pane',
    ]);
    const edit = items(node).find((row) => label(row).startsWith('Edit'));
    // Closing hands the caret back to the command line.
    vi.stubGlobal('window', { dispatchEvent: () => true });
    edit?.props.onSelect?.();
    vi.unstubAllGlobals();
    expect(openSettingsTab).toHaveBeenCalledWith('scripts:weather_pane');
    for (const pane of ['map', 'affects', 'group', 'chat'] as const) {
      expect(menu(pane), pane).not.toContain('in Scripts');
    }
  });

  it('offers a split only when every pane keeps its minimum at your panel size', () => {
    // Map, Affects and Chat in a 300 by 650 panel, so a split adds Group.
    laid.layout = {
      version: 1,
      panel_open: true,
      panel_width: 300,
      root: {
        id: 'root',
        split: 'column',
        weight: 1,
        children: [
          { id: 'map', pane: 'map', weight: 0.4, props: {} },
          { id: 'affects', pane: 'affects', weight: 0.35, props: {} },
          { id: 'leaf-chat', pane: 'chat', weight: 0.25, props: {} },
        ],
      },
    };
    const area = { clientWidth: 300, clientHeight: 650 };
    const inPanel = {
      getBoundingClientRect: () => ({ left: 0, top: 0, right: 0, bottom: 0 }),
      closest: (selector: string) => (selector === '.panel-panes' ? area : null),
    } as unknown as HTMLButtonElement;
    const leaf: PaneLeaf = { id: 'leaf-chat', pane: 'chat', weight: 1, props: {} };
    const splits = (size: number) =>
      menuRows(
        renderToStaticMarkup(
          <PaneTextSizeContext.Provider value={size}>
            <PaneMenu leaf={leaf} anchor={inPanel} onClose={() => {}} />
          </PaneTextSizeContext.Provider>,
        ),
      ).filter((row) => row.startsWith('Split'));
    // Splitting Chat down needs 557 px at 12 px and 738 px at 16 px.
    // Side by side, the panes keep the heights they have now, 462 px
    // and 613 px.
    expect(splits(12)).toEqual(['Split right', 'Split down']);
    expect(splits(16)).toEqual(['Split right', 'Split down (off)']);
  });
});

describe('ChannelColorRows', () => {
  it('lists each channel with a dot in its color, then Reset all', () => {
    const { list, all } = channelRows(new Map([['tell', 'brightMagenta']]));
    expect(all.map(label)).toEqual([...CHAT_CHANNELS, 'Reset all']);
    expect(list.at(-2)).toBe('---');
    expect(swatch(all[1])).toBe(palette.brightMagenta);
    expect(swatch(all[0])).toBe(chatChannelColor('say', palette));
  });

  it('keeps Reset all quiet until you recolor a channel', () => {
    expect(channelRows(new Map()).all.at(-1)?.props.disabled).toBe(true);
    expect(channelRows(new Map([['say', 'red']])).all.at(-1)?.props.disabled).toBe(false);
  });

  it('clears every pick from Reset all and closes the menu', () => {
    const { all, done } = channelRows(new Map([['say', 'red']]));
    all.at(-1)?.props.onSelect?.();
    expect(resetChatColors).toHaveBeenCalledTimes(1);
    expect(done).toHaveBeenCalledTimes(1);
    expect(setChatColor).not.toHaveBeenCalled();
  });

  it('opens a channel from its row and closes the others', () => {
    const { all, open, leave } = channelRows(new Map(), 'yell');
    const yell = all[CHAT_CHANNELS.indexOf('yell')];
    expect(yell.props.submenu?.open).toBe(true);
    expect(yell.props.submenu?.controls).toBe('colors-yell');
    expect(all.filter((r) => r.props.submenu?.open)).toHaveLength(1);
    all[0].props.submenu?.onOpen(true);
    expect(open).toHaveBeenCalledWith('say', true);
    all[0].props.onFocus?.();
    expect(leave).toHaveBeenLastCalledWith('say');
    all.at(-1)?.props.onHover?.();
    expect(leave).toHaveBeenLastCalledWith(null);
  });
});

describe('ChannelColorItems', () => {
  const list = (colors: ChatColors) => {
    const done = vi.fn();
    const node = ChannelColorItems({ channel: 'tell', colors, palette, done });
    return { list: rows(node), all: items(node), done };
  };

  it('offers Default, set apart, then the theme colors, with a check on the pick', () => {
    const { list: shown, all } = list(new Map([['tell', 'red']]));
    expect(shown[1]).toBe('---');
    expect(all).toHaveLength(17);
    expect(label(all[0])).toBe('Default');
    expect(label(all[2])).toBe('Red');
    expect(swatch(all[2])).toBe(palette.red);
    expect(all.filter((r) => r.props.trailing).map(label)).toEqual(['Red']);
  });

  it('saves the pick for that channel and closes the menu', () => {
    const { all, done } = list(new Map());
    all[2].props.onSelect?.();
    expect(setChatColor).toHaveBeenLastCalledWith('tell', 'red');
    all[0].props.onSelect?.();
    expect(setChatColor).toHaveBeenLastCalledWith('tell', null);
    expect(done).toHaveBeenCalledTimes(2);
  });
});

describe('Show here instead', () => {
  const weather: PaneLeaf = {
    id: 'leaf-weather',
    pane: 'lua',
    weight: 1,
    props: { plugin: 'weather_pane', id: 'weather', title: 'Weather' },
  };

  const listed = (leaf: PaneLeaf) => {
    const pick = vi.fn();
    const list = rows(ShowHereRows({ ...panesToShowInstead(leaf), pick }));
    const text = list.map((row) => {
      if (row === '---') return row;
      const plugin = row.props.trailing as ReactElement<{ children: string }> | null;
      return plugin ? `${label(row)} | ${plugin.props.children}` : label(row);
    });
    return { text, all: list.filter((r): r is Item => r !== '---'), pick };
  };

  it('lists the other panes, then a rule and the Lua panes in title order', () => {
    lua.on = true;
    const leaf: PaneLeaf = { id: 'leaf-chat', pane: 'chat', weight: 1, props: {} };
    expect(listed(leaf).text).toEqual([
      'Map',
      'Affects',
      'Group',
      '---',
      'Weather | weather_pane',
      'Worth | worth_pane',
    ]);
  });

  it('leaves out the Lua pane it is and those whose plugin is off', () => {
    lua.on = true;
    expect(listed(weather).text).toEqual([
      'Map',
      'Affects',
      'Group',
      'Chat',
      '---',
      'Worth | worth_pane',
    ]);
    lua.on = false;
    expect(listed(weather).text).toEqual(['Map', 'Affects', 'Group', 'Chat']);
  });

  it('shows the Lua pane picked in place of the pane', () => {
    lua.on = true;
    const { all, pick } = listed(weather);
    all.find((row) => label(row) === 'Worth')?.props.onSelect?.();
    expect(pick).toHaveBeenCalledWith({
      pane: 'lua',
      props: { plugin: 'worth_pane', id: 'worth', title: 'Worth' },
    });
  });
});
