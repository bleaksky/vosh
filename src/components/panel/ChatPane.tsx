import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { getChatLines, subscribeChatLines, type ChatLine } from '../../lib/chatStore';
import { MenuItem, MenuSurface } from './MenuSurface';
import { updateLeafProps, usePaneLeaf } from './paneActions';
import { PaneHeader } from './PaneHeader';
import { CheckIcon, ChevronDownIcon } from './paneIcons';
import { chatTime, splitSpeaker } from './paneText';

// Channel chat (SPEC 9). Messages sit at the bottom like the terminal:
// a quiet time and channel line, then the message with the speaker in
// bold. The channel filter lives in the pane's props, so it follows
// the profile and two chat panes can each show a different channel.

// Distance from the bottom, in pixels, that still counts as reading
// the newest message. Scrolled further up, new lines leave you be.
const STICKY_PX = 24;

export function ChatPane() {
  const leaf = usePaneLeaf();
  const channel = leaf?.props.channel ?? '';
  const [lines, setLines] = useState<ChatLine[]>(() => getChatLines());
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const stickyRef = useRef(true);

  useEffect(() => subscribeChatLines(setLines), []);

  const channels = Array.from(new Set(lines.map((l) => l.pane))).sort();
  const visible = channel ? lines.filter((l) => l.pane === channel) : lines;

  // Follow the newest line while you are at the bottom.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el && stickyRef.current) el.scrollTop = el.scrollHeight;
  }, [lines]);

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
        className="pane-body pane-chat-scroll"
        onScroll={(e) => {
          const el = e.currentTarget;
          stickyRef.current = el.scrollHeight - (el.scrollTop + el.clientHeight) < STICKY_PX;
        }}
      >
        {visible.length === 0 ? (
          <p className="pane-empty">
            {channel
              ? `Messages on ${channel} appear here.`
              : 'Chat appears when someone talks on a channel.'}
          </p>
        ) : (
          <ol className="pane-chat-log">
            {visible.map((line, i) => (
              <ChatMessage key={`${line.ts}-${i}`} line={line} />
            ))}
          </ol>
        )}
      </div>
    </>
  );
}

function ChatMessage({ line }: { line: ChatLine }) {
  const { speaker, text } = splitSpeaker(line.text);
  return (
    <li className="pane-chat-msg">
      <div className="pane-chat-when">
        <time dateTime={new Date(line.ts).toISOString()}>{chatTime(line.ts)}</time>
        <span className="pane-chat-channel">{line.pane}</span>
      </div>
      <p className="pane-chat-text">
        {speaker && <span className="pane-chat-speaker">{speaker}</span>}
        {speaker ? ` ${text}` : text}
      </p>
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
            if (reason === 'escape') anchor.focus();
          }}
        >
          {['', ...options].map((c) => (
            <MenuItem
              key={c || '*'}
              onSelect={() => {
                setOpen(false);
                onPick(c);
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
