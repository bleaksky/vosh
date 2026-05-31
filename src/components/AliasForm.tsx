import { useEffect, useMemo, useState } from 'react';
import { UnsavedDot } from './UnsavedDot';
import { useUnsavedWarning } from '../lib/unsaved';

interface Alias {
  name: string;
  expansion: string;
  enabled: boolean;
}

interface Props {
  load: () => Promise<string>;
  save: (json: string) => Promise<number>;
  onError: (e: string | null) => void;
}

function blankAlias(): Alias {
  return { name: '', expansion: '', enabled: true };
}

// Structured editor for aliases. Single row per alias: enabled
// toggle, name, expansion, delete. Add button at the bottom appends
// a blank row.
export function AliasForm({ load, save, onError }: Props) {
  const [list, setList] = useState<Alias[] | null>(null);
  // Snapshot of the last value the backend confirmed. Drives the
  // "● unsaved" indicator; gets reset on load and after every
  // successful save so toggling a row and toggling back leaves the
  // form looking clean.
  const [baseline, setBaseline] = useState<string>('[]');
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const dirty = useMemo(() => list !== null && JSON.stringify(list) !== baseline, [list, baseline]);
  useUnsavedWarning(dirty);

  useEffect(() => {
    if (savedAt === null) return;
    const id = window.setTimeout(() => setSavedAt(null), 1500);
    return () => window.clearTimeout(id);
  }, [savedAt]);

  useEffect(() => {
    let cancelled = false;
    load()
      .then((json) => {
        if (cancelled) return;
        let parsed: Alias[] = [];
        try {
          const raw = JSON.parse(json);
          parsed = Array.isArray(raw) ? (raw as Alias[]) : [];
        } catch {
          parsed = [];
        }
        setList(parsed);
        setBaseline(JSON.stringify(parsed));
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [load, onError]);

  const update = (idx: number, patch: Partial<Alias>) => {
    if (!list) return;
    const next = list.slice();
    next[idx] = { ...next[idx], ...patch };
    setList(next);
  };

  const remove = (idx: number) => {
    if (!list) return;
    const next = list.slice();
    next.splice(idx, 1);
    setList(next);
  };

  const add = () => {
    if (!list) return;
    setList([...list, blankAlias()]);
  };

  const doSave = async () => {
    if (!list) return;
    try {
      // Wire format is pretty-printed for the textarea-driven
      // JsonTab readers, but the baseline tracks the canonical
      // (compact) form so equality stays stable even if the user
      // happens to retype to a different whitespace shape.
      await save(JSON.stringify(list, null, 2));
      setSavedAt(Date.now());
      setBaseline(JSON.stringify(list));
    } catch (e) {
      onError(String(e));
    }
  };

  if (!list) return <div className="settings-loading">loading aliases…</div>;

  return (
    <div className="alias-form">
      <div className="settings-triggers-meta">
        <span className="settings-triggers-count">
          {list.length} alias{list.length === 1 ? '' : 'es'}
        </span>
        <span className="settings-triggers-hint">
          form edits the live alias store; save replaces it atomically
        </span>
      </div>

      <div className="alias-form-list">
        {list.map((a, i) => (
          <div key={i} className="alias-row">
            <label className="alias-row-enabled">
              <input
                type="checkbox"
                checked={a.enabled}
                onChange={(e) => update(i, { enabled: e.target.checked })}
              />
            </label>
            <input
              className="alias-row-name"
              type="text"
              placeholder="name"
              value={a.name}
              onChange={(e) => update(i, { name: e.target.value })}
            />
            <input
              className="alias-row-expansion"
              type="text"
              placeholder="expansion"
              spellCheck={false}
              value={a.expansion}
              onChange={(e) => update(i, { expansion: e.target.value })}
            />
            <button type="button" className="alias-row-remove" onClick={() => remove(i)}>
              ×
            </button>
          </div>
        ))}
        {list.length === 0 && (
          <div className="settings-font-empty">no aliases yet — click [+ alias] to add one</div>
        )}
      </div>

      <div className="settings-actions">
        <button type="button" className="settings-btn" onClick={() => void doSave()}>
          [save]
        </button>
        <button type="button" className="settings-btn settings-btn-mute" onClick={add}>
          [+ alias]
        </button>
        {dirty && <UnsavedDot />}
        {savedAt !== null && <span className="settings-saved">saved.</span>}
      </div>
    </div>
  );
}
