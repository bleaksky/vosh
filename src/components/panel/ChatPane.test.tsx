import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { parseCommChannel, parseRoutedLine, type ChatLine } from '../../lib/chatStore';
import { findTheme, themeTokens } from '../../lib/themes';
import panelCss from '../../styles/panel.css?raw';
import { aabahranChatPacket } from '../../test/aabahranGmcp';
import { ChatLog } from './ChatPane';
import { CHAT_TAG_OPACITY, normalizeChatColors } from '../../lib/chatColors';
import { chatTime } from './paneText';

// The chat store reaches the Tauri bridge when it starts. ChatLog, under
// test, draws from plain lines and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const kanso = findTheme('kanso-zen').xterm;
const vellum = findTheme('vellum').xterm;
const kansoGround = themeTokens(findTheme('kanso-zen'));
const vellumGround = themeTokens(findTheme('vellum'));
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
    speaker: 'Erelei',
    text: 'fine by me, let me finish this note first',
  }),
  packet('tell.gmcp'),
  routed('tell', "You tell Tolliver 'yes, inside. north from the square'"),
  packet('say.gmcp'),
  comm({ channel: 'say', speaker: 'Erelei', text: 'take your time', language: 'common' }),
  comm({ channel: 'gtell', speaker: 'Joral', text: 'sanc is down, can someone recast?' }),
  comm({ channel: 'gtell', speaker: 'Erelei', text: 'one tick, waiting on mana' }),
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
      '[immortal]Erelei: fine by me, let me finish this note first',
      '[tell]Tolliver: are you still at the bank?',
      '[tell]to Tolliver: yes, inside. north from the square',
      '[say]Joral: grabbing my bank box, back soon',
      '[say]Erelei: take your time',
      '[gtell]Joral: sanc is down, can someone recast?',
      '[gtell]Erelei: one tick, waiting on mana',
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
    // Vellum's bright yellow sits at 2.87:1 on its panel, so say darkens
    // to 3:1 there. The terminal keeps #b88226.
    expect(drawn(BOARD.slice(4, 7), vellum, vellumGround).map((m) => m.color)).toEqual([
      '#4f7a3a',
      '#4f7a3a',
      '#b37d1f',
    ]);
  });

  it('steps the tag back only where it still reads at 3:1 on the panel', () => {
    expect(drawn(BOARD).map((m) => m.solidTag)).toEqual(BOARD.map(() => false));
    expect(drawn(BOARD.slice(4, 7), vellum, vellumGround).map((m) => m.solidTag)).toEqual([
      true,
      true,
      true,
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
  it('sets the log in the terminal face at 12 on a 17 px line', () => {
    const log = rule('.pane-chat-log');
    expect(log).toContain('font-family: var(--font-mud);');
    expect(log).toContain('font-size: 12px;');
    expect(log).toContain('line-height: 17px;');
    expect(log).toContain('padding: 8px 12px 12px 18px;');
  });

  it('hangs wrapped lines two cells in and keeps 3 px between messages', () => {
    const msg = rule('.pane-chat-msg');
    expect(msg).toContain('padding: 0 0 0 2ch;');
    expect(msg).toContain('text-indent: -2ch;');
    expect(msg).toContain('white-space: pre-wrap;');
    expect(msg).toContain('overflow-wrap: break-word;');
    expect(rule('.pane-chat-msg + .pane-chat-msg')).toContain('margin-top: 3px;');
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
