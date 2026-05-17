import { useEffect, useState } from 'react';

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
  const [savedAt, setSavedAt] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    load()
      .then((json) => {
        if (cancelled) return;
        try {
          const parsed = JSON.parse(json);
          setList(Array.isArray(parsed) ? parsed : []);
        } catch {
          setList([]);
        }
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
      await save(JSON.stringify(list, null, 2));
      setSavedAt(Date.now());
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
        {savedAt !== null && <span className="settings-saved">saved.</span>}
      </div>
    </div>
  );
}
