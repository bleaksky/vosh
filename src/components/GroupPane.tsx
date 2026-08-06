import { useEffect, useState } from 'react';
import {
  getGroupState,
  subscribeGroupState,
  type GroupInfo,
  type GroupMember,
  type Worth,
} from '../lib/groupStore';

// Party roster + your-own-worth panel. Subscribes to Group.Info +
// Char.Worth via the module store (chatStore/groupStore pattern) so
// state survives close/reopen. Renders one line per member as a
// 6-column grid (display:contents per row). Placement (which zone
// it lives in, hidden / visible) is managed by Settings · Panels.
export function GroupPane() {
  const [state, setState] = useState<{ group: GroupInfo; worth: Worth }>(() => getGroupState());

  useEffect(() => subscribeGroupState(setState), []);

  const { group, worth } = state;
  const grouped = !!group.leader && Array.isArray(group.members) && group.members.length > 0;

  return (
    <div className="group-pane group-pane-pinned">
      <div className="chat-pane-header">
        <span className="chat-pane-title">group</span>
        <span className="chat-pane-count">{grouped ? `${group.members!.length}` : 'solo'}</span>
      </div>
      <div className="chat-pane-body">
        {grouped ? (
          <div className="group-list" role="list">
            {group.members!.map((m, i) => (
              <GroupRow
                key={`${m.name ?? '?'}-${i}`}
                member={m}
                isLeader={!!m.name && m.name === group.leader}
              />
            ))}
          </div>
        ) : (
          <SoloWorth worth={worth} />
        )}
      </div>
    </div>
  );
}

function GroupRow({ member, isLeader }: { member: GroupMember; isLeader: boolean }) {
  const hp = asPct(member.hp_pct);
  const mn = asPct(member.mana_pct);
  const mv = asPct(member.move_pct);
  return (
    <div className={`group-row${isLeader ? ' is-leader' : ''}`} role="listitem">
      <span className="group-name" title={member.name ?? ''}>
        {member.name ?? '?'}
      </span>
      <span className="group-meta">
        {(member.class ?? '?').toLowerCase()} {asNumberOrDash(member.level)}
      </span>
      <VitalNum pct={hp} />
      <VitalNum pct={mn} />
      <VitalNum pct={mv} />
      <span className="group-tnl">{formatTnl(member.tnl)}</span>
    </div>
  );
}

function SoloWorth({ worth }: { worth: Worth }) {
  const hasAny =
    worth.tnl !== undefined ||
    worth.gold !== undefined ||
    worth.exp !== undefined ||
    worth.trains !== undefined;
  if (!hasAny) {
    return (
      <div className="chat-pane-empty">
        no group. group up with `follow &lt;name&gt;` and members will list here.
      </div>
    );
  }
  return (
    <dl className="group-worth">
      {worth.tnl !== undefined && (
        <>
          <dt>tnl</dt>
          <dd>{asNumberOrDash(worth.tnl)}</dd>
        </>
      )}
      {worth.exp !== undefined && (
        <>
          <dt>exp</dt>
          <dd>{asNumberOrDash(worth.exp)}</dd>
        </>
      )}
      {worth.gold !== undefined && (
        <>
          <dt>gold</dt>
          <dd>{asNumberOrDash(worth.gold)}</dd>
        </>
      )}
      {worth.bank !== undefined && (
        <>
          <dt>bank</dt>
          <dd>{asNumberOrDash(worth.bank)}</dd>
        </>
      )}
      {worth.trains !== undefined && (
        <>
          <dt>trains</dt>
          <dd>{asNumberOrDash(worth.trains)}</dd>
        </>
      )}
      {worth.practices !== undefined && (
        <>
          <dt>prac</dt>
          <dd>{asNumberOrDash(worth.practices)}</dd>
        </>
      )}
    </dl>
  );
}

function VitalNum({ pct }: { pct: number | null }) {
  const color = pct !== null ? colorForPct(pct) : 'var(--c-text-dim)';
  return (
    <span className="group-vital" style={{ color }}>
      {pct !== null ? `${pct}%` : '-'}
    </span>
  );
}

function formatTnl(value: number | string | undefined): string {
  if (value === undefined || value === '') return '-';
  const n = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(n)) return '-';
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 10_000) return `${Math.round(n / 1_000)}k`;
  return String(n);
}

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
