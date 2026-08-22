import { useEffect, useState } from 'react';
import {
  getGroupState,
  subscribeGroupState,
  type GroupMember,
  type GroupState,
  type Worth,
} from '../lib/groupStore';

// Party roster + your-own-worth panel. Subscribes to Group.Info +
// Char.Worth via the module store (chatStore/groupStore pattern) so
// state survives close/reopen. Renders one Ember row per member:
// mono name, 44px hp mini-bar, hp% — health at a glance, colored by
// tier. Placement (which zone it lives in, hidden / visible) is
// managed by Settings · Panels.
export function GroupPane() {
  const [state, setState] = useState<GroupState>(() => getGroupState());

  useEffect(() => subscribeGroupState(setState), []);

  const { group, worth, self } = state;
  const grouped = !!group.leader && Array.isArray(group.members) && group.members.length > 0;

  return (
    <div className="group-pane group-pane-pinned">
      <div className="chat-pane-header">
        <span className="chat-pane-title">group</span>
        <span className="chat-pane-count">{grouped ? `${group.members!.length}` : 'solo'}</span>
      </div>
      <div className="chat-pane-body">
        {grouped ? (
          <div className="group-rows" role="list">
            {group.members!.map((m, i) => (
              <GroupRow
                key={`${m.name ?? '?'}-${i}`}
                member={m}
                isLeader={!!m.name && m.name === group.leader}
                isSelf={!!m.name && !!self && m.name.toLowerCase() === self.toLowerCase()}
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

function GroupRow({
  member,
  isLeader,
  isSelf,
}: {
  member: GroupMember;
  isLeader: boolean;
  isSelf: boolean;
}) {
  const hp = asPct(member.hp_pct);
  const tone = hp !== null ? hpTone(hp) : 'var(--c-text-dim)';
  return (
    <div
      className={`group-row-ember${isLeader ? ' is-leader' : ''}${isSelf ? ' is-self' : ''}`}
      role="listitem"
    >
      <span className="group-row-name" title={member.name ?? ''}>
        {member.name ?? '?'}
      </span>
      <span className="group-row-bar">
        {hp !== null && (
          <span className="group-row-fill" style={{ width: `${hp}%`, background: tone }} />
        )}
      </span>
      <span className="group-row-hp" style={{ color: tone }}>
        {hp !== null ? `${hp}%` : '-'}
      </span>
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

/// Three-tier health tone for the roster bar + percent: top third
/// healthy, middle third warning, bottom third danger.
function hpTone(p: number): string {
  if (p >= 67) return 'var(--c-success)';
  if (p >= 34) return 'var(--c-warn)';
  return 'var(--c-danger)';
}

function asPct(value: unknown): number | null {
  if (value === undefined || value === null || value === '') return null;
  const n = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(n)) return null;
  return Math.max(0, Math.min(100, Math.round(n)));
}

function asNumberOrDash(value: number | string | undefined): string {
  if (value === undefined || value === '') return '-';
  return String(value);
}
