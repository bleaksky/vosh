import { useSyncExternalStore } from 'react';
import {
  getGroupState,
  memberKey,
  subscribeGroupState,
  type GroupInfo,
  type GroupMember,
} from '../../stores/gmcp/groupStore';
import { thirdsTone } from '../vitalsView';
import { PaneHeader, PaneMeta } from '../PaneHeader';

// Your group at a glance. One dense row per member: the name,
// a `lead` tag on the leader, a 48 by 3 health meter, and the percent.
// The meter and percent stay quiet until a member drops into the
// middle third (warn) or the bottom third (danger). Your own vitals
// read the same thirds while Warn before you run low is on.
//
// While the game hides your group (Group.Info with the hidden flag,
// under lamented tears) the pane says so in place of the roster. The
// store keeps no roster from before, so no stale health shows.

export function GroupPane() {
  const { group } = useSyncExternalStore(subscribeGroupState, getGroupState);
  return <GroupPaneView group={group} />;
}

/** The pane drawn from a plain Group.Info, so each state renders in a
 *  test. */
export function GroupPaneView({ group }: { group: GroupInfo }) {
  const hidden = group.hidden === true;
  const members = !hidden && group.leader && Array.isArray(group.members) ? group.members : [];

  let empty: string | null = null;
  if (hidden) empty = 'The game hides your group right now.';
  else if (members.length === 0) empty = 'Group appears when you join a group.';

  return (
    <>
      <PaneHeader
        meta={members.length > 0 ? <PaneMeta tone="quiet">{members.length}</PaneMeta> : null}
      />
      <div className="pane-body">
        {empty !== null ? (
          <p className="pane-empty">{empty}</p>
        ) : (
          <ul className="pane-rows">
            {members.map((m) => (
              <MemberRow
                key={memberKey(m)}
                member={m}
                leader={!!m.name && m.name === group.leader}
              />
            ))}
          </ul>
        )}
      </div>
    </>
  );
}

function MemberRow({ member, leader }: { member: GroupMember; leader: boolean }) {
  const hp = asPct(member.hp_pct);
  const third = hp === null ? 'quiet' : thirdsTone(hp);
  const tone = third === 'quiet' ? '' : third;
  return (
    <li className={`pane-row pane-member${tone ? ` pane-member-${tone}` : ''}`}>
      <span className="pane-row-name">
        {member.name ?? 'Someone'}
        {leader && <span className="pane-member-lead">lead</span>}
      </span>
      <span className="pane-member-meter" aria-hidden="true">
        {hp !== null && <span className="pane-member-fill" style={{ width: `${hp}%` }} />}
      </span>
      <span className="pane-member-pct">{hp === null ? '' : `${hp}%`}</span>
    </li>
  );
}

function asPct(value: unknown): number | null {
  if (value === undefined || value === null || value === '') return null;
  const n = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(n)) return null;
  return Math.max(0, Math.min(100, Math.round(n)));
}
