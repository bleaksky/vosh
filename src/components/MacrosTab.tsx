import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import {
  deleteMacro,
  listMacros,
  setMacro,
  subscribeMacrosChanged,
  type Macro,
} from '../lib/session';
import { canonicalKeyFromEvent, labelForKey } from '../lib/macroKeys';

interface Props {
  onError: (e: string | null) => void;
}

// Keyboard macro bindings. Each row is one key -> command mapping.
// The key cell is a focused "press a key" capture input; on keydown
// it records the canonical key string (allowing plain printable
// keys here so you CAN bind to "a" — at the bottom row, the input
// passes allowPlainPrintable through to canonicalKeyFromEvent).
// The command cell is a plain text input. Save round-trips the
// whole list to the backend, which persists to profile.toml.
export function MacrosTab({ onError }: Props) {
  const [macros, setMacros] = useState<Macro[]>([]);
  const [loading, setLoading] = useState(true);

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

  const handleSave = async (key: string, command: string) => {
    try {
      const next = await setMacro(key, command);
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

  return (
    <div className="macros-tab">
      <div className="macros-help">
        Press a key in the &quot;key&quot; field below to capture it
        (e.g. F1, Ctrl+N, Numpad7). The command fires when that key
        is pressed while the input bar is focused. Commands may
        contain `;` to chain multiple actions.
      </div>
      {loading ? (
        <div className="settings-font-empty">loading...</div>
      ) : (
        <div className="macros-list">
          {macros.length === 0 && (
            <div className="settings-font-empty">no macros bound yet</div>
          )}
          {macros.map((m) => (
            <MacroRow
              key={m.key}
              initialKey={m.key}
              initialCommand={m.command}
              onSave={handleSave}
              onDelete={() => handleDelete(m.key)}
            />
          ))}
          <MacroRow
            key="__new"
            initialKey=""
            initialCommand=""
            onSave={handleSave}
            isNew
          />
        </div>
      )}
    </div>
  );
}

interface RowProps {
  initialKey: string;
  initialCommand: string;
  onSave: (key: string, command: string) => Promise<void> | void;
  onDelete?: () => void;
  isNew?: boolean;
}

function MacroRow({ initialKey, initialCommand, onSave, onDelete, isNew }: RowProps) {
  const [capturedKey, setCapturedKey] = useState(initialKey);
  const [command, setCommand] = useState(initialCommand);
  const [capturing, setCapturing] = useState(false);
  const keyInputRef = useRef<HTMLInputElement | null>(null);

  // Reset back to incoming props when the row is reused for a
  // different macro (e.g. parent list reordered).
  useEffect(() => {
    setCapturedKey(initialKey);
    setCommand(initialCommand);
  }, [initialKey, initialCommand]);

  const handleKeyCapture = (event: KeyboardEvent<HTMLInputElement>) => {
    if (!capturing) return;
    // Allow plain printable keys so the user can bind "a" / "1" /
    // "?" if they want — they explicitly clicked the capture field
    // and they know what they are doing.
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
    await onSave(capturedKey, command.trim());
    if (isNew) {
      // Clear the new-row inputs after a successful save so the
      // next macro can be captured.
      setCapturedKey('');
      setCommand('');
    }
  };

  const canSave =
    capturedKey !== '' &&
    command.trim() !== '' &&
    !(capturedKey === initialKey && command === initialCommand);

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
      <div className="macros-actions">
        <button
          type="button"
          className="settings-btn"
          disabled={!canSave}
          onClick={() => void handleSaveClick()}
        >
          {isNew ? '[add]' : '[save]'}
        </button>
        {!isNew && onDelete && (
          <button
            type="button"
            className="settings-btn settings-btn-danger"
            onClick={onDelete}
          >
            [delete]
          </button>
        )}
      </div>
    </div>
  );
}
