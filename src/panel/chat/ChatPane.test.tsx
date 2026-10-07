import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { parseCommChannel, parseRoutedLine, type ChatLine } from '../../stores/gmcp/chatStore';
import { findTheme, themeTokens } from '../../theme/themes';
import panelCss from '../../styles/panel.css?raw';
import { aabahranChatPacket } from '../../test/aabahranGmcp';
import { PaneLeafContext } from '../paneActions';
import { ChatLog, ChatPane } from './ChatPane';
import { CHAT_TAG_OPACITY, normalizeChatColors } from './chatColors';
import { chatTime } from '../paneText';

// The chat store reaches the Tauri bridge when it starts. ChatLog, under
// test, draws from plain lines and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The panel's tree, which a Chat pane reads for the channels the other
// Chat panes show on their own, and the live theme it draws in.
vi.mock('../panelLayoutStore', async (actual) => ({
  ...(await actual<typeof import('../panelLayoutStore')>()),
  usePanelLayout: () => null,
}));
vi.mock('../../theme/useActiveTheme', async () => {
  const { findTheme } = await import('../../theme/themes');
  return { useActiveTheme: () => findTheme('kanso-zen') };
});
vi.mock('../../theme/fitGameColors', async () => {
  const { findTheme } = await import('../../theme/themes');
  return { usePlayPalette: () => findTheme('kanso-zen').xterm };
});
vi.mock('../../stores/config/chatColorsStore', async () => {
  const { NO_CHAT_COLORS } = await import('./chatColors');
  return { useChatColors: () => NO_CHAT_COLORS };
});

const kanso = findTheme('kanso-zen').xterm;
const rubric = findTheme('rubric').xterm;
const kansoGround = themeTokens(findTheme('kanso-zen'));
const rubricGround = themeTokens(findTheme('rubric'));
const TS = new Date(2026, 8, 30, 20, 41).getTime();

const comm = (data: Record<string, unknown>): ChatLine => {
  const line = parseCommChannel(data, TS);
  if (!line) throw new Error('no line');
  return line;
};
const packet = (name: string): ChatLine => comm(aabahranChatPacket(name).data as never);
const routed = (pane: string, text: string): ChatLine => {
  const line = parseRoutedLine({ pane, text }, TS);
  if (!line) throw new Error('no line');
  return line;
};

// The sixteen messages on the approved board, as the store reads them.
const BOARD: ChatLine[] = [
  comm({
    channel: 'newbie',
    speaker: 'Fallenleaves',
    text: 'where do I train? the guard just laughs at me',
  }),
  comm({
    channel: 'newbie',
    speaker: 'Brannoc',
    text: 'find your guildmaster first, then type practice',
  }),
  packet('immortal.gmcp'),
  comm({
    channel: 'immortal',
    speaker: 'Ilsabet',
    text: 'fine by me, let me finish this note first',
  }),
  packet('tell.gmcp'),
  routed('tell', "You tell Tolliver 'yes, inside. north from the square'"),
  packet('say.gmcp'),
  comm({ channel: 'say', speaker: 'Ilsabet', text: 'take your time', language: 'common' }),
  comm({ channel: 'gtell', speaker: 'Joral', text: 'sanc is down, can someone recast?' }),
  comm({ channel: 'gtell', speaker: 'Ilsabet', text: 'one tick, waiting on mana' }),
  packet('cabal.gmcp'),
  packet('yell.gmcp'),
  comm({ channel: 'tell', speaker: 'Tolliver', text: 'omw, two minutes, grabbing my pack first' }),
  comm({
    channel: 'faction',
    speaker: 'Orvelle',
    text: 'Dovic just walked into the square, careful',
  }),
  comm({
    channel: 'immortal',
    speaker: 'Ysolde',
    text: 'reboot moved to half past, builders are saving',
  }),
  comm({ channel: 'newbie', speaker: 'Fallenleaves', text: 'found it, thanks!' }),
];

interface Drawn {
  color: string | null;
  title: string | null;
  tag: string | null;
  /** Whether the tag draws at full strength instead of a step back. */
  solidTag: boolean;
  speaker: string | null;
  text: string;
}

const unescape = (s: string) =>
  s
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');

