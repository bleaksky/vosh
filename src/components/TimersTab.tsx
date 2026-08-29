import { useEffect, useState, type KeyboardEvent } from 'react';
import {
  timersDelete,
  timersList,
  timersSet,
  subscribeTimersChanged,
  type Timer,
} from '../lib/session';
import { XIcon } from './Icons';

interface Props {
  onError: (e: string | null) => void;
}

// Interval timers. Each row is one recurring command: an enable toggle,
// an optional name, an interval in seconds, and the command to fire.
// Existing rows commit as you edit (the toggle instantly, text and the
// interval on blur or Enter), matching the live-save feel of the other
// settings; the pinned top row accumulates a new timer until you add it.
export function TimersTab({ onError }: Props) {
  const [timers, setTimers] = useState<Timer[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    timersList()
      .then((list) => {
        if (!cancelled) {
          setTimers(list);
          setLoading(false);
        }
      })
      .catch((e) => {
        onError(String(e));
        setLoading(false);
      });
    subscribeTimersChanged((list) => {
      if (!cancelled) setTimers(list);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [onError]);

  const save = async (
    id: number | null,
    name: string,
    intervalSecs: number,
    command: string,
    enabled: boolean,
  ) => {
    try {
      setTimers(await timersSet(id, name, intervalSecs, command, enabled));
    } catch (e) {
      onError(String(e));
    }
  };

  const remove = async (id: number) => {
    try {
      setTimers(await timersDelete(id));
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className="timers-tab">
      <div className="settings-tab-head">
        <div className="settings-pane-title">timers</div>
        <span className="settings-tab-head-spacer" />
        <span className="settings-autosave-hint">rows apply as you edit</span>
      </div>
      <div className="timers-help">
        Each timer fires its command every interval while you are connected. The interval is in
        seconds. Commands may contain `;` to chain several actions. Untick a timer to pause it
        without deleting it.
      </div>
      {loading ? (
        <div className="settings-empty">loading...</div>
      ) : (
        <div className="timers-list">
          <TimerRow key="__new" onSave={save} isNew />
          {timers.length === 0 && <div className="settings-empty">no timers yet</div>}
          {timers.map((t) => (
            <TimerRow key={t.id} timer={t} onSave={save} onRemove={() => void remove(t.id)} />
          ))}
        </div>
      )}
    </div>
  );
}

interface RowProps {
  timer?: Timer;
  onSave: (
    id: number | null,
    name: string,
    intervalSecs: number,
    command: string,
    enabled: boolean,
  ) => Promise<void> | void;
  onRemove?: () => void;
  isNew?: boolean;
}

function TimerRow({ timer, onSave, onRemove, isNew }: RowProps) {
  const [name, setName] = useState(timer?.name ?? '');
  const [interval, setInterval] = useState(String(timer?.interval_secs ?? 30));
  const [command, setCommand] = useState(timer?.command ?? '');
  const [enabled, setEnabled] = useState(timer?.enabled ?? true);

  // Re-sync when the same row is reused for a different timer (list
  // reorder or a cross-window edit).
  useEffect(() => {
    if (!timer) return;
    setName(timer.name);
    setInterval(String(timer.interval_secs));
    setCommand(timer.command);
    setEnabled(timer.enabled);
  }, [timer]);

  const intervalNum = () => Math.max(1, Math.floor(Number(interval) || 1));

  // Commit an existing row. The new row commits only through its add
  // button so a half-typed timer is not saved on every keystroke.
  const commit = (over: Partial<Timer> = {}) => {
    if (isNew || !timer) return;
    const next = {
      name: over.name ?? name,
      interval_secs: over.interval_secs ?? intervalNum(),
      command: over.command ?? command,
      enabled: over.enabled ?? enabled,
    };
    if (!next.command.trim()) return;
    void onSave(timer.id, next.name.trim(), next.interval_secs, next.command.trim(), next.enabled);
  };

  const add = () => {
    if (!command.trim()) return;
    void onSave(null, name.trim(), intervalNum(), command.trim(), enabled);
    setName('');
    setInterval('30');
    setCommand('');
    setEnabled(true);
  };

  const onEnter = (e: KeyboardEvent) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    if (isNew) add();
    else (e.currentTarget as HTMLInputElement).blur();
  };

  return (
    <div className={`timers-row${isNew ? ' timers-row-new' : ''}`}>
      <label
        className="timers-enable"
        title={enabled ? 'on; click to pause' : 'paused; click to run'}
      >
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => {
            setEnabled(e.target.checked);
            if (!isNew) commit({ enabled: e.target.checked });
          }}
          aria-label="enabled"
        />
      </label>
      <input
        type="text"
        className="timers-name"
        spellCheck={false}
        value={name}
        placeholder="name (optional)"
        onChange={(e) => setName(e.target.value)}
        onBlur={() => commit()}
        onKeyDown={onEnter}
        aria-label="timer name"
      />
      <span className="timers-interval">
        <input
          type="number"
          min={1}
          value={interval}
          onChange={(e) => setInterval(e.target.value)}
          onBlur={() => {
            setInterval(String(intervalNum()));
            commit({ interval_secs: intervalNum() });
          }}
          onKeyDown={onEnter}
          aria-label="interval in seconds"
        />
        <span className="timers-interval-unit">s</span>
      </span>
      <input
        type="text"
        className="timers-command"
        spellCheck={false}
        value={command}
        placeholder="command (use ; to chain)"
        onChange={(e) => setCommand(e.target.value)}
        onBlur={() => commit()}
        onKeyDown={onEnter}
        aria-label="command"
      />
      <div className="timers-actions">
        {isNew ? (
          <button type="button" className="settings-btn" disabled={!command.trim()} onClick={add}>
            add
          </button>
        ) : (
          <button
            type="button"
            className="timers-remove"
            onClick={onRemove}
            aria-label="remove timer"
            title="remove timer"
          >
            <XIcon />
          </button>
        )}
      </div>
    </div>
  );
}
