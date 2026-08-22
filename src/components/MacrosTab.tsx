import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import {
  deleteMacro,
  listMacros,
  listMacroGroups,
  setMacro,
  setMacroGroupEnabled,
  subscribeMacrosChanged,
  subscribeMacroGroupsChanged,
  type GroupState,
  type Macro,
} from '../lib/session';
import { canonicalKeyFromEvent, labelForKey } from '../lib/macroKeys';
import { Chevron } from './Icons';
import { usePersistedSet } from '../lib/usePersistedSet';

interface Props {
  onError: (e: string | null) => void;
}

const UNGROUPED_LABEL = '(no group)';

function groupKey(m: Macro): string {
  const g = m.group?.trim();
  return g && g.length > 0 ? g : '';
}

// Keyboard macro bindings. Each row is one key -> command mapping.
// The key cell is a focused "press a key" capture input; on keydown
// it records the canonical key string (allowing plain printable
// keys here so you CAN bind to "a" — at the bottom row, the input
// passes allowPlainPrintable through to canonicalKeyFromEvent).
// The command and group cells are plain text inputs. Macros are
// bucketed into collapsible group sections that share a single
// enable/disable checkbox each.
export function MacrosTab({ onError }: Props) {
  const [macros, setMacros] = useState<Macro[]>([]);
  const [loading, setLoading] = useState(true);
  const [groupStates, setGroupStates] = useState<GroupState[]>([]);
  const [collapsed, toggleCollapsed] = usePersistedSet('vosh.macros.collapsed');

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    listMacros()
      .then((list) => {
        if (!cancelled) {
          setMacros(list);
          setLoading(false);
        }
      })
      .catch((e) => {
        onError(String(e));
        setLoading(false);
      });
    subscribeMacrosChanged((list) => {
      if (!cancelled) setMacros(list);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [onError]);

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    const refresh = () => {
      void listMacroGroups()
        .then((groups) => {
          if (!cancelled) setGroupStates(groups);
        })
        .catch(() => undefined);
    };
    refresh();
    subscribeMacroGroupsChanged(() => {
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

  const handleSave = async (key: string, command: string, group: string | null) => {
    try {
      const next = await setMacro(key, command, group);
      setMacros(next);
    } catch (e) {
      onError(String(e));
    }
  };

  const handleDelete = async (key: string) => {
    try {
      const next = await deleteMacro(key);
      setMacros(next);
    } catch (e) {
      onError(String(e));
    }
  };

  // Bucket by group. Ungrouped first; named groups alphabetically.
  const buckets = new Map<string, Macro[]>();
  for (const m of macros) {
    const key = groupKey(m);
    if (!buckets.has(key)) buckets.set(key, []);
    buckets.get(key)!.push(m);
  }
  const named = Array.from(buckets.keys())
    .filter((k) => k !== '')
    .sort((a, b) => a.localeCompare(b));
  const renderOrder: string[] = [];
  if (buckets.has('')) renderOrder.push('');
  renderOrder.push(...named);
  const groupEnabledMap = new Map(groupStates.map((g) => [g.name, g.enabled]));

  return (
    <div className="macros-tab">
      {/* Local copy of the SettingsApp TabHead markup (it is not
          exported). Rows commit through their own add/save buttons,
          so the right slot says that instead of the autosave note. */}
      <div className="settings-tab-head">
        <div className="settings-pane-title">macros</div>
        <span className="settings-tab-head-spacer" />
        <span className="settings-autosave-hint">rows apply on add or save</span>
      </div>
      <div className="macros-help">
        Press a key in the &quot;key&quot; field below to capture it (e.g. F1, Ctrl+N, Numpad7). The
        command fires when that key is pressed while the input bar is focused. Commands may contain
        `;` to chain multiple actions. Set a group to bulk-toggle a folder of bindings.
      </div>
      {loading ? (
        <div className="settings-empty">loading...</div>
      ) : (
        <div className="macros-list">
          {/* The create row is pinned above the groups so a long list of
              macros never forces a scroll to add a new one. */}
          <MacroRow
            key="__new"
            initialKey=""
            initialCommand=""
            initialGroup=""
            onSave={handleSave}
            isNew
          />
          {macros.length === 0 && <div className="settings-empty">no macros bound yet</div>}
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
                    <Chevron open={!isCollapsed} />
                  </button>
                  <span className="group-section-name">
                    {isUngrouped ? UNGROUPED_LABEL : group}
                  </span>
                  <span className="group-section-count">
                    {entries.length} macro{entries.length === 1 ? '' : 's'}
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
                          void setMacroGroupEnabled(group, e.target.checked).catch((err) =>
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
                    {entries.map((m) => (
                      <MacroRow
                        key={m.key}
                        initialKey={m.key}
                        initialCommand={m.command}
                        initialGroup={m.group ?? ''}
                        onSave={handleSave}
                        onDelete={() => handleDelete(m.key)}
                      />
                    ))}
                  </div>
                )}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

interface RowProps {
  initialKey: string;
  initialCommand: string;
  initialGroup: string;
  onSave: (key: string, command: string, group: string | null) => Promise<void> | void;
  onDelete?: () => void;
  isNew?: boolean;
}

function MacroRow({ initialKey, initialCommand, initialGroup, onSave, onDelete, isNew }: RowProps) {
  const [capturedKey, setCapturedKey] = useState(initialKey);
  const [command, setCommand] = useState(initialCommand);
  const [group, setGroup] = useState(initialGroup);
  const [capturing, setCapturing] = useState(false);
  const keyInputRef = useRef<HTMLInputElement | null>(null);

  // Reset back to incoming props when the row is reused for a
  // different macro (e.g. parent list reordered).
  useEffect(() => {
    setCapturedKey(initialKey);
    setCommand(initialCommand);
    setGroup(initialGroup);
  }, [initialKey, initialCommand, initialGroup]);

  const handleKeyCapture = (event: KeyboardEvent<HTMLInputElement>) => {
    if (!capturing) return;
    const canonical = canonicalKeyFromEvent(event, { allowPlainPrintable: true });
    if (canonical) {
      event.preventDefault();
      setCapturedKey(canonical);
      setCapturing(false);
      keyInputRef.current?.blur();
    }
  };

  const handleSaveClick = async () => {
    if (!capturedKey || !command.trim()) return;
    const trimmedGroup = group.trim();
    await onSave(capturedKey, command.trim(), trimmedGroup.length > 0 ? trimmedGroup : null);
    if (isNew) {
      setCapturedKey('');
      setCommand('');
      setGroup('');
    }
  };

  const canSave =
    capturedKey !== '' &&
    command.trim() !== '' &&
    !(capturedKey === initialKey && command === initialCommand && group === initialGroup);

  return (
    <div className={`macros-row${isNew ? ' macros-row-new' : ''}`}>
      <input
        ref={keyInputRef}
        type="text"
        className="macros-key"
        readOnly
        spellCheck={false}
        value={capturing ? 'press a key...' : labelForKey(capturedKey)}
        placeholder="click + press"
        onFocus={() => setCapturing(true)}
        onBlur={() => setCapturing(false)}
        onKeyDown={handleKeyCapture}
        aria-label="key binding"
      />
      <input
        type="text"
        className="macros-command"
        spellCheck={false}
        value={command}
        placeholder="command (use ; to chain)"
        onChange={(e) => setCommand(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void handleSaveClick();
        }}
        aria-label="command"
      />
      <input
        type="text"
        className="macros-group"
        spellCheck={false}
        value={group}
        placeholder="group"
        title="optional folder; macros sharing a group can be bulk-disabled"
        onChange={(e) => setGroup(e.target.value)}
        aria-label="group"
      />
      <div className="macros-actions">
        <button
          type="button"
          className="settings-btn"
          disabled={!canSave}
          onClick={() => void handleSaveClick()}
        >
          {isNew ? 'add' : 'save'}
        </button>
        {!isNew && onDelete && (
          <button type="button" className="settings-btn settings-btn-danger" onClick={onDelete}>
            delete
          </button>
        )}
      </div>
    </div>
  );
}
