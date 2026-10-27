import { useEffect, useState } from 'react';
import { profileBackups, type ProfileBackup } from '../../ipc/characters';
import { loadoutsGetState } from '../../ipc/loadouts';
import { profileDisplayName } from '../../lib/characterProfiles';
import { CheckIcon, cx } from '../../ui';
import { ConfirmDialog } from '../../ui/ConfirmDialog';

// Restore from a backup… on a profile's menu: the confirm recipe in the
// primary tone with the backups Vosh kept, newest first, one picked.
// What you have now becomes a backup first, so a restore can be undone
// by restoring that.

interface Props {
  /** The profile, raw like `default`. */
  name: string;
  onRestore: (file: string) => void;
  onCancel: () => void;
}

const DAY = 86_400_000;

/** "Today at 14:02", "Yesterday at 22:15" or "Oct 24 at 09:41". */
export function backupTime(ms: number, now: number = Date.now()): string {
  const at = new Date(ms);
  const time = at.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  const start = new Date(now);
  start.setHours(0, 0, 0, 0);
  if (ms >= start.getTime()) return `Today at ${time}`;
  if (ms >= start.getTime() - DAY) return `Yesterday at ${time}`;
  const day = at.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  return `${day} at ${time}`;
}

function counts(b: ProfileBackup, loadouts: boolean): string {
  const n = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
  if (loadouts) return n(b.timers, 'timer', 'timers');
  return [
    n(b.triggers, 'trigger', 'triggers'),
    n(b.aliases, 'alias', 'aliases'),
    n(b.macros, 'macro', 'macros'),
  ].join(', ');
}

export function RestoreDialog({ name, onRestore, onCancel }: Props) {
  const [backups, setBackups] = useState<ProfileBackup[] | null>(null);
  const [loadouts, setLoadouts] = useState(false);
  const [picked, setPicked] = useState<string | null>(null);
  const display = profileDisplayName(name);

  useEffect(() => {
    let cancelled = false;
    loadoutsGetState(name).then(
      (state) => {
        if (!cancelled) setLoadouts(state.path_b_active);
      },
      () => undefined,
    );
    profileBackups(name).then(
      (list) => {
        if (cancelled) return;
        setBackups(list);
        setPicked(list[0]?.file ?? null);
      },
      () => {
        if (!cancelled) setBackups([]);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [name]);

  const body =
    backups !== null && backups.length === 0
      ? `Vosh has no backup of ${display} yet. It keeps one each time a change saves.`
      : loadouts
        ? `Vosh puts back the timers, switches and layouts of ${display} as they were. Your triggers, aliases and macros live in the shared catalog and stay as they are. What you have now becomes a backup first.`
        : `Vosh puts ${display} back as it was. What you have now becomes a backup first, so you can come back to it.`;

  return (
    <ConfirmDialog
      title={`Restore ${display} from a backup?`}
      body={body}
      confirmLabel="Restore"
      tone="primary"
      confirmDisabled={picked === null}
      onConfirm={() => picked && onRestore(picked)}
      onCancel={onCancel}
    >
      {backups && backups.length > 0 && (
        <div className="ov-pick-list" role="radiogroup" aria-label="Backups">
          {backups.map((b) => {
            const on = b.file === picked;
            return (
              <button
                key={b.file}
                type="button"
                role="radio"
                aria-checked={on}
                tabIndex={on ? 0 : -1}
                className={cx('ov-pick-row', on && 'is-on')}
                onClick={() => setPicked(b.file)}
              >
                <span className="ov-pick-check" aria-hidden="true">
                  {on && <CheckIcon size={12} />}
                </span>
                <span className="ov-pick-text">
                  <span className="ov-pick-name">{backupTime(b.saved_ms)}</span>
                  <span className="ov-pick-detail">{counts(b, loadouts)}</span>
                </span>
              </button>
            );
          })}
        </div>
      )}
    </ConfirmDialog>
  );
}
