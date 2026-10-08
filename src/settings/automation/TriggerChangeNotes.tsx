import type { ReactNode } from 'react';
import { countPhrase } from '../../automation/automationDraft';
import type { TriggerRecord } from '../../ipc/automation';
import { PencilIcon } from '../../ui';
import { ADVANCED_ROWS, alsoChanges, morePatternsDiffer, type FromPreset } from './triggerChanged';

const CHANGE_NOUN = { one: 'change', many: 'changes' };

/** How a changed row says what the preset has. */
export function Changed({ children }: { children: ReactNode }) {
  return <span className="st-auto-changed">Changed. The preset has {children}.</span>;
}

/** A text row's field with the line under it that says what the preset
 *  has, once you changed it. */
export function UnderField({ changed, children }: { changed: ReactNode; children: ReactNode }) {
  if (changed === null) return children;
  return (
    <div className="st-auto-stack">
      {children}
      <p className="st-auto-under">{changed}</p>
    </div>
  );
}

/** The rows under Advanced that differ from the preset, the count a
 *  closed Advanced shows beside the pencil so no edit hides there. */
export function AdvancedCount({ t, from }: { t: TriggerRecord; from: FromPreset | null }) {
  if (!from) return null;
  const rows = ADVANCED_ROWS.filter((key) => Object.hasOwn(from.changed, key)).length;
  const count = rows + (morePatternsDiffer(t, from.ship) ? 1 : 0) + alsoChanges(t, from.ship);
  if (count === 0) return null;
  return (
    <span className="st-auto-count">
      <PencilIcon size={12} />
      {countPhrase(count, CHANGE_NOUN)}
    </span>
  );
}
