import { useEffect, useRef, useState } from 'react';
import { onRouted, onState, type RoutedPayload } from '../lib/session';

interface ChatLine {
  ts: number;
  pane: string;
  text: string;
}

const MAX_LINES = 500;

function formatTime(ts: number): string {
  const d = new Date(ts);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  const ss = String(d.getSeconds()).padStart(2, '0');
  return `${hh}:${mm}:${ss}`;
}

// Embedded bottom pane that collects routed channel text from
// session://routed. Trigger actions of kind `route` send their match
// text to a named pane; we buffer the most recent MAX_LINES entries
// across all panes so the user can keep one eye on chat without
// scrolling the terminal.
export function ChatPane() {
  const [lines, setLines] = useState<ChatLine[]>([]);
  const [filter, setFilter] = useState<string | null>(null);
  const bodyRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let unsubRouted: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;

    onRouted((payload: RoutedPayload) => {
      setLines((prev) => {
        const next = [...prev, { ts: Date.now(), pane: payload.pane, text: payload.text }];
        return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
      });
    }).then((fn) => {
      if (cancelled) fn();
      else unsubRouted = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') setLines([]);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    return () => {
      cancelled = true;
      unsubRouted?.();
      unsubState?.();
    };
  }, []);

  // Auto-scroll to the bottom on new lines unless the user has
  // scrolled up. We treat "near the bottom" as within 24px since exact
  // equality is fragile with sub-pixel scroll positions.
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
    <div className="chat-pane">
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
            no routed lines yet — wire a trigger with `route &lt;pane&gt;` to send text here
          </div>
        ) : (
          visible.map((l, i) => (
            <div key={i} className="chat-pane-line">
              <span className="chat-pane-ts">{formatTime(l.ts)}</span>
              <span className="chat-pane-tag">{l.pane}</span>
              <span className="chat-pane-text">{l.text}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
