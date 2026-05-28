import { useEffect, useRef, useState } from 'react';
import { getChatLines, subscribeChatLines, type ChatLine } from '../lib/chatStore';
import { GroupPane } from './GroupPane';
import { Resizable } from './Resizable';

interface Props {
  /** When true the group pane is rendered elsewhere (right column
   *  under the map), so the chat pane only shows the chat half. */
  groupPinned: boolean;
  onToggleGroupPin: () => void;
}

// Embedded bottom pane. By default it splits into two columns:
// chat backlog on the left, party roster on the right. When the
// user pins the group panel to the right column (under the map),
// the chat pane drops the split and shows chat only.
export function ChatPane({ groupPinned, onToggleGroupPin }: Props) {
  return (
    <div className="chat-pane">
      <div className="chat-pane-cols">
        <ChatColumn />
        {!groupPinned && (
          <Resizable
            storageKey="vosh.layout.groupWidth"
            defaultSize={260}
            minSize={120}
            maxSize={1600}
            reservePx={140}
            className="chat-pane-group-resizable"
            handleLabel="resize group column"
          >
            <GroupPane pinned={false} onTogglePin={onToggleGroupPin} />
          </Resizable>
        )}
      </div>
    </div>
  );
}

function ChatColumn() {
  const [lines, setLines] = useState<ChatLine[]>(() => getChatLines());
  const [filter, setFilter] = useState<string | null>(null);
  const bodyRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => subscribeChatLines(setLines), []);

  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - (el.scrollTop + el.clientHeight);
    if (distanceFromBottom < 24) {
      el.scrollTop = el.scrollHeight;
    }
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
