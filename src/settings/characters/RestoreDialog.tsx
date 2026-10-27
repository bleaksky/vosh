import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { profileBackups, type ProfileBackup } from '../../ipc/characters';
import { loadoutsGetState } from '../../ipc/loadouts';
import { profileDisplayName } from '../../lib/characterProfiles';
import { CheckIcon, cx } from '../../ui';
import { ConfirmDialog } from '../../ui/ConfirmDialog';
import { backupTime } from './backupTime';

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
  const [failed, setFailed] = useState<string | null>(null);
  const rows = useRef<(HTMLButtonElement | null)[]>([]);
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
      (e) => {
        if (cancelled) return;
        setBackups([]);
        setFailed(String(e));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [name]);

  // The arrows move the pick through the list, as in a radio group.
  const onRowKey = (e: KeyboardEvent<HTMLButtonElement>, at: number) => {
    if (!backups) return;
    const step =
      e.key === 'ArrowDown' || e.key === 'ArrowRight'
        ? 1
        : e.key === 'ArrowUp' || e.key === 'ArrowLeft'
          ? -1
          : 0;
    if (step === 0) return;
    e.preventDefault();
    const next = (at + step + backups.length) % backups.length;
    const row = backups[next];
    if (!row) return;
    setPicked(row.file);
    rows.current[next]?.focus();
  };

  const body =
    failed !== null
      ? failed
      : backups !== null && backups.length === 0
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
          {backups.map((b, at) => {
            const on = b.file === picked;
            return (
              <button
                key={b.file}
                ref={(el) => {
                  rows.current[at] = el;
                }}
                type="button"
                role="radio"
                aria-checked={on}
                tabIndex={on ? 0 : -1}
                className={cx('ov-pick-row', on && 'is-on')}
                onClick={() => setPicked(b.file)}
                onKeyDown={(e) => onRowKey(e, at)}
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
