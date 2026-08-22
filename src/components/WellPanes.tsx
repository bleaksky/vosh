import { useEffect, useRef, useState } from 'react';
import { chatChannelColor } from '../lib/chatColors';
import { getChatLines, subscribeChatLines, type ChatLine } from '../lib/chatStore';
import { getLogLines, startLogTail, subscribeLogTail, type LogLine } from '../lib/logTail';

// The chat and raw-log panes that live inside the terminal well when
// splits are open. Each carries the canvas pane chip (number + caps
// name) and a bottom-anchored body.

function useStickyBottom(dep: unknown) {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [dep]);
  return ref;
}

function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

function PaneChip({ num, name, active }: { num: string; name: string; active?: boolean }) {
  return (
    <div className="well-pane-chip-row">
      <span className={`well-pane-chip${active ? ' is-active' : ''}`}>
        <span className="well-pane-chip-num">{num}</span>
        <span className="well-pane-chip-name">{name}</span>
      </span>
    </div>
  );
}

export function ChatWellPane() {
  const [lines, setLines] = useState<ChatLine[]>(() => getChatLines());
  useEffect(() => subscribeChatLines(setLines), []);
  const bodyRef = useStickyBottom(lines.length);
  return (
    <div className="well-pane well-pane-chat">
      <PaneChip num="2" name="chat" />
      <div ref={bodyRef} className="well-chat-body">
        {lines.length === 0 ? (
          <div className="well-chat-line" style={{ color: 'var(--c-text-dim)' }}>
            no channel chat yet
          </div>
        ) : (
          lines.map((l, i) => (
            <div key={i} className="well-chat-line" style={{ color: chatChannelColor(l.pane) }}>
              <span className="well-chat-ts">{fmtTime(l.ts)}</span>
              <span>{l.text}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

export function LogWellPane() {
  const [, bump] = useState(0);
  useEffect(() => {
    startLogTail();
    return subscribeLogTail(() => bump((n) => n + 1));
  }, []);
  const lines: LogLine[] = getLogLines();
  const bodyRef = useStickyBottom(lines.length);
  return (
    <div className="well-pane well-pane-log">
      <PaneChip num="3" name="log · raw" />
      <div ref={bodyRef} className="well-log-body">
        {lines.length === 0 ? (
          <div>quiet — raw session output tails here</div>
        ) : (
          lines.map((l, i) => (
            <div key={i}>
              {fmtTime(l.ts)} {l.text}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
