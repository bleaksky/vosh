import { useEffect, useRef, useState } from 'react';
import { onGmcp, onRouted, onState } from '../lib/session';

interface ChatLine {
  id: number;
  channel: string;
  text: string;
  ts: number;
}

const MAX_LINES = 500;

let nextId = 0;
const newId = () => {
  nextId += 1;
  return nextId;
};

function commTextLine(payload: unknown): ChatLine | null {
  if (!payload || typeof payload !== 'object') return null;
  const data = payload as Record<string, unknown>;
  // Aabahran's Comm.Channel push uses `channel`, `speaker`, `text`.
  // Other ROM derivatives sometimes send `chan` / `msg` / `message`,
  // so honour both spellings.
  const channel = String(data.channel ?? data.chan ?? 'chat');
  const speaker = data.speaker ? String(data.speaker) : '';
  const text = String(data.text ?? data.msg ?? data.message ?? '');
  if (!text) return null;
  const composed = speaker ? `${speaker}: ${text}` : text;
  return { id: newId(), channel, text: composed, ts: Date.now() };
}

function stripAnsi(text: string): string {
  // eslint-disable-next-line no-control-regex
  return text.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '');
}

export function ChatPane() {
  const [lines, setLines] = useState<ChatLine[]>([]);
  const bottomRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubRouted: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcp((payload) => {
      // Accept both the bare `Comm.Channel` package (Aabahran) and the
      // longer `Comm.Channel.Text` form some MUDs use.
      if (payload.package !== 'Comm.Channel' && payload.package !== 'Comm.Channel.Text') {
        return;
      }
      const line = commTextLine(payload.data);
      if (!line) return;
      setLines((prev) => [...prev.slice(-(MAX_LINES - 1)), line]);
    }).then((fn) => {
      unsubGmcp = fn;
    });

    onRouted((payload) => {
      const line: ChatLine = {
        id: newId(),
        channel: payload.pane,
        text: stripAnsi(payload.text),
        ts: Date.now(),
      };
      setLines((prev) => [...prev.slice(-(MAX_LINES - 1)), line]);
    }).then((fn) => {
      unsubRouted = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setLines([]);
      }
    }).then((fn) => {
      unsubState = fn;
    });

    return () => {
      unsubGmcp?.();
      unsubRouted?.();
      unsubState?.();
    };
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'auto', block: 'end' });
  }, [lines]);

  return (
    <section className="chat-pane" aria-label="chat">
      <header className="pane-header">chat</header>
      <div className="chat-body">
        {lines.length === 0 ? (
          <div className="chat-empty">no channel traffic yet</div>
        ) : (
          lines.map((l) => (
            <div key={l.id} className="chat-line">
              <span className="chat-channel">[{l.channel}]</span>{' '}
              <span className="chat-text">{l.text}</span>
            </div>
          ))
        )}
        <div ref={bottomRef} />
      </div>
    </section>
  );
}
