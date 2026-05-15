import { useEffect, useRef, useState } from 'react';
import {
  colorForDuration,
  formatDuration,
  formatModifier,
  groupAffects,
  type Affect,
  type GroupedAffect,
} from '../lib/affects';
import { onGmcp, onRouted, onState } from '../lib/session';

type TabId = 'chat' | 'wealth' | 'group' | 'affects';

interface Props {
  open: boolean;
  onClose: () => void;
}

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

const MAX_CHAT_LINES = 500;

let nextId = 0;
const newId = () => {
  nextId += 1;
  return nextId;
};

function asPct(value: unknown): number | null {
  if (value === undefined || value === null || value === '') return null;
  const n = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(n)) return null;
  return Math.max(0, Math.min(100, Math.round(n)));
}

function colorForPct(p: number): string {
  if (p >= 80) return '#87a987';
  if (p >= 60) return '#e6c384';
  if (p >= 40) return '#d99a6c';
  if (p >= 20) return '#e46876';
  return '#7d1d1d';
}

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

// Slide-out tray from the right edge that houses Chat, Wealth, Group,
// and the full Affects browser. Summoned by F2 (or the ⋯ button in
// the BottomHUD); dismissed by Escape or clicking the backdrop. Keeps
// the side panel free for the map alone.
export function AuxDrawer({ open, onClose }: Props) {
  const [tab, setTab] = useState<TabId>('chat');
  const [lines, setLines] = useState<ChatLine[]>([]);
  const [worth, setWorth] = useState<Worth>({});
  const [group, setGroup] = useState<GroupInfo>({});
  const [affectGroups, setAffectGroups] = useState<GroupedAffect[]>([]);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const bottomRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubRouted: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcp((payload) => {
      const pkg = payload.package;
      if (pkg === 'Comm.Channel' || pkg === 'Comm.Channel.Text') {
        const line = chatFromComm(payload.data);
        if (line) {
          setLines((prev) => [...prev.slice(-(MAX_CHAT_LINES - 1)), line]);
        }
        return;
      }
      if (pkg === 'Char.Worth' && payload.data && typeof payload.data === 'object') {
        setWorth((prev) => ({ ...prev, ...(payload.data as Worth) }));
        return;
      }
      if (pkg === 'Group.Info') {
        setGroup(
          payload.data && typeof payload.data === 'object'
            ? (payload.data as GroupInfo)
            : {},
        );
        return;
      }
      if (pkg === 'Char.Affects') {
        const data = payload.data as { affects?: Affect[] } | undefined;
        const list = Array.isArray(data?.affects) ? data!.affects : [];
        const grouped = groupAffects(list);
        grouped.sort((a, b) => a.name.localeCompare(b.name));
        setAffectGroups(grouped);
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
        setAffectGroups([]);
        setExpanded({});
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
    if (open && tab === 'chat') {
      bottomRef.current?.scrollIntoView({ behavior: 'auto', block: 'end' });
    }
  }, [lines, tab, open]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [open, onClose]);

  const toggleAffect = (name: string) => {
    setExpanded((prev) => ({ ...prev, [name]: !prev[name] }));
  };

  return (
    <>
      <div
        className={`aux-backdrop${open ? ' is-open' : ''}`}
        onClick={onClose}
        aria-hidden={!open}
      />
      <aside
        className={`aux-drawer${open ? ' is-open' : ''}`}
        aria-label="auxiliary drawer"
        aria-hidden={!open}
      >
        <header className="aux-drawer-header" role="tablist">
          {(['chat', 'wealth', 'group', 'affects'] as TabId[]).map((id) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              className={`aux-tab${tab === id ? ' is-active' : ''}`}
              onClick={() => setTab(id)}
            >
              {id}
            </button>
          ))}
          <button
            type="button"
            className="aux-close"
            onClick={onClose}
            aria-label="close drawer"
            title="close (Esc)"
          >
            ×
          </button>
        </header>
        <div className="aux-drawer-body" role="tabpanel">
          {tab === 'chat' &&
            (lines.length === 0 ? (
              <div className="aux-empty">no channel traffic yet</div>
            ) : (
              <div className="aux-chat">
                {lines.map((l) => (
                  <div key={l.id} className="chat-line">
                    <span className="chat-channel">[{l.channel}]</span>{' '}
                    <span className="chat-text">{l.text}</span>
                  </div>
                ))}
                <div ref={bottomRef} />
              </div>
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
              <div className="aux-empty">solo (no group)</div>
            ))}
          {tab === 'affects' &&
            (affectGroups.length === 0 ? (
              <div className="aux-empty">no active affects</div>
            ) : (
              <ul className="affects-list">
                {affectGroups.map((grp) => {
                  const isOpen = expanded[grp.name] ?? false;
                  const hasDetails = grp.modifiers.length > 0 || !!grp.description;
                  const color = colorForDuration(grp.duration);
                  return (
                    <li key={grp.name} className="affect-row">
                      <button
                        type="button"
                        className="affect-row-head"
                        onClick={() => toggleAffect(grp.name)}
                        aria-expanded={isOpen}
                        disabled={!hasDetails}
                      >
                        <span className="affect-row-toggle" aria-hidden="true">
                          {hasDetails ? (isOpen ? '▾' : '▸') : '·'}
                        </span>
                        <span className="affect-row-name">{grp.name}</span>
                        {grp.level !== undefined && (
                          <span className="affect-row-level">L{grp.level}</span>
                        )}
                        <span className="affect-row-dur" style={{ color }}>
                          {formatDuration(grp.duration)}
                        </span>
                      </button>
                      {isOpen && hasDetails && (
                        <div className="affect-row-detail">
                          {grp.modifiers.length > 0 && (
                            <ul className="affect-mods">
                              {grp.modifiers.map((mod, i) => (
                                <li key={`${mod.location}:${i}`} className="affect-mod">
                                  <span className="affect-mod-loc">{mod.location}</span>
                                  <span
                                    className={`affect-mod-val${
                                      Number(mod.modifier) > 0 ? ' is-positive' : ''
                                    }${Number(mod.modifier) < 0 ? ' is-negative' : ''}`}
                                  >
                                    {formatModifier(mod.modifier)}
                                  </span>
                                </li>
                              ))}
                            </ul>
                          )}
                          {grp.description && (
                            <div className="affect-desc">{grp.description}</div>
                          )}
                        </div>
                      )}
                    </li>
                  );
                })}
              </ul>
            ))}
        </div>
      </aside>
    </>
  );
}
