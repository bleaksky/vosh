import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import {
  chatChannelSlot,
  chatInks,
  NO_CHAT_COLORS,
  type ChatColors,
  type ChatGround,
  type ChatInk,
} from './chatColors';
import { getChatLines, subscribeChatLines, type ChatLine } from '../../stores/gmcp/chatStore';
import { usePlayPalette } from '../../theme/fitGameColors';
import { useChatColors } from '../../stores/config/chatColorsStore';
import { themeTokens, type XtermPalette } from '../../theme/themes';
import { useActiveTheme } from '../../theme/useActiveTheme';
import { MenuItem, MenuSeparator, MenuSurface } from '../../ui/MenuSurface';
import { returnToCommandLine, updateLeafProps, usePaneLeaf } from '../paneActions';
import { usePanelLayout } from '../panelLayoutStore';
import { PaneHeader } from '../PaneHeader';
import { CheckIcon, ChevronDownIcon } from '../../ui/icons';
import { chatTime } from '../paneText';
import { usePaneText } from '../paneTextSize';
import {
  EVERYTHING_ELSE,
  chatFilterLabel,
  chatFilterOf,
  chatLinesFor,
  ownPaneChannels,
  type ChatFilter,
} from './chatFilter';

// Channel chat, the line you had from May to September on the theme
// (the approved Chat A board). Messages sit at the bottom like the
// terminal, one mono line each, [channel] Speaker: text in the color
// the game prints that channel in, or the theme color you picked for it
// under Channel colors in the pane menu, lifted where it would read
// under 3:1 on the panel (chatColors.ts). The filter lives in the
// pane's props, so it follows the profile and two chat panes can each
// show a different channel, or one a channel and the other Everything
// else (chatFilter.ts). The messages follow your panel size, and the
// lines and the gaps between them scale with it.

export function ChatPane() {
  const leaf = usePaneLeaf();
  const tree = usePanelLayout()?.root ?? null;
  const filter: ChatFilter = leaf ? chatFilterOf(leaf) : { kind: 'all' };
  const label = chatFilterLabel(filter);
  const owned = useMemo(() => ownPaneChannels(tree, leaf?.id ?? ''), [tree, leaf?.id]);
  const [lines, setLines] = useState<ChatLine[]>(() => getChatLines());
  const theme = useActiveTheme();
  const palette = usePlayPalette();
  const ground = useMemo(() => themeTokens(theme), [theme]);
  const colors = useChatColors();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const stickyRef = useRef(true);
  // Distance from the bottom that still counts as reading the newest
  // message, about a message at your panel size. Scrolled further
  // up, new lines leave you be.
  const text = usePaneText();
  const sticky = text.chatSticky;

  useEffect(() => subscribeChatLines(setLines), []);

  const channels = Array.from(new Set(lines.map((l) => l.pane))).sort();
  const visible = chatLinesFor(lines, filter, owned);

  // Follow the newest line while you are at the bottom. A new terminal
  // size makes every line taller or shorter with the pane the same
  // size, so it pins the log to the bottom again too.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el && stickyRef.current) el.scrollTop = el.scrollHeight;
  }, [lines, text.size]);

  // A pane that grows or shrinks (a splitter drag, the window) keeps
  // the newest line in view while you are at the bottom.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      if (stickyRef.current) el.scrollTop = el.scrollHeight;
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // A new filter is a fresh view, so it starts at the bottom.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    stickyRef.current = true;
  }, [label]);

  return (
    <>
      <PaneHeader
        meta={
          leaf ? (
            <ChannelSelect
              filter={filter}
              channels={channels}
              owned={owned}
              onPick={(next) =>
                updateLeafProps(leaf.id, {
                  channel: next.kind === 'channel' ? next.channel : '',
                  rest: next.kind === 'rest' ? '1' : '',
                })
              }
            />
          ) : null
        }
      />
      <div
        ref={scrollRef}
        className={`pane-body pane-chat-scroll${visible.length === 0 ? ' is-empty' : ''}`}
        onScroll={(e) => {
          const el = e.currentTarget;
          stickyRef.current = el.scrollHeight - (el.scrollTop + el.clientHeight) < sticky;
        }}
      >
        {visible.length === 0 ? (
          <p className="pane-empty">
            {filter.kind === 'channel'
              ? `Messages on ${filter.channel} appear here.`
              : filter.kind === 'rest'
                ? 'Messages on other channels appear here.'
                : 'Chat appears when someone talks on a channel.'}
          </p>
        ) : (
          <ChatLog lines={visible} palette={palette} ground={ground} colors={colors} />
        )}
      </div>
    </>
  );
}

