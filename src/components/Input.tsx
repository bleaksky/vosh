import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';
import {
  getTarget,
  getUiConfig,
  listMacros,
  onInputMode,
  onTarget,
  sendInput,
  subscribeMacrosChanged,
  type Macro,
  type QuickKey,
} from '../lib/session';
import { canonicalKeyFromEvent } from '../lib/macroKeys';
import { listen } from '@tauri-apps/api/event';

export interface InputHandle {
  focus: () => void;
}

interface Props {
  enabled: boolean;
  onError?: (message: string) => void;
  onLocalEcho?: (text: string) => void;
  /** Scroll the terminal scrollback by N pages. Called from
   *  PageUp/PageDown handling (which on macOS is Fn+Up/Fn+Down). */
  onScrollTerminal?: (pages: number) => void;
  /** Close the split-scrollback view. Fires on Esc; the host decides
   *  whether anything is currently open. */
  onExitSplit?: () => void;
}

export const Input = forwardRef<InputHandle, Props>(function Input(
  { enabled, onError, onLocalEcho, onScrollTerminal, onExitSplit }: Props,
  ref,
) {
  const [value, setValue] = useState('');
  const [history, setHistory] = useState<string[]>([]);
  const [passwordMode, setPasswordMode] = useState(false);
  // When the user starts arrow-key navigation with non-empty input, we
  // remember that prefix so Up and Down cycle only matching history entries.
  // Null means no active prefix search; cycle the full history.
  const [searchPrefix, setSearchPrefix] = useState<string | null>(null);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (enabled) inputRef.current?.focus();
  }, [enabled]);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    let cancelled = false;
    onInputMode((payload) => {
      setPasswordMode(payload.password);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // Track configured quick-keys so we can skip the local echo when
  // the user types one. The backend echoes the expansion (`bash
  // blah`) via session://output, so the shortcut itself never lands
  // in xterm — only the resolved command does.
  const quickKeysRef = useRef<QuickKey[]>([]);
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    getTarget()
      .then((snap) => {
        if (!cancelled) quickKeysRef.current = snap.quick_keys;
      })
      .catch(() => {});
    onTarget((payload) => {
      quickKeysRef.current = payload.quick_keys;
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // Keep-last-command preference. When true, after submitting a
  // line the input retains the value and selects the text so
  // pressing Enter resends. Read once on mount, refreshed via the
  // vosh://keep-last-changed event the settings save fires.
  const keepLastRef = useRef<boolean>(false);
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    getUiConfig()
      .then((cfg) => {
        if (!cancelled) keepLastRef.current = cfg.keep_last_command;
      })
      .catch(() => {});
    listen<boolean>('vosh://keep-last-changed', (event) => {
      keepLastRef.current = Boolean(event.payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Keyboard macro bindings — keyed by canonical key string
  // ("F1", "Ctrl+N", "Numpad7"). Seeded from the backend and
  // refreshed on every macros-changed broadcast.
  const macroMapRef = useRef<Map<string, string>>(new Map());
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    const apply = (list: Macro[]) => {
      const m = new Map<string, string>();
      for (const entry of list) {
        if (entry.key && entry.command) m.set(entry.key, entry.command);
      }
      macroMapRef.current = m;
    };
    listMacros()
      .then((list) => {
        if (!cancelled) apply(list);
      })
      .catch(() => {});
    subscribeMacrosChanged((list) => {
      if (!cancelled) apply(list);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      focus: () => inputRef.current?.focus(),
    }),
    [],
  );

  const matchingIndices = (prefix: string | null): number[] => {
    if (prefix === null || prefix === '') {
      return history.map((_, i) => i);
    }
    return history.flatMap((line, i) => (line.startsWith(prefix) ? [i] : []));
  };

  const startSearchIfNeeded = (): number[] => {
    if (searchPrefix === null) {
      const prefix = value;
      setSearchPrefix(prefix);
      return matchingIndices(prefix);
    }
    return matchingIndices(searchPrefix);
  };

  const handleChange = (next: string) => {
    setValue(next);
    // Any direct edit cancels the active prefix search so the next Up uses
    // the current input as the new prefix.
    if (searchPrefix !== null) {
      setSearchPrefix(null);
      setHistoryIndex(null);
    }
  };

  const handleKeyDown = async (event: KeyboardEvent<HTMLInputElement>) => {
    // Macro lookup runs first so a bound key fires its command
    // regardless of any other handler. allowPlainPrintable matches
    // what the Settings capture path uses, so a binding to a bare
    // character (e.g. "\") fires here too. The lookup is gated by
    // macroMapRef.current.get(canonical), so unbound printable keys
    // still fall through to normal typing.
    const canonical = canonicalKeyFromEvent(event, { allowPlainPrintable: true });
    if (canonical) {
      const command = macroMapRef.current.get(canonical);
      if (command) {
        event.preventDefault();
        try {
          await sendInput(command);
        } catch (e) {
          onError?.(String(e));
        }
        return;
      }
    }

    // Page-scroll the terminal scrollback. macOS sends PageUp/PageDown
    // when the user presses Fn+Up/Fn+Down. Other platforms: PageUp/
    // PageDown directly.
    if (event.key === 'PageUp' || event.key === 'PageDown') {
      event.preventDefault();
      onScrollTerminal?.(event.key === 'PageUp' ? -1 : 1);
      return;
    }

    // Esc closes the split-scrollback view if it is open. The host
    // ignores the call when nothing is split, so a stray Esc is safe.
    if (event.key === 'Escape') {
      onExitSplit?.();
      return;
    }

    // Move to start/end of the input line. macOS conventions:
    //   Cmd+Left / Cmd+Right — start / end of line
    //   Fn+Left / Fn+Right   — generate Home / End in browsers
    // Cross-platform Home/End still works.
    if (event.key === 'Home' || (event.metaKey && event.key === 'ArrowLeft')) {
      event.preventDefault();
      const el = inputRef.current;
      if (el) el.setSelectionRange(0, 0);
      return;
    }
    if (event.key === 'End' || (event.metaKey && event.key === 'ArrowRight')) {
      event.preventDefault();
      const el = inputRef.current;
      if (el) el.setSelectionRange(el.value.length, el.value.length);
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      const line = value;
      const keepLast = keepLastRef.current && line.length > 0 && !passwordMode;
      if (keepLast) {
        // Restore the line and select it after React commits so the
        // user can press Enter to resend. setSelectionRange selects
        // the whole value; the OS will paint the standard text-
        // selection highlight (overridden by .input-row input::
        // selection in styles.css to use the theme accent).
        setValue(line);
        requestAnimationFrame(() => {
          const el = inputRef.current;
          if (el) el.setSelectionRange(0, line.length);
        });
      } else {
        setValue('');
      }
      setSearchPrefix(null);
      setHistoryIndex(null);
      if (line.length > 0 && !passwordMode) {
        setHistory((prev) => {
          if (prev[prev.length - 1] === line) return prev;
          return [...prev, line];
        });
      }
      // Echo synchronously so the typed line appears the same frame the
      // user pressed Enter. The cursor sits at the end of the partial
      // prompt, so the line lands inline (TinTin++ style) and the
      // trailing \r\n moves the cursor to the row where the server
      // response will print. In password mode, only echo a newline so
      // the password itself never lands in the terminal scrollback.
      //
      // Quick-keys are a third case: skip the local echo entirely so
      // the shortcut name doesn't appear. The backend pushes the
      // expansion via session://output, which lands inline at the
      // same cursor position the shortcut would have occupied.
      const firstWord = line.split(/\s+/)[0] ?? '';
      const isQuickKey = quickKeysRef.current.some(
        (q) => q.name === firstWord && q.verb.length > 0,
      );
      if (passwordMode) {
        onLocalEcho?.('\r\n');
      } else if (!isQuickKey) {
        onLocalEcho?.(`${line}\r\n`);
      }
      try {
        await sendInput(line);
      } catch (e) {
        onError?.(String(e));
      }
      return;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      const matches = startSearchIfNeeded();
      if (matches.length === 0) return;
      const currentMatchPos =
        historyIndex === null ? matches.length : matches.indexOf(historyIndex);
      const nextPos = Math.max(0, currentMatchPos - 1);
      const next = matches[nextPos];
      if (next === undefined) return;
      setHistoryIndex(next);
      setValue(history[next] ?? '');
      return;
    }

    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (historyIndex === null) return;
      const matches = matchingIndices(searchPrefix);
      const currentMatchPos = matches.indexOf(historyIndex);
      const nextPos = currentMatchPos + 1;
      if (nextPos >= matches.length) {
        setHistoryIndex(null);
        setValue(searchPrefix ?? '');
      } else {
        const next = matches[nextPos];
        if (next === undefined) return;
        setHistoryIndex(next);
        setValue(history[next] ?? '');
      }
    }
  };

  return (
    <div className="input-row">
      <span className="prompt" aria-hidden="true">
        &gt;
      </span>
      <input
        ref={inputRef}
        type={passwordMode ? 'password' : 'text'}
        value={value}
        disabled={!enabled}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        autoComplete={passwordMode ? 'current-password' : 'off'}
        placeholder={passwordMode ? 'password' : ''}
        aria-label={passwordMode ? 'password input' : 'command input'}
        onChange={(e) => handleChange(e.target.value)}
        onKeyDown={handleKeyDown}
      />
    </div>
  );
});
