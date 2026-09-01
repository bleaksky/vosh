import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import {
  timersDelete,
  timersList,
  timersSet,
  subscribeTimersChanged,
  type Timer,
} from '../lib/session';
import { CodeEditor } from './CodeEditor';
import { XIcon } from './Icons';

interface Props {
  onError: (e: string | null) => void;
}

// Interval timers. Each timer is a small card: an enable toggle, an
// optional name, and an interval on the top line, with the command as a
// full-width Lua editor below so multi-line `#lua` bodies fit. Existing
// cards save on a short debounce after you edit (the toggle saves at
// once); the pinned top card accumulates a new timer until you add it.
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
        seconds. The command runs like a line typed at the prompt, so `;` chains actions, `#echo`
        prints locally, and `#lua` runs a Lua body across as many lines as you need. Untick a timer
        to pause it without deleting it.
      </div>
      {loading ? (
        <div className="settings-empty">loading...</div>
      ) : (
        <div className="timers-list">
          <TimerCard key="__new" onSave={save} isNew />
          {timers.length === 0 && <div className="settings-empty">no timers yet</div>}
          {timers.map((t) => (
            <TimerCard key={t.id} timer={t} onSave={save} onRemove={() => void remove(t.id)} />
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

function TimerCard({ timer, onSave, onRemove, isNew }: RowProps) {
  const [name, setName] = useState(timer?.name ?? '');
  const [interval, setInterval] = useState(String(timer?.interval_secs ?? 30));
  const [command, setCommand] = useState(timer?.command ?? '');
  const [enabled, setEnabled] = useState(timer?.enabled ?? true);
  const saveTimer = useRef<number | null>(null);

  // Re-sync when the same card is reused for a different timer (list
  // reorder or a cross-window edit).
  useEffect(() => {
    if (!timer) return;
    setName(timer.name);
    setInterval(String(timer.interval_secs));
    setCommand(timer.command);
    setEnabled(timer.enabled);
  }, [timer]);

  useEffect(
    () => () => {
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
    },
    [],
  );

  const intervalNum = () => Math.max(1, Math.floor(Number(interval) || 1));

  // Commit an existing card. Fields debounce so typing does not save on
  // every keystroke; the enable toggle passes debounce=false to save at
  // once. The new card never auto-saves — it commits through its add
  // button so a half-written timer is not stored.
  const commit = (over: Partial<Timer> = {}, debounce = true) => {
    if (isNew || !timer) return;
    const run = () => {
      const nextCommand = over.command ?? command;
      if (!nextCommand.trim()) return;
      void onSave(
        timer.id,
        (over.name ?? name).trim(),
        over.interval_secs ?? intervalNum(),
        nextCommand.trim(),
        over.enabled ?? enabled,
      );
    };
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
    if (debounce) saveTimer.current = window.setTimeout(run, 400);
    else run();
  };

  const add = () => {
    if (!command.trim()) return;
    void onSave(null, name.trim(), intervalNum(), command.trim(), enabled);
    setName('');
    setInterval('30');
    setCommand('');
    setEnabled(true);
  };

  // Enter in the name / interval inputs commits (existing) or adds
  // (new); it never reaches the command editor, where Enter is a
  // newline for multi-line Lua.
  const onFieldEnter = (e: KeyboardEvent) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    if (isNew) add();
    else commit({}, false);
  };

  return (
    <div className={`timers-card${isNew ? ' timers-card-new' : ''}`}>
      <div className="timers-card-head">
        <label
          className="timers-enable"
          title={enabled ? 'on; click to pause' : 'paused; click to run'}
        >
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => {
              setEnabled(e.target.checked);
              if (!isNew) commit({ enabled: e.target.checked }, false);
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
          onKeyDown={onFieldEnter}
          aria-label="timer name"
        />
        <span className="timers-interval">
          <span className="timers-interval-label">every</span>
          <input
            type="number"
            min={1}
            value={interval}
            onChange={(e) => setInterval(e.target.value)}
            onBlur={() => {
              setInterval(String(intervalNum()));
              commit({ interval_secs: intervalNum() });
            }}
            onKeyDown={onFieldEnter}
            aria-label="interval in seconds"
          />
          <span className="timers-interval-unit">s</span>
        </span>
        <div className="timers-card-actions">
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
      <CodeEditor
        className="timers-command"
        ariaLabel="timer command"
        language="lua"
        inline
        minHeight="1.7em"
        maxHeight="40vh"
        placeholder="command — e.g. sip health   ·   #echo rest   ·   #lua mud.send('cast heal')"
        value={command}
        onChange={(next) => {
          setCommand(next);
          commit({ command: next });
        }}
      />
    </div>
  );
}
