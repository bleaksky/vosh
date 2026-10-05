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
import { MenuItem, MenuSurface } from '../../ui/MenuSurface';
import { returnToCommandLine, updateLeafProps, usePaneLeaf } from '../paneActions';
import { PaneHeader } from '../PaneHeader';
import { CheckIcon, ChevronDownIcon } from '../../ui/paneIcons';
import { chatTime } from '../paneText';
import { usePaneText } from '../paneTextSize';

// Channel chat, the line you had from May to September on the theme
// (the approved Chat A board). Messages sit at the bottom like the
// terminal, one mono line each, [channel] Speaker: text in the color
// the game prints that channel in, or the theme color you picked for it
// under Channel colors in the pane menu, lifted where it would read
// under 3:1 on the panel (chatColors.ts). The channel filter lives in
// the pane's props, so it follows the profile and two chat panes can
// each show a different channel. The messages follow your panel size,
// and the lines and the gaps between them scale with it.

export function ChatPane() {
  const leaf = usePaneLeaf();
  const channel = leaf?.props.channel ?? '';
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
  const visible = channel ? lines.filter((l) => l.pane === channel) : lines;

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
  }, [channel]);

  return (
    <>
      <PaneHeader
        meta={
          leaf ? (
            <ChannelSelect
              channel={channel}
              channels={channels}
              onPick={(next) => updateLeafProps(leaf.id, { channel: next })}
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
            {channel
              ? `Messages on ${channel} appear here.`
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

function ChannelSelect({
  channel,
  channels,
  onPick,
}: {
  channel: string;
  channels: string[];
  onPick: (channel: string) => void;
}) {
  const ref = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const label = channel || 'All';
  const options = channel && !channels.includes(channel) ? [...channels, channel] : channels;
  const anchor = ref.current;
  const rect = open && anchor ? anchor.getBoundingClientRect() : null;

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
        {label}
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
          {['', ...options].map((c) => (
            <MenuItem
              key={c || '*'}
              onSelect={() => {
                setOpen(false);
                onPick(c);
                returnToCommandLine();
              }}
              trailing={c === channel ? <CheckIcon className="pane-menu-check" /> : null}
            >
              {c || 'All'}
            </MenuItem>
          ))}
        </MenuSurface>
      )}
    </>
  );
}
