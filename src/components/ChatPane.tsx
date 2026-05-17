import { useEffect, useRef, useState } from 'react';
import { onGmcp, onRouted, onState, type GmcpPayload, type RoutedPayload } from '../lib/session';

interface ChatLine {
  pane: string;
  text: string;
}

const MAX_LINES = 500;

// eslint-disable-next-line no-control-regex
const ANSI_RE = /\x1b\[[0-9;]*[A-Za-z]/g;

function stripAnsi(text: string): string {
  return text.replace(ANSI_RE, '');
}

// Convert a Comm.Channel(.Text) GMCP payload to a ChatLine. Aabahran
// and other ROM-derived servers vary the field names; fall back through
// the common alternates.
function commToChatLine(data: unknown): ChatLine | null {
  if (!data || typeof data !== 'object') return null;
  const obj = data as Record<string, unknown>;
  const pane = String(obj.channel ?? obj.chan ?? 'chat');
  const speaker = obj.speaker
    ? String(obj.speaker)
    : obj.talker
      ? String(obj.talker)
      : '';
  const raw = String(obj.text ?? obj.msg ?? obj.message ?? '');
  if (!raw) return null;
  const cleaned = stripAnsi(raw);
  return { pane, text: speaker ? `${speaker}: ${cleaned}` : cleaned };
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
    let unsubGmcp: (() => void) | undefined;
    let unsubRouted: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;

    const append = (line: ChatLine) => {
      setLines((prev) => {
        const next = [...prev, line];
        return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
      });
    };

    // Primary feed: Comm.Channel.Text GMCP from the server. Aabahran
    // pushes channel chat here automatically; no trigger setup needed.
    onGmcp((payload: GmcpPayload) => {
      if (payload.package !== 'Comm.Channel' && payload.package !== 'Comm.Channel.Text') {
        return;
      }
      const line = commToChatLine(payload.data);
      if (line) append(line);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    // Secondary feed: trigger actions of kind `route` push hand-picked
    // terminal lines here. Useful for capturing patterns the server
    // does not surface through Comm.Channel.
    onRouted((payload: RoutedPayload) => {
      append({ pane: payload.pane, text: stripAnsi(payload.text) });
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
      unsubGmcp?.();
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
            no channel chat yet — Comm.Channel GMCP arrives here automatically; trigger
            actions of kind `route &lt;pane&gt;` route here too
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