/** Each message the log draws, read back out of its markup. */
function drawn(lines: ChatLine[], palette = kanso, ground = kansoGround): Drawn[] {
  const html = renderToStaticMarkup(<ChatLog lines={lines} palette={palette} ground={ground} />);
  return Array.from(html.matchAll(/<li([^>]*)>([\s\S]*?)<\/li>/g), ([, attrs, body]) => ({
    color: /style="color:([^";]+)"/.exec(attrs)?.[1] ?? null,
    title: /title="([^"]*)"/.exec(attrs)?.[1] ?? null,
    tag: /<span class="pane-chat-tag(?: is-solid)?">([^<]*)<\/span>/.exec(body)?.[1] ?? null,
    solidTag: body.includes('<span class="pane-chat-tag is-solid">'),
    speaker: /<span class="pane-chat-speaker">([^<]*)<\/span>/.exec(body)?.[1] ?? null,
    text: unescape(body.replace(/<[^>]+>/g, '')),
  }));
}

/** The declarations of one rule in panel.css. */
function rule(selector: string): string {
  const at = panelCss.indexOf(`\n${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return panelCss.slice(at, panelCss.indexOf('}', at));
}

describe('ChatLog', () => {
  it('prints each message on one line, [channel] Speaker: text', () => {
    expect(drawn(BOARD).map((m) => m.text)).toEqual([
      '[newbie]Fallenleaves: where do I train? the guard just laughs at me',
      '[newbie]Brannoc: find your guildmaster first, then type practice',
      '[immortal]Morrow: reboot at the top of the hour unless anyone objects',
      '[immortal]Ilsabet: fine by me, let me finish this note first',
      '[tell]Tolliver: are you still at the bank?',
      '[tell]to Tolliver: yes, inside. north from the square',
      '[say]Joral: grabbing my bank box, back soon',
      '[say]Ilsabet: take your time',
      '[gtell]Joral: sanc is down, can someone recast?',
      '[gtell]Ilsabet: one tick, waiting on mana',
      '[cabal]Grisvald: the gate at Blackwatch is open again',
      '[yell]a Blackwatch villager: Help! I am being attacked by Dovic!',
      '[tell]Tolliver: omw, two minutes, grabbing my pack first',
      '[faction]Orvelle: Dovic just walked into the square, careful',
      '[immortal]Ysolde: reboot moved to half past, builders are saving',
      '[newbie]Fallenleaves: found it, thanks!',
    ]);
  });

  it('draws the whole line in the channel color from the theme', () => {
    expect(drawn(BOARD).map((m) => m.color)).toEqual([
      '#87a987',
      '#87a987',
      '#e46876',
      '#e46876',
      '#8a9a7b',
      '#8a9a7b',
      '#e6c384',
      '#e6c384',
      '#938aa9',
      '#938aa9',
      '#7fb4ca',
      '#8ea4a2',
      '#8a9a7b',
      '#c4b28a',
      '#e46876',
      '#87a987',
    ]);
    // Rubric's verdigris tell reads at 4.3:1 on its panel, which sits on
    // the paper under the one ground rule, and its umber say at 11.9:1,
    // so both keep the colors the terminal draws.
    expect(drawn(BOARD.slice(4, 7), rubric, rubricGround).map((m) => m.color)).toEqual([
      '#007873',
      '#007873',
      '#3b2200',
    ]);
  });

  it('steps the tag back only where it still reads at 3:1 on the panel', () => {
    expect(drawn(BOARD).map((m) => m.solidTag)).toEqual(BOARD.map(() => false));
    // The tell's tag reads near 2.7:1 a step back, and the say's at 5:1.
    expect(drawn(BOARD.slice(4, 7), rubric, rubricGround).map((m) => m.solidTag)).toEqual([
      true,
      true,
      false,
    ]);
  });

  it('draws a channel you recolored in the theme slot you picked', () => {
    const picked = normalizeChatColors({ tell: 'brightRed' });
    const html = renderToStaticMarkup(
      <ChatLog lines={BOARD.slice(4, 7)} palette={kanso} ground={kansoGround} colors={picked} />,
    );
    const colors = [...html.matchAll(/style="color:([^";]+)"/g)].map((m) => m[1]);
    expect(colors).toEqual([kanso.brightRed, kanso.brightRed, '#e6c384']);
  });

  it('sets the tag apart and the whole speaker in bold', () => {
    const [newbie, , , , tell, sent, , , , , , yell] = drawn(BOARD);
    expect(newbie).toMatchObject({ tag: '[newbie]', speaker: 'Fallenleaves' });
    expect(tell).toMatchObject({ tag: '[tell]', speaker: 'Tolliver' });
    expect(sent).toMatchObject({ tag: '[tell]', speaker: 'Tolliver' });
    expect(yell).toMatchObject({ tag: '[yell]', speaker: 'a Blackwatch villager' });
    expect(drawn([packet('gtell-disguised.gmcp')])[0].speaker).toBe('{Joral} a shadow');
  });

  it('keeps a routed line in its own words', () => {
    const [loot] = drawn([routed('loot', 'You get a gold coin.')]);
    expect(loot).toMatchObject({
      tag: '[loot]',
      speaker: null,
      text: '[loot]You get a gold coin.',
    });
  });

  it('prints no time on the line and shows it when you point at the message', () => {
    const html = renderToStaticMarkup(
      <ChatLog lines={BOARD} palette={kanso} ground={kansoGround} />,
    );
    expect(html).not.toContain('<time');
    expect(html).not.toContain(chatTime(TS) + '<');
    for (const m of drawn(BOARD)) expect(m.title).toBe(chatTime(TS));
  });
});

describe('the chat line in panel.css', () => {
  it('sets the log in the game face at your panel size, 17 px lines at 12', () => {
    const log = rule('.pane-chat-log');
    // Your terminal font under As designed, your Panel font under any
    // other pick (src/panel/panelFont.test.ts).
    expect(log).toContain('font-family: var(--font-panel-game);');
    expect(log).toContain('font-size: var(--mud-text);');
    expect(log).toContain('line-height: var(--mud-chat-line);');
    // 8 above and 12 below at 12 px, scaled with the size
    // (paneTextSize.test.ts), and the side insets as they are.
    expect(log).toContain(
      'padding: round(8px * var(--mud-scale), 1px) 12px round(12px * var(--mud-scale), 1px) 18px;',
    );
  });

  it('hangs wrapped lines two cells in and keeps 3 px between messages at 12', () => {
    const msg = rule('.pane-chat-msg');
    expect(msg).toContain('padding: 0 0 0 2ch;');
    expect(msg).toContain('text-indent: -2ch;');
    expect(msg).toContain('white-space: pre-wrap;');
    expect(msg).toContain('overflow-wrap: break-word;');
    expect(rule('.pane-chat-msg + .pane-chat-msg')).toContain('margin-top: var(--mud-chat-gap);');
  });

  it('sets the tag at 600 and 0.7 and the speaker bold', () => {
    const tag = rule('.pane-chat-tag');
    expect(tag).toContain('margin-right: 6px;');
    expect(tag).toContain('font-weight: 600;');
    expect(tag).toContain('letter-spacing: 0.02em;');
    expect(tag).toContain(`opacity: ${CHAT_TAG_OPACITY};`);
    expect(rule('.pane-chat-tag.is-solid')).toContain('opacity: 1;');
    expect(rule('.pane-chat-speaker')).toContain('font-weight: 700;');
  });
});

describe('the pane select', () => {
  it('gives way before the more button and ends its words in an ellipsis', () => {
    const select = rule('.pane-select');
    expect(select).toContain('flex-shrink: 1000000;');
    const text = rule('.pane-select-text');
    expect(text).toContain('min-width: 0;');
    expect(text).toContain('overflow: hidden;');
    expect(text).toContain('text-overflow: ellipsis;');
    expect(rule('.pane-label:has(+ .pane-select)')).toContain('flex: 0 1 auto;');
  });

  it('keeps the chevron whole', () => {
    expect(rule('.pane-select-chevron')).toContain('flex: none;');
    expect(rule('.pane-label + .pane-select')).toContain(
      'min-width: calc(round(up, 1ch, 1px) + 16px);',
    );
  });
});

describe('ChatPane', () => {
  const header = (props: Record<string, string>) =>
    renderToStaticMarkup(
      <PaneLeafContext.Provider value={{ id: 'chat', pane: 'chat', weight: 1, props }}>
        <ChatPane />
      </PaneLeafContext.Provider>,
    );

  it('names its filter on the select and says what will show here', () => {
    const rest = header({ rest: '1' });
    expect(rest).toContain('aria-label="Channel, Everything else"');
    expect(rest).toContain('Messages on other channels appear here.');
    expect(header({ channel: 'tell' })).toContain('aria-label="Channel, tell"');
    expect(header({})).toContain('aria-label="Channel, All"');
  });
});
