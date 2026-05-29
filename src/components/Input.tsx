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
  onGmcp,
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

  // Room characters from Room.Chars GMCP. Used as a noun source for
  // Tab completion so the user can complete combat target names
  // without typing the whole word.
  const roomCharsRef = useRef<string[]>([]);
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    onGmcp((payload) => {
      if (payload.package !== 'Room.Chars') return;
      const data = payload.data;
      if (!Array.isArray(data)) {
        roomCharsRef.current = [];
        return;
      }
      const names: string[] = [];
      for (const entry of data) {
        if (entry && typeof entry === 'object') {
          const name = (entry as { name?: unknown }).name;
          if (typeof name === 'string' && name.length > 0) {
            names.push(name);
          }
        }
      }
      roomCharsRef.current = names;
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // Tab-completion cycling state. When the user presses Tab we
  // resolve the word being typed, build a candidate list, and
  // remember the cycle so consecutive Tab presses walk through the
  // matches. Any input change other than Tab resets this so the next
  // Tab starts a fresh search.
  const tabStateRef = useRef<{
    wordStart: number;
    matches: string[];
    idx: number;
    suffixOffset: number;
  } | null>(null);

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
    // Same for the tab-completion cycle; typing anything breaks it.
    if (tabStateRef.current) {
      tabStateRef.current = null;
    }
  };

  // Build the list of completion candidates ordered by source priority:
  //   1. Unique words pulled from typed-command history, most recent first.
  //   2. Room-character names from the latest Room.Chars GMCP push.
  // Filter by case-insensitive prefix and deduplicate so the user does
  // not see the same word twice when a noun also appeared in history.
  const buildTabMatches = (prefix: string): string[] => {
    const lower = prefix.toLowerCase();
    const seen = new Set<string>();
    const matches: string[] = [];
    const consider = (word: string) => {
      if (word.length === 0) return;
      if (word.toLowerCase() === lower) return;
      if (!word.toLowerCase().startsWith(lower)) return;
      const key = word.toLowerCase();
      if (seen.has(key)) return;
      seen.add(key);
      matches.push(word);
    };
    for (let i = history.length - 1; i >= 0; i--) {
      for (const token of history[i].split(/\s+/)) {
        consider(token);
      }
    }
    for (const name of roomCharsRef.current) {
      consider(name);
    }
    return matches;
  };

  const handleTabComplete = (step: number) => {
    const el = inputRef.current;
    if (!el) return;
    const caret = el.selectionStart ?? value.length;
    const state = tabStateRef.current;
    if (state) {
      // Cycle within the existing match set.
      if (state.matches.length === 0) return;
      const next = (state.idx + step + state.matches.length) % state.matches.length;
      const match = state.matches[next];
      const before = value.slice(0, state.wordStart);
      const after = value.slice(value.length - state.suffixOffset);
      const nextValue = before + match + after;
      setValue(nextValue);
      tabStateRef.current = { ...state, idx: next };
      // Move the caret to the end of the inserted match on the next
      // tick so React has committed the value update.
      requestAnimationFrame(() => {
        const e2 = inputRef.current;
        if (!e2) return;
        const pos = before.length + match.length;
        e2.setSelectionRange(pos, pos);
      });
      return;
    }
    // Fresh completion. Walk back from caret to find the start of
    // the current word.
    let start = caret;
    while (start > 0 && /\S/.test(value[start - 1])) start -= 1;
    const prefix = value.slice(start, caret);
    if (prefix.length === 0) return;
    const matches = buildTabMatches(prefix);
    if (matches.length === 0) return;
    const idx = step >= 0 ? 0 : matches.length - 1;
    const match = matches[idx];
    const before = value.slice(0, start);
    const after = value.slice(caret);
    const nextValue = before + match + after;
    setValue(nextValue);
    tabStateRef.current = {
      wordStart: start,
      matches,
      idx,
      suffixOffset: after.length,
    };
    requestAnimationFrame(() => {
      const e2 = inputRef.current;
      if (!e2) return;
      const pos = before.length + match.length;
      e2.setSelectionRange(pos, pos);
    });
  };

  const handleKeyDown = async (event: KeyboardEvent<HTMLInputElement>) => {
    // Tab completion. Pressing Tab once builds a candidate list from
    // history words and room characters that prefix-match the word
    // being typed. Pressing Tab again cycles through the matches.
    // Any other key resets the cycle.
    if (event.key === 'Tab') {
      event.preventDefault();
      handleTabComplete(event.shiftKey ? -1 : 1);
      return;
    }
    if (tabStateRef.current) {
      tabStateRef.current = null;
    }

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
    //
    // Shift+Home / Shift+End extend the selection from the current
    // caret to the start/end of the input. Without that branch the
    // caret just collapsed and the user lost the selection.
    //
    // Long inputs that overflow horizontally need scrollLeft set
    // explicitly so the caret actually appears at the new position;
    // setSelectionRange alone moves the caret in the document but
    // does not always pan the viewport, leaving the user looking at
    // the old position until the next keystroke.
    if (event.key === 'Home' || (event.metaKey && event.key === 'ArrowLeft')) {
      event.preventDefault();
      const el = inputRef.current;
      if (!el) return;
      if (event.shiftKey) {
        const anchor = el.selectionEnd ?? 0;
        el.setSelectionRange(0, anchor, 'backward');
      } else {
        el.setSelectionRange(0, 0);
      }
      el.scrollLeft = 0;
      return;
    }
    if (event.key === 'End' || (event.metaKey && event.key === 'ArrowRight')) {
      event.preventDefault();
      const el = inputRef.current;
      if (!el) return;
      const end = el.value.length;
      if (event.shiftKey) {
        const anchor = el.selectionStart ?? end;
        el.setSelectionRange(anchor, end, 'forward');
      } else {
        el.setSelectionRange(end, end);
      }
      el.scrollLeft = el.scrollWidth;
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
        // Stay enabled even when disconnected so the user can compose
        // commands ahead of a reconnect. The backend echoes
        // [not connected] when Enter fires without a session, which
        // is friendlier than a dead input field.
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
