import { useEffect, useRef, useState } from 'react';
import { getChatLines, subscribeChatLines, type ChatLine } from '../lib/chatStore';

interface Props {
  /** Hide the chat panel. The host re-renders with the panel in
   *  the `hidden` zone so the close button is the canonical way
   *  to dismiss the pane from inside its own chrome. */
  onClose?: () => void;
}

// Chat backlog panel. Renders into whichever zone the user has
// assigned via Settings → Panels. The group roster is no longer
// embedded here — it lives as its own panel.
export function ChatPane({ onClose }: Props) {
  // Stop wheel events from leaving the chat panel. Without this, when
  // the chat body has no scrollable content (messages fit on screen),
  // a wheel gesture bubbles up to the terminal area and triggers the
  // split-scrollback gesture or xterm scroll. Capture phase + stop
  // propagation keeps every wheel event inside chat.
  const onWheel = (e: React.WheelEvent<HTMLDivElement>) => {
    e.stopPropagation();
  };
  return (
    <div className="chat-pane" onWheelCapture={onWheel}>
      {onClose ? <ChatColumn onClose={onClose} /> : <ChatColumn />}
    </div>
  );
}

function ChatColumn({ onClose }: { onClose?: () => void }) {
  const [lines, setLines] = useState<ChatLine[]>(() => getChatLines());
  const [filter, setFilter] = useState<string | null>(null);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  // Stick-to-bottom flag. Starts `true` so the first batch of lines to
  // arrive after mount snaps to the bottom; only flips to false once
  // the user scrolls up to read back. Returns to true when the user
  // scrolls back within the 24px threshold of the bottom. Read-back
  // therefore pauses autoscroll without locking it out.
  const stickyBottomRef = useRef(true);

  useEffect(() => subscribeChatLines(setLines), []);

  // Track user-driven scrolling so the sticky flag follows where the
  // viewport actually is. Programmatic scrollTop = scrollHeight also
  // triggers a scroll event but lands at distance 0, so the flag
  // settles back to true and we don't get pongs.
  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const onScroll = () => {
      const distance = el.scrollHeight - (el.scrollTop + el.clientHeight);
      stickyBottomRef.current = distance < 24;
    };
    el.addEventListener('scroll', onScroll);
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  // Snap to the bottom on first mount so opening the chat panel shows
  // the most recent lines instead of the oldest, and again whenever
  // new lines arrive while the sticky flag is on.
  useEffect(() => {
    const el = bodyRef.current;
    if (!el || !stickyBottomRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [lines]);

  const panes = Array.from(new Set(lines.map((l) => l.pane))).sort();
  const visible = filter ? lines.filter((l) => l.pane === filter) : lines;

  return (
    <div className="chat-pane-half chat-pane-chat">
      <div className="chat-pane-header">
        <span className="chat-pane-title">chat</span>
        <div className="chat-pane-filter">
          <button
            type="button"
            className={`chat-pane-tab${filter === null ? ' chat-pane-tab-active' : ''}`}
            onClick={() => setFilter(null)}
          >
            all
          </button>
          {panes.map((p) => (
            <button
              key={p}
              type="button"
              className={`chat-pane-tab${filter === p ? ' chat-pane-tab-active' : ''}`}
              onClick={() => setFilter(p)}
            >
              {p}
            </button>
          ))}
        </div>
        <span className="chat-pane-count">
          {visible.length}
          {filter && `/${lines.length}`}
        </span>
        {onClose && (
          <button
            type="button"
            className="chat-pane-close"
            onClick={onClose}
            aria-label="hide chat"
            title="hide chat"
          >
            ×
          </button>
        )}
      </div>
      <div ref={bodyRef} className="chat-pane-body">
        {visible.length === 0 ? (
          <div className="chat-pane-empty">
            no channel chat yet — Comm.Channel GMCP arrives here automatically; trigger actions of
            kind `route &lt;pane&gt;` route here too
          </div>
        ) : (
          visible.map((l, i) => (
            <div key={i} className="chat-pane-line">
              <span className="chat-pane-tag">[{l.pane}]</span>
              <span className="chat-pane-text">{l.text}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
