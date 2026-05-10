import { useEffect, useRef, useState } from 'react';
import { onGmcp, onRouted, onState } from '../lib/session';
import { useDockTarget } from '../lib/docking';

type TabId = 'chat' | 'wealth' | 'group';

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

interface GroupMember {
  name?: string;
  level?: number | string;
  class?: string;
  hp_pct?: number | string;
  mana_pct?: number | string;
  move_pct?: number | string;
  tnl?: number | string;
}

interface GroupInfo {
  leader?: string;
  members?: GroupMember[];
}

function asPct(value: unknown): number | null {
  if (value === undefined || value === null || value === '') return null;
  const n = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(n)) return null;
  return Math.max(0, Math.min(100, Math.round(n)));
}

// Same ramp as the bottom-rail HP/MN/MV bars so colors line up across
// the chrome.
function colorForPct(p: number): string {
  if (p >= 80) return '#87a987';
  if (p >= 60) return '#e6c384';
  if (p >= 40) return '#d99a6c';
  if (p >= 20) return '#e46876';
  return '#7d1d1d';
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
  const [group, setGroup] = useState<GroupInfo>({});
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const dockRef = useRef<HTMLElement>(null);
  useDockTarget(dockRef, 'info-tabs-pane');

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
        return;
      }
      if (payload.package === 'Group.Info') {
        // Solo state ships as `{}`; everything else carries leader +
        // members. Replace state wholesale on each push so a member
        // leaving doesn't linger.
        setGroup(
          payload.data && typeof payload.data === 'object'
            ? (payload.data as GroupInfo)
            : {},
        );
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
        setGroup({});
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
    <section ref={dockRef} className="info-tabs-pane" aria-label="info tabs">
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
          aria-selected={tab === 'wealth'}
          className={`info-tab${tab === 'wealth' ? ' is-active' : ''}`}
          onClick={() => setTab('wealth')}
        >
          wealth
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'group'}
          className={`info-tab${tab === 'group' ? ' is-active' : ''}`}
          onClick={() => setTab('group')}
        >
          group
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
        {tab === 'group' &&
          (group.leader && Array.isArray(group.members) && group.members.length > 0 ? (
            <ul className="group-list">
              {group.members.map((m, i) => {
                const hp = asPct(m.hp_pct);
                const mn = asPct(m.mana_pct);
                const mv = asPct(m.move_pct);
                const isLeader = !!m.name && m.name === group.leader;
                return (
                  <li
                    key={`${m.name ?? i}`}
                    className={`group-row${isLeader ? ' is-leader' : ''}`}
                  >
                    <div className="group-row-head">
                      <span className="group-name">{m.name ?? '?'}</span>
                      <span className="group-meta">
                        {(m.class ?? '?').toLowerCase()} {asNumberOrDash(m.level)}
                      </span>
                    </div>
                    <div className="group-row-vitals">
                      <span style={{ color: hp !== null ? colorForPct(hp) : '#73726a' }}>
                        HP {hp !== null ? `${hp}%` : '-'}
                      </span>
                      <span style={{ color: mn !== null ? colorForPct(mn) : '#73726a' }}>
                        MN {mn !== null ? `${mn}%` : '-'}
                      </span>
                      <span style={{ color: mv !== null ? colorForPct(mv) : '#73726a' }}>
                        MV {mv !== null ? `${mv}%` : '-'}
                      </span>
                    </div>
                  </li>
                );
              })}
            </ul>
          ) : (
            <div className="chat-empty">solo (no group)</div>
          ))}
        {tab === 'wealth' && (
          <dl className="info-grid">
            <dt>gold</dt>
            <dd>{asNumberOrDash(worth.gold)}</dd>
            <dt>bank</dt>
            <dd>{asNumberOrDash(worth.bank)}</dd>
            <dt>exp</dt>
            <dd>{asNumberOrDash(worth.exp)}</dd>
            <dt>tnl</dt>
            <dd>{asNumberOrDash(worth.tnl)}</dd>
            <dt>trains</dt>
            <dd>{asNumberOrDash(worth.trains)}</dd>
            <dt>prac</dt>
            <dd>{asNumberOrDash(worth.practices)}</dd>
            <dt>cabal pts</dt>
            <dd>{asNumberOrDash(worth.cps)}</dd>
            <dt>rp pts</dt>
            <dd>{asNumberOrDash(worth.rps)}</dd>
          </dl>
        )}
      </div>
    </section>
  );
}
