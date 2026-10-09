import type { ReactNode } from 'react';
import { Button, PlusIcon } from '../../ui';

export type SaveStatus = 'clean' | 'dirty' | 'saved';

interface SaveBarProps {
  /** `New trigger`, or undefined for kinds you cannot add to. */
  newLabel?: string | undefined;
  onNew?: (() => void) | undefined;
  /** Another quiet action on the left, like Loadouts' Turn all off. */
  extra?: ReactNode;
  status: SaveStatus;
  /** What stopped the last Save, in red beside the buttons, until you
   *  fix it, save, or discard. */
  error?: string | undefined;
  /** The id the error line takes, so the rows it names can point at it. */
  errorId?: string | undefined;
  /** Save stays off while the page cannot save, like a JSON view that
   *  does not read. */
  canSave: boolean;
  busy: boolean;
  onDiscard: () => void;
  onSave: () => void;
}

/** The bar pinned under the Automation list and detail: 48 high with a
 *  hairline on top. `New trigger` and the save state on the left,
 *  Discard and Save on the right. The state reads `Unsaved changes`
 *  while the draft differs from the last save and `Saved` for two
 *  seconds after one. What stopped a Save sits in red just left of
 *  Discard, where your eye is when you press Save. */
export function SaveBar({
  newLabel,
  onNew,
  extra,
  status,
  error,
  errorId,
  canSave,
  busy,
  onDiscard,
  onSave,
}: SaveBarProps) {
  const dirty = status === 'dirty';
  return (
    <div className="st-savebar">
      <div className="st-savebar-side">
        {newLabel && onNew && (
          <Button icon={<PlusIcon />} onClick={onNew} disabled={busy}>
            {newLabel}
          </Button>
        )}
        {extra}
        <span className="st-savebar-status" role="status" aria-live="polite">
          {status === 'dirty' ? 'Unsaved changes' : status === 'saved' ? 'Saved' : ''}
        </span>
      </div>
      {error && (
        <p id={errorId} className="st-savebar-error" role="alert">
          {error}
        </p>
      )}
      <div className="st-savebar-actions">
        <Button onClick={onDiscard} disabled={!dirty || busy}>
          Discard
        </Button>
        <Button variant="primary" onClick={onSave} disabled={!dirty || !canSave || busy}>
          Save
        </Button>
      </div>
    </div>
  );
}
