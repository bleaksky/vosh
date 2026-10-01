import { Fragment, isValidElement, type ReactElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CHAT_CHANNELS, chatChannelColor, type ChatColors } from '../../lib/chatColors';
import type { PaneLeaf, PaneType } from '../../lib/paneLayout';
import { resetChatColors, setChatColor } from '../../lib/session';
import { findTheme } from '../../lib/themes';
import { MenuItem, MenuSeparator } from './MenuSurface';
import { ChannelColorItems, ChannelColorRows, PaneMenu } from './PaneMenu';

// The stores behind the menu reach the Tauri bridge when they start.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('../../lib/session', async (actual) => ({
  ...(await actual<typeof import('../../lib/session')>()),
  resetChatColors: vi.fn(() => Promise.resolve()),
  setChatColor: vi.fn(() => Promise.resolve()),
  profilesList: vi.fn(() => new Promise(() => undefined)),
}));
// The menu draws its rows in place, with no page to portal into, and
// reads the stores' values without the running app.
vi.mock('./MenuSurface', async (actual) => ({
  ...(await actual<typeof import('./MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: ReactNode }) => (
    <menu aria-label={label}>{children}</menu>
  ),
}));
vi.mock('../../lib/stores/affectsDisplayStore', async () => {
  const { DEFAULT_AFFECTS_DISPLAY } = await import('../../lib/session');
  return { useAffectsDisplay: () => DEFAULT_AFFECTS_DISPLAY };
});
vi.mock('../../lib/stores/chatColorsStore', () => ({ useChatColors: () => new Map() }));
vi.mock('../../lib/useActiveTheme', async () => {
  const { findTheme: find } = await import('../../lib/themes');
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

  it('offers Channel colors in the Chat pane menu alone', () => {
    expect(menu('chat')).toContain('Channel colors');
    for (const pane of ['map', 'affects', 'group'] as const) {
      expect(menu(pane), pane).not.toContain('Channel colors');
    }
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
