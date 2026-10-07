import type { ComponentType } from 'react';
import { PresetFixNotice } from './PresetFixNotice';
import { Toasts } from './Toasts';
import { UpdateNotice } from './UpdateNotice';

/** The notices in the corner, top to bottom in First Run Q19's order:
 *  Get started, the reconnect notice, a preset fix, then the update
 *  notice. An item that adds a notice puts it in its slot here, so each
 *  notice takes its place in the stack with no offsets of its own. Each
 *  draws nothing while it has nothing to say. */
const SLOTS: readonly { id: string; Notice: ComponentType }[] = [
  { id: 'preset-fix', Notice: PresetFixNotice },
  { id: 'update', Notice: UpdateNotice },
];

// The corner of the terminal column, 16 in from its right edge and 16
// above the input band. The toasts stack above the notices, and the
// stack grows up as each one shows.
export function CornerNotices() {
  return (
    <div className="ov-corner">
      <Toasts />
      {SLOTS.map(({ id, Notice }) => (
        <Notice key={id} />
      ))}
    </div>
  );
}
