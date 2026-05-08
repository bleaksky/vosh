import { useEffect, useRef, useState } from 'react';
import { onGmcp, onRouted, onState } from '../lib/session';

type TabId = 'chat' | 'gold' | 'cabal';

interface ChatLine {
  id: number;
  channel: string;
  text: string;
}

interface Worth {
  gold?: number | string;
  bank?: number | string;
  exp?: number | string;
  tnl?: number | string;
  trains?: number | string;
  practices?: number | string;
  cps?: number | string;
  rps?: number | string;
}

const MAX_CHAT_LINES = 500;

let nextId = 0;
const newId = () => {
  nextId += 1;
  return nextId;
};

function asNumberOrDash(value: number | string | undefined): string {
  if (value === undefined || value === '') return '-';
  return String(value);
}

function chatFromComm(payload: unknown): ChatLine | null {
  if (!payload || typeof payload !== 'object') return null;
  const data = payload as Record<string, unknown>;
  const channel = String(data.channel ?? data.chan ?? 'chat');
  const speaker = data.speaker ? String(data.speaker) : '';
  const text = String(data.text ?? data.msg ?? data.message ?? '');
  if (!text) return null;
  return { id: newId(), channel, text: speaker ? `${speaker}: ${text}` : text };
}

function stripAnsi(text: string): string {
  // eslint-disable-next-line no-control-regex
  return text.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '');
}

export function InfoTabsPane() {
  const [tab, setTab] = useState<TabId>('chat');
  const [lines, setLines] = useState<ChatLine[]>([]);
  const [worth, setWorth] = useState<Worth>({});
  const bottomRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubRouted: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcp((payload) => {
      if (
        payload.package === 'Comm.Channel' ||
        payload.package === 'Comm.Channel.Text'
      ) {
        const line = chatFromComm(payload.data);
        if (line) {
          setLines((prev) => [...prev.slice(-(MAX_CHAT_LINES - 1)), line]);
        }
        return;
      }
      if (payload.package === 'Char.Worth' && payload.data && typeof payload.data === 'object') {
        setWorth((prev) => ({ ...prev, ...(payload.data as Worth) }));
      }
    }).then((fn) => {
      unsubGmcp = fn;
    });

    onRouted((payload) => {
      const line: ChatLine = {
        id: newId(),
        channel: payload.pane,
        text: stripAnsi(payload.text),
      };
      setLines((prev) => [...prev.slice(-(MAX_CHAT_LINES - 1)), line]);
    }).then((fn) => {
      unsubRouted = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setLines([]);
        setWorth({});
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
    if (tab === 'chat') {
      bottomRef.current?.scrollIntoView({ behavior: 'auto', block: 'end' });
    }
  }, [lines, tab]);

  return (
    <section className="info-tabs-pane" aria-label="info tabs">
      <header className="info-tabs-header" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'chat'}
          className={`info-tab${tab === 'chat' ? ' is-active' : ''}`}
          onClick={() => setTab('chat')}
        >
          chat
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'gold'}
          className={`info-tab${tab === 'gold' ? ' is-active' : ''}`}
          onClick={() => setTab('gold')}
        >
          gold
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'cabal'}
          className={`info-tab${tab === 'cabal' ? ' is-active' : ''}`}
          onClick={() => setTab('cabal')}
        >
          cabal
        </button>
      </header>
      <div className="info-tabs-body" role="tabpanel">
        {tab === 'chat' &&
          (lines.length === 0 ? (
            <div className="chat-empty">no channel traffic yet</div>
          ) : (
            <>
              {lines.map((l) => (
                <div key={l.id} className="chat-line">
                  <span className="chat-channel">[{l.channel}]</span>{' '}
                  <span className="chat-text">{l.text}</span>
                </div>
              ))}
              <div ref={bottomRef} />
            </>
          ))}
        {tab === 'gold' && (
          <ul className="info-list">
            <li>
              <span className="info-key">gold</span>
              <span className="info-value">{asNumberOrDash(worth.gold)}</span>
            </li>
            <li>
              <span className="info-key">bank</span>
              <span className="info-value">{asNumberOrDash(worth.bank)}</span>
            </li>
            <li>
              <span className="info-key">exp</span>
              <span className="info-value">{asNumberOrDash(worth.exp)}</span>
            </li>
            <li>
              <span className="info-key">tnl</span>
              <span className="info-value">{asNumberOrDash(worth.tnl)}</span>
            </li>
            <li>
              <span className="info-key">trains</span>
              <span className="info-value">{asNumberOrDash(worth.trains)}</span>
            </li>
            <li>
              <span className="info-key">prac</span>
              <span className="info-value">{asNumberOrDash(worth.practices)}</span>
            </li>
          </ul>
        )}
        {tab === 'cabal' && (
          <ul className="info-list">
            <li>
              <span className="info-key">cabal pts</span>
              <span className="info-value">{asNumberOrDash(worth.cps)}</span>
            </li>
            <li>
              <span className="info-key">renown pts</span>
              <span className="info-value">{asNumberOrDash(worth.rps)}</span>
            </li>
          </ul>
        )}
      </div>
    </section>
  );
}
