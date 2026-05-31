import { useEffect, useMemo, useState } from 'react';
import { UnsavedDot } from './UnsavedDot';
import { useUnsavedWarning } from '../lib/unsaved';
import {
  listAliasGroups,
  setAliasGroupEnabled,
  subscribeAliasGroupsChanged,
  type GroupState,
} from '../lib/session';

interface Alias {
  name: string;
  expansion: string;
  enabled: boolean;
  group?: string | null;
}

interface Props {
  load: () => Promise<string>;
  save: (json: string) => Promise<number>;
  onError: (e: string | null) => void;
}

const UNGROUPED_LABEL = '(no group)';

function blankAlias(group: string | null = null): Alias {
  return { name: '', expansion: '', enabled: true, group };
}

function groupKey(a: Alias): string {
  const g = a.group?.trim();
  return g && g.length > 0 ? g : '';
}

// Structured editor for aliases. Rows are sorted into collapsible
// group sections so a user can pile a "Combat" loadout into one
// folder and bulk-toggle it with a single checkbox without losing
// each row's individual enabled flag. Ungrouped aliases live at the
// top of the list in a "(no group)" section that has no toggle.
export function AliasForm({ load, save, onError }: Props) {
  const [list, setList] = useState<Alias[] | null>(null);
  // Snapshot of the last value the backend confirmed. Drives the
  // "● unsaved" indicator; gets reset on load and after every
  // successful save so toggling a row and toggling back leaves the
  // form looking clean.
  const [baseline, setBaseline] = useState<string>('[]');
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const [groupStates, setGroupStates] = useState<GroupState[]>([]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
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

  // Pull the group-enabled state from the backend on mount + on
  // every cross-window change. Source of truth lives in the backend
  // AliasStore so a `#alias` slash command or a settings-window
  // toggle keeps every viewer in sync.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    const refresh = () => {
      void listAliasGroups()
        .then((groups) => {
          if (!cancelled) setGroupStates(groups);
        })
        .catch(() => undefined);
    };
    refresh();
    subscribeAliasGroupsChanged(() => {
      if (!cancelled) refresh();
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

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

  const add = (group: string | null = null) => {
    if (!list) return;
    setList([...list, blankAlias(group)]);
  };

  const doSave = async () => {
    if (!list) return;
    try {
      await save(JSON.stringify(list, null, 2));
      setSavedAt(Date.now());
      setBaseline(JSON.stringify(list));
    } catch (e) {
      onError(String(e));
    }
  };

  const toggleCollapsed = (group: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  };

  if (!list) return <div className="settings-loading">loading aliases…</div>;

  // Build the bucket map: group name -> [index in list, alias]. Empty
  // string keys the ungrouped bucket. Preserves original list order
  // within each bucket so saved offsets are predictable.
  const buckets = new Map<string, Array<{ index: number; alias: Alias }>>();
  list.forEach((alias, index) => {
    const key = groupKey(alias);
    if (!buckets.has(key)) buckets.set(key, []);
    buckets.get(key)!.push({ index, alias });
  });
  // Render order: ungrouped first, then named groups alphabetically.
  const groupNames = Array.from(buckets.keys())
    .filter((k) => k !== '')
    .sort((a, b) => a.localeCompare(b));
  const renderOrder: string[] = [];
  if (buckets.has('')) renderOrder.push('');
  renderOrder.push(...groupNames);

  const groupEnabledMap = new Map(groupStates.map((g) => [g.name, g.enabled]));

  return (
    <div className="alias-form">
      <div className="settings-triggers-meta">
        <span className="settings-triggers-count">
          {list.length} alias{list.length === 1 ? '' : 'es'} in {renderOrder.length} group
          {renderOrder.length === 1 ? '' : 's'}
        </span>
        <span className="settings-triggers-hint">
          set a group to bulk-toggle a folder of aliases; ungrouped rows live in the top section
        </span>
      </div>

      <div className="alias-form-list">
        {renderOrder.map((group) => {
          const entries = buckets.get(group) ?? [];
          const isUngrouped = group === '';
          const groupEnabled = isUngrouped ? true : (groupEnabledMap.get(group) ?? true);
          const isCollapsed = collapsed.has(group);
          return (
            <section
              key={isUngrouped ? '__ungrouped__' : group}
              className={`group-section${groupEnabled ? '' : ' group-disabled'}`}
            >
              <header className="group-section-head">
                <button
                  type="button"
                  className="group-section-collapse"
                  onClick={() => toggleCollapsed(group)}
                  aria-expanded={!isCollapsed}
                  title={isCollapsed ? 'expand group' : 'collapse group'}
                >
                  {isCollapsed ? '▸' : '▾'}
                </button>
                <span className="group-section-name">{isUngrouped ? UNGROUPED_LABEL : group}</span>
                <span className="group-section-count">
                  {entries.length} alias{entries.length === 1 ? '' : 'es'}
                </span>
                {!isUngrouped && (
                  <label
                    className="group-section-toggle"
                    title={
                      groupEnabled
                        ? 'group is on; click to disable'
                        : 'group is off; click to enable'
                    }
                  >
                    <input
                      type="checkbox"
                      checked={groupEnabled}
                      onChange={(e) =>
                        void setAliasGroupEnabled(group, e.target.checked).catch((err) =>
                          onError(String(err)),
                        )
                      }
                    />
                    enabled
                  </label>
                )}
              </header>
              {!isCollapsed && (
                <div className="group-section-body">
                  {entries.map(({ index, alias }) => (
                    <div key={index} className="alias-row">
                      <label className="alias-row-enabled">
                        <input
                          type="checkbox"
                          checked={alias.enabled}
                          onChange={(e) => update(index, { enabled: e.target.checked })}
                        />
                      </label>
                      <input
                        className="alias-row-name"
                        type="text"
                        placeholder="name"
                        value={alias.name}
                        onChange={(e) => update(index, { name: e.target.value })}
                      />
                      <input
                        className="alias-row-expansion"
                        type="text"
                        placeholder="expansion"
                        spellCheck={false}
                        value={alias.expansion}
                        onChange={(e) => update(index, { expansion: e.target.value })}
                      />
                      <input
                        className="alias-row-group"
                        type="text"
                        placeholder="group"
                        title="optional folder; aliases sharing a group can be bulk-disabled"
                        value={alias.group ?? ''}
                        onChange={(e) =>
                          update(index, {
                            group: e.target.value.trim().length > 0 ? e.target.value : null,
                          })
                        }
                      />
                      <button
                        type="button"
                        className="alias-row-remove"
                        onClick={() => remove(index)}
                      >
                        ×
                      </button>
                    </div>
                  ))}
                  {entries.length === 0 && (
                    <div className="settings-font-empty">empty group — delete or add a row</div>
                  )}
                </div>
              )}
            </section>
          );
        })}
        {list.length === 0 && (
          <div className="settings-font-empty">no aliases yet — click [+ alias] to add one</div>
        )}
      </div>

      <div className="settings-actions">
        <button type="button" className="settings-btn" onClick={() => void doSave()}>
          [save]
        </button>
        <button type="button" className="settings-btn settings-btn-mute" onClick={() => add(null)}>
          [+ alias]
        </button>
        {dirty && <UnsavedDot />}
        {savedAt !== null && <span className="settings-saved">saved.</span>}
      </div>
    </div>
  );
}