// chatStore appends line objects and never changes one, so an id per
// object keys each message across the 500 line roll off.
const lineIds = new WeakMap<ChatLine, number>();
let nextLineId = 0;

function lineKey(line: ChatLine): number {
  let id = lineIds.get(line);
  if (id === undefined) {
    id = nextLineId++;
    lineIds.set(line, id);
  }
  return id;
}

/** The messages, oldest first, each in its channel's color on the
 *  theme whose terminal palette and panel are given: the slot you picked
 *  under Channel colors, or the one the game prints the channel in,
 *  lifted to 3:1 on the panel. */
export function ChatLog({
  lines,
  palette,
  ground,
  colors = NO_CHAT_COLORS,
}: {
  lines: ChatLine[];
  palette: XtermPalette;
  ground: ChatGround;
  colors?: ChatColors;
}) {
  const inks = useMemo(() => chatInks(palette, ground), [palette, ground]);
  return (
    <ol className="pane-chat-log">
      {lines.map((line) => (
        <ChatMessage
          key={lineKey(line)}
          line={line}
          ink={inks[chatChannelSlot(line.pane, colors)]}
        />
      ))}
    </ol>
  );
}

// One line per message, [tell] Tolliver: text, the whole line in the
// channel's color. A tell you send reads to Tolliver. A routed line keeps
// its own words after the tag. The tag steps back unless that would
// take it under 3:1 on the panel. The arrival time shows only when you
// point at the message.
function ChatMessage({ line, ink }: { line: ChatLine; ink: ChatInk }) {
  return (
    <li className="pane-chat-msg" style={{ color: ink.color }} title={chatTime(line.ts)}>
      <span className={ink.fadeTag ? 'pane-chat-tag' : 'pane-chat-tag is-solid'}>
        [{line.pane}]
      </span>
      {line.speaker !== null && (
        <>
          {line.direction === 'sent' && 'to '}
          <span className="pane-chat-speaker">{line.speaker}</span>
          {': '}
        </>
      )}
      {line.text}
    </li>
  );
}

// The filter menu: All, Everything else, then the channels heard plus
// the one the pane names, sorted. A channel another Chat pane shows on
// its own says so.
function ChannelSelect({
  filter,
  channels,
  owned,
  onPick,
}: {
  filter: ChatFilter;
  channels: string[];
  owned: ReadonlySet<string>;
  onPick: (filter: ChatFilter) => void;
}) {
  const ref = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const label = chatFilterLabel(filter);
  const named = filter.kind === 'channel' ? filter.channel : '';
  const options = named && !channels.includes(named) ? [...channels, named].sort() : channels;
  const anchor = ref.current;
  const rect = open && anchor ? anchor.getBoundingClientRect() : null;
  const pick = (next: ChatFilter) => () => {
    setOpen(false);
    onPick(next);
    returnToCommandLine();
  };
  const check = <CheckIcon className="pane-menu-check" />;

  return (
    <>
      <button
        ref={ref}
        type="button"
        className="pane-select"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Channel, ${label}`}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="pane-select-text">{label}</span>
        <ChevronDownIcon size={12} className="pane-select-chevron" />
      </button>
      {open && anchor && rect && (
        <MenuSurface
          label="Channel"
          className="pane-menu-narrow"
          anchor={anchor}
          at={{ x: rect.left - 12, y: rect.bottom + 8, flipY: rect.top - 8 }}
          onClose={(reason) => {
            setOpen(false);
            if (reason !== 'outside') returnToCommandLine();
          }}
        >
          <MenuItem
            onSelect={pick({ kind: 'all' })}
            trailing={filter.kind === 'all' ? check : null}
          >
            All
          </MenuItem>
          <MenuItem
            onSelect={pick({ kind: 'rest' })}
            trailing={filter.kind === 'rest' ? check : null}
          >
            {EVERYTHING_ELSE}
          </MenuItem>
          {options.length > 0 && <MenuSeparator />}
          {options.map((c) => (
            <MenuItem
              key={c}
              onSelect={pick({ kind: 'channel', channel: c })}
              trailing={
                c === named ? (
                  check
                ) : owned.has(c) ? (
                  <span className="shell-menu-kbd">own pane</span>
                ) : null
              }
            >
              {c}
            </MenuItem>
          ))}
        </MenuSurface>
      )}
    </>
  );
}
