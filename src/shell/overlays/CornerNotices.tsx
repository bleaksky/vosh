import type { ComponentType, ReactNode } from 'react';
import { AlertNotice } from './AlertNotice';
import { GetStartedNotice } from './GetStartedNotice';
import { PresetFixNotice } from './PresetFixNotice';
import { Toasts } from './Toasts';
import { UpdateNotice } from './UpdateNotice';

/** The notices in the corner, top to bottom. First Run Q19 stacks them
 *  from the command line up: the update notice keeps its spot at the
 *  bottom, Get started stands above it, a preset fix above that, then
 *  the notice of an alert from a session behind (Sessions Q10), and the
 *  reconnect notice goes on top, over the alert notice, since it comes
 *  and goes with the link and nothing under it moves. MainWindow hands
 *  the reconnect notice in, since it needs the selected session. Each
 *  sits 52 px over the one below while both are up. An item that adds a
 *  notice puts it in its slot here, so each notice takes its place in
 *  the stack with no offsets of its own. Each draws nothing while it has
 *  nothing to say. */
const SLOTS: readonly { id: string; Notice: ComponentType }[] = [
  { id: 'alert', Notice: AlertNotice },
  { id: 'preset-fix', Notice: PresetFixNotice },
  { id: 'get-started', Notice: GetStartedNotice },
  { id: 'update', Notice: UpdateNotice },
];

// The corner of the terminal column, 16 in from its right edge and 16
// above the input band. The toasts stack above the notices, and the
// stack grows up as each one shows.
export function CornerNotices({ reconnect }: { reconnect?: ReactNode }) {
  return (
    <div className="ov-corner">
      <Toasts />
      {reconnect}
      {SLOTS.map(({ id, Notice }) => (
        <Notice key={id} />
      ))}
    </div>
  );
}
